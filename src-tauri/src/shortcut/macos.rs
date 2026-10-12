use super::detect::ModifierKind;
use crate::state::AppError;
use tauri::AppHandle;

pub fn is_supported() -> bool {
    cfg!(target_os = "macos")
}

pub struct PlatformHandle {
    #[cfg(target_os = "macos")]
    _trigger: Option<std::sync::mpsc::SyncSender<()>>,
}

pub fn start(
    app: &AppHandle,
    kind: ModifierKind,
    on_trigger: impl Fn() + Send + Sync + 'static,
) -> Result<(PlatformHandle, Option<String>), AppError> {
    #[cfg(target_os = "macos")]
    {
        native::start(app, kind, on_trigger)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, kind, on_trigger);
        Err(AppError::Shortcut("当前平台不是 macOS".into()))
    }
}

pub fn stop(app: &AppHandle, handle: PlatformHandle) {
    #[cfg(target_os = "macos")]
    native::stop(app, handle);
    #[cfg(not(target_os = "macos"))]
    let _ = (app, handle);
}

#[cfg(target_os = "macos")]
mod native {
    use super::{ModifierKind, PlatformHandle};
    use crate::shortcut::detect::{DoubleTapDetector, KeyClass};
    use crate::state::AppError;
    use block2::RcBlock;
    use core_foundation::base::TCFType;
    use core_foundation::boolean::CFBoolean;
    use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
    use core_foundation::mach_port::CFMachPortRef;
    use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};
    use core_foundation::string::{CFString, CFStringRef};
    use core_graphics::event::{
        CallbackResult, CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions,
        CGEventTapPlacement, CGEventType, EventField, KeyCode,
    };
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSEvent, NSEventMask, NSEventModifierFlags, NSEventType};
    use objc2_foundation::{ns_string, NSActivityOptions, NSProcessInfo};
    use std::cell::RefCell;
    use std::ptr::NonNull;
    use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
    use std::sync::{mpsc, Arc, Mutex};
    use std::time::{Duration, Instant};
    use tauri::{AppHandle, Manager};

    static GENERATION: AtomicU64 = AtomicU64::new(0);
    static ACTIVITY_STARTED: AtomicBool = AtomicBool::new(false);
    static TAP_PORT: AtomicUsize = AtomicUsize::new(0);
    static LISTEN_READY: AtomicBool = AtomicBool::new(false);

    /// 主循环起来之后再确认几次。启动瞬间 AXIsProcessTrusted 会误报未授权。
    const TRUST_RETRIES_BEFORE_PROMPT: u32 = 8;

    struct Installed {
        monitors: Vec<Retained<AnyObject>>,
        tap: Option<CGEventTap<'static>>,
    }

    thread_local! {
        static INSTALLED: RefCell<Installed> = const {
            RefCell::new(Installed {
                monitors: Vec::new(),
                tap: None,
            })
        };
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
        fn CGEventTapIsEnabled(tap: CFMachPortRef) -> bool;
    }

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
        fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;
        static kAXTrustedCheckOptionPrompt: CFStringRef;
    }

    const ACCESSIBILITY_HINT: &str =
        "请在系统设置 › 隐私与安全性 › 辅助功能 中允许 Quka，然后无需重启即可双击呼出";

    fn is_trusted() -> bool {
        unsafe { AXIsProcessTrusted() }
    }

    fn should_prompt_for_trust(trusted: bool, already_prompted: bool, attempt: u32) -> bool {
        !trusted && !already_prompted && attempt >= TRUST_RETRIES_BEFORE_PROMPT
    }

    fn request_accessibility_access() -> bool {
        if is_trusted() {
            return true;
        }
        unsafe {
            let key = CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt);
            let options = CFDictionary::from_CFType_pairs(&[(key, CFBoolean::true_value())]);
            AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef())
        }
    }

    fn listen_healthy() -> bool {
        if !LISTEN_READY.load(Ordering::SeqCst) {
            return false;
        }
        let port = TAP_PORT.load(Ordering::SeqCst);
        if port == 0 {
            return true;
        }
        unsafe { CGEventTapIsEnabled(port as CFMachPortRef) }
    }

    fn prevent_app_nap() {
        if ACTIVITY_STARTED.swap(true, Ordering::SeqCst) {
            return;
        }
        let activity = NSProcessInfo::processInfo().beginActivityWithOptions_reason(
            NSActivityOptions::UserInteractive,
            ns_string!("Quka global shortcut"),
        );
        std::mem::forget(activity);
    }

    fn run_sync_on_main<T: Send + 'static>(
        app: &AppHandle,
        work: impl FnOnce() -> T + Send + 'static,
    ) -> Result<T, AppError> {
        if MainThreadMarker::new().is_some() {
            return Ok(work());
        }
        let (tx, rx) = mpsc::channel();
        app.run_on_main_thread(move || {
            let _ = tx.send(work());
        })
        .map_err(|error| AppError::Shortcut(error.to_string()))?;
        rx.recv()
            .map_err(|error| AppError::Shortcut(error.to_string()))
    }

    pub fn start(
        app: &AppHandle,
        kind: ModifierKind,
        on_trigger: impl Fn() + Send + Sync + 'static,
    ) -> Result<(PlatformHandle, Option<String>), AppError> {
        let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
        let detector = Arc::new(Mutex::new(DoubleTapDetector::new(kind)));
        let (trigger_tx, trigger_rx) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("quka-shortcut-fire".into())
            .spawn(move || {
                while trigger_rx.recv().is_ok() {
                    on_trigger();
                }
            })
            .map_err(|error| AppError::Shortcut(format!("快捷键注册失败：{error}")))?;

        // 不要在这里弹辅助功能授权框。启动瞬间权限检查会误报失败，
        // 一旦弹出系统框，当前进程的全局监听会一直失效，直到再开一个进程。
        let trusted = is_trusted();
        let installed = run_sync_on_main(app, {
            let detector = detector.clone();
            let trigger_tx = trigger_tx.clone();
            move || install(generation, detector, trigger_tx)
        })?;
        spawn_listen_keeper(app.clone(), generation, detector, trigger_tx.clone());

        let warning = if installed.global || !trusted {
            None
        } else {
            Some("全局快捷键未能挂到主线程，请重启 Quka".into())
        };
        if let Some(warning) = &warning {
            log::warn!("{warning}");
        } else if installed.global {
            log::info!("快捷键监听已挂到主线程（其它应用前台也可触发）");
        } else {
            log::info!("辅助功能权限尚未确认，主循环启动后会重试，先不弹出系统授权框");
        }

        Ok((
            PlatformHandle {
                _trigger: Some(trigger_tx),
            },
            warning,
        ))
    }

    pub fn stop(app: &AppHandle, handle: PlatformHandle) {
        GENERATION.fetch_add(1, Ordering::SeqCst);
        let _ = run_sync_on_main(app, uninstall);
        drop(handle);
    }

    struct InstallResult {
        global: bool,
    }

    fn install(
        generation: u64,
        detector: Arc<Mutex<DoubleTapDetector>>,
        trigger_tx: mpsc::SyncSender<()>,
    ) -> InstallResult {
        uninstall();
        prevent_app_nap();

        let mask = NSEventMask::KeyDown | NSEventMask::KeyUp | NSEventMask::FlagsChanged;
        let mut monitors = Vec::new();

        let local_detector = detector.clone();
        let local_tx = trigger_tx.clone();
        let local_block = RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
            handle_ns_event(generation, &local_detector, &local_tx, unsafe {
                event.as_ref()
            });
            event.as_ptr()
        });
        if let Some(monitor) =
            unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(mask, &local_block) }
        {
            monitors.push(monitor);
            log::info!("已注册本应用按键监听");
        }

        let global_detector = detector.clone();
        let global_tx = trigger_tx.clone();
        let global_block = RcBlock::new(move |event: NonNull<NSEvent>| {
            handle_ns_event(generation, &global_detector, &global_tx, unsafe {
                event.as_ref()
            });
        });
        let global = if let Some(monitor) =
            NSEvent::addGlobalMonitorForEventsMatchingMask_handler(mask, &global_block)
        {
            monitors.push(monitor);
            log::info!("已注册其它应用按键监听");
            true
        } else {
            log::warn!("其它应用按键监听不可用，需要辅助功能权限");
            false
        };

        let tap = install_main_tap(generation, detector, trigger_tx);
        let has_tap = tap.is_some();
        if has_tap {
            log::info!("已在主线程启用 HID 快捷键监听");
        }

        INSTALLED.with(|slot| {
            let mut installed = slot.borrow_mut();
            installed.monitors = monitors;
            installed.tap = tap;
        });

        let ready = global || has_tap;
        LISTEN_READY.store(ready, Ordering::SeqCst);
        InstallResult { global: ready }
    }

    fn uninstall() {
        TAP_PORT.store(0, Ordering::SeqCst);
        LISTEN_READY.store(false, Ordering::SeqCst);
        INSTALLED.with(|slot| {
            let mut installed = slot.borrow_mut();
            for monitor in installed.monitors.drain(..) {
                unsafe { NSEvent::removeMonitor(&monitor) };
            }
            installed.tap = None;
        });
    }

    fn install_main_tap(
        generation: u64,
        detector: Arc<Mutex<DoubleTapDetector>>,
        trigger_tx: mpsc::SyncSender<()>,
    ) -> Option<CGEventTap<'static>> {
        let port_slot = Arc::new(AtomicUsize::new(0));
        let tap = create_tap(
            CGEventTapLocation::HID,
            generation,
            detector,
            trigger_tx,
            port_slot.clone(),
        )
        .ok()?;
        let port = tap.mach_port().as_concrete_TypeRef() as usize;
        port_slot.store(port, Ordering::SeqCst);
        let loop_source = tap.mach_port().create_runloop_source(0).ok()?;
        CFRunLoop::get_main().add_source(&loop_source, unsafe { kCFRunLoopCommonModes });
        tap.enable();
        TAP_PORT.store(port, Ordering::SeqCst);
        Some(tap)
    }

    fn spawn_listen_keeper(
        app: AppHandle,
        generation: u64,
        detector: Arc<Mutex<DoubleTapDetector>>,
        trigger_tx: mpsc::SyncSender<()>,
    ) {
        std::thread::Builder::new()
            .name("quka-shortcut-trust".into())
            .spawn(move || {
                let mut prompted = false;
                let mut attempt = 0u32;
                loop {
                    if attempt > 0 {
                        let delay = if attempt < TRUST_RETRIES_BEFORE_PROMPT {
                            Duration::from_millis(250)
                        } else {
                            Duration::from_secs(1)
                        };
                        std::thread::sleep(delay);
                    }
                    if GENERATION.load(Ordering::SeqCst) != generation {
                        return;
                    }
                    let trusted = run_sync_on_main(&app, move || {
                        if GENERATION.load(Ordering::SeqCst) != generation {
                            return false;
                        }
                        is_trusted()
                    })
                    .unwrap_or(false);
                    if GENERATION.load(Ordering::SeqCst) != generation {
                        return;
                    }
                    attempt = attempt.saturating_add(1);

                    if !trusted {
                        if should_prompt_for_trust(false, prompted, attempt) {
                            prompted = true;
                            log::info!("仍未获得辅助功能权限，请求系统授权");
                            let _ = run_sync_on_main(&app, request_accessibility_access);
                            if !is_trusted() {
                                if let Some(state) =
                                    app.try_state::<crate::shortcut::ShortcutState>()
                                {
                                    state.set_error(ACCESSIBILITY_HINT);
                                }
                            }
                        }
                        if attempt >= TRUST_RETRIES_BEFORE_PROMPT + 180 {
                            return;
                        }
                        continue;
                    }

                    remount_if_needed(&app, generation, &detector, &trigger_tx);
                    note_listen_state(&app);
                    break;
                }

                loop {
                    std::thread::sleep(Duration::from_secs(2));
                    if GENERATION.load(Ordering::SeqCst) != generation {
                        return;
                    }
                    let needs_remount = run_sync_on_main(&app, move || {
                        if GENERATION.load(Ordering::SeqCst) != generation {
                            return false;
                        }
                        let port = TAP_PORT.load(Ordering::SeqCst);
                        if port == 0 {
                            return !LISTEN_READY.load(Ordering::SeqCst);
                        }
                        if unsafe { CGEventTapIsEnabled(port as CFMachPortRef) } {
                            return false;
                        }
                        log::warn!("快捷键监听被系统停用，正在重新启用");
                        unsafe { CGEventTapEnable(port as CFMachPortRef, true) };
                        !unsafe { CGEventTapIsEnabled(port as CFMachPortRef) }
                    })
                    .unwrap_or(false);
                    if !needs_remount {
                        continue;
                    }
                    remount_if_needed(&app, generation, &detector, &trigger_tx);
                    note_listen_state(&app);
                }
            })
            .ok();
    }

    fn remount_if_needed(
        app: &AppHandle,
        generation: u64,
        detector: &Arc<Mutex<DoubleTapDetector>>,
        trigger_tx: &mpsc::SyncSender<()>,
    ) {
        let detector = detector.clone();
        let trigger_tx = trigger_tx.clone();
        if run_sync_on_main(app, move || {
            if GENERATION.load(Ordering::SeqCst) != generation || listen_healthy() {
                return;
            }
            log::info!("辅助功能已授权，正在挂载全局快捷键");
            install(generation, detector, trigger_tx);
        })
        .is_err()
        {
            log::warn!("重新挂载全局快捷键失败");
        }
    }

    fn note_listen_state(app: &AppHandle) {
        let Some(state) = app.try_state::<crate::shortcut::ShortcutState>() else {
            return;
        };
        if LISTEN_READY.load(Ordering::SeqCst) {
            state.clear_error();
        } else {
            state.set_error("全局快捷键未能挂到主线程，请重启 Quka");
        }
    }

    fn handle_ns_event(
        generation: u64,
        detector: &Mutex<DoubleTapDetector>,
        trigger_tx: &mpsc::SyncSender<()>,
        event: &NSEvent,
    ) {
        if GENERATION.load(Ordering::SeqCst) != generation {
            return;
        }
        let event_type = event.r#type();
        if matches!(event_type, NSEventType::KeyDown | NSEventType::KeyUp) && event.isARepeat() {
            return;
        }
        let keycode = event.keyCode();
        let class = classify(keycode);
        let down = match event_type {
            NSEventType::KeyDown => true,
            NSEventType::KeyUp => false,
            NSEventType::FlagsChanged => modifier_is_down(keycode, event.modifierFlags()),
            _ => return,
        };
        fire_if_triggered(detector, trigger_tx, class, down);
    }

    fn fire_if_triggered(
        detector: &Mutex<DoubleTapDetector>,
        trigger_tx: &mpsc::SyncSender<()>,
        class: KeyClass,
        down: bool,
    ) {
        let triggered = detector
            .lock()
            .map(|mut item| item.on_event(class, down, Instant::now()))
            .unwrap_or(false);
        if triggered {
            let _ = trigger_tx.try_send(());
        }
    }

    fn create_tap(
        location: CGEventTapLocation,
        generation: u64,
        detector: Arc<Mutex<DoubleTapDetector>>,
        trigger_tx: mpsc::SyncSender<()>,
        port_slot: Arc<AtomicUsize>,
    ) -> Result<CGEventTap<'static>, ()> {
        CGEventTap::new(
            location,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::ListenOnly,
            vec![
                CGEventType::KeyDown,
                CGEventType::KeyUp,
                CGEventType::FlagsChanged,
            ],
            move |_proxy, event_type, event| {
                if matches!(
                    event_type,
                    CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput
                ) {
                    log::warn!("快捷键监听被系统停用，正在重新启用");
                    let port = port_slot.load(Ordering::SeqCst);
                    if port != 0 {
                        unsafe { CGEventTapEnable(port as CFMachPortRef, true) }
                    }
                    return CallbackResult::Keep;
                }
                if GENERATION.load(Ordering::SeqCst) != generation {
                    return CallbackResult::Keep;
                }
                if let Some((class, down)) = classify_event(event_type, event) {
                    fire_if_triggered(&detector, &trigger_tx, class, down);
                }
                CallbackResult::Keep
            },
        )
    }

    fn classify_event(
        event_type: CGEventType,
        event: &core_graphics::event::CGEvent,
    ) -> Option<(KeyClass, bool)> {
        if event.get_integer_value_field(EventField::KEYBOARD_EVENT_AUTOREPEAT) != 0 {
            return None;
        }
        let keycode = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE) as u16;
        let class = classify(keycode);
        let down = match event_type {
            CGEventType::KeyDown => true,
            CGEventType::KeyUp => false,
            CGEventType::FlagsChanged => modifier_is_down_cg(keycode, event.get_flags()),
            _ => return None,
        };
        Some((class, down))
    }

    fn modifier_is_down(keycode: u16, flags: NSEventModifierFlags) -> bool {
        match classify(keycode) {
            KeyClass::Modifier(ModifierKind::Control) => {
                flags.contains(NSEventModifierFlags::Control)
            }
            KeyClass::Modifier(ModifierKind::Alt) => flags.contains(NSEventModifierFlags::Option),
            KeyClass::Modifier(ModifierKind::Super) => {
                flags.contains(NSEventModifierFlags::Command)
            }
            KeyClass::Other => false,
        }
    }

    fn modifier_is_down_cg(keycode: u16, flags: CGEventFlags) -> bool {
        match classify(keycode) {
            KeyClass::Modifier(ModifierKind::Control) => {
                flags.contains(CGEventFlags::CGEventFlagControl)
            }
            KeyClass::Modifier(ModifierKind::Alt) => {
                flags.contains(CGEventFlags::CGEventFlagAlternate)
            }
            KeyClass::Modifier(ModifierKind::Super) => {
                flags.contains(CGEventFlags::CGEventFlagCommand)
            }
            KeyClass::Other => false,
        }
    }

    fn classify(keycode: u16) -> KeyClass {
        match keycode {
            KeyCode::CONTROL | KeyCode::RIGHT_CONTROL => KeyClass::Modifier(ModifierKind::Control),
            KeyCode::OPTION | KeyCode::RIGHT_OPTION => KeyClass::Modifier(ModifierKind::Alt),
            KeyCode::COMMAND | KeyCode::RIGHT_COMMAND => KeyClass::Modifier(ModifierKind::Super),
            _ => KeyClass::Other,
        }
    }

    #[cfg(test)]
    mod tests {
        use super::{classify, KeyClass, ModifierKind};

        #[test]
        fn classifies_command_and_other_keys() {
            assert_eq!(classify(0x37), KeyClass::Modifier(ModifierKind::Super));
            assert_eq!(classify(0x36), KeyClass::Modifier(ModifierKind::Super));
            assert_eq!(classify(0x3B), KeyClass::Modifier(ModifierKind::Control));
            assert_eq!(classify(0x00), KeyClass::Other);
        }

        #[test]
        fn does_not_prompt_for_accessibility_during_startup() {
            assert!(!super::should_prompt_for_trust(false, false, 1));
            assert!(!super::should_prompt_for_trust(
                false,
                false,
                super::TRUST_RETRIES_BEFORE_PROMPT - 1
            ));
            assert!(!super::should_prompt_for_trust(true, false, 20));
            assert!(!super::should_prompt_for_trust(
                false,
                true,
                super::TRUST_RETRIES_BEFORE_PROMPT
            ));
            assert!(super::should_prompt_for_trust(
                false,
                false,
                super::TRUST_RETRIES_BEFORE_PROMPT
            ));
        }
    }
}
