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

        request_accessibility_access();
        let trusted = is_trusted();
        let installed = run_sync_on_main(app, {
            let detector = detector.clone();
            let trigger_tx = trigger_tx.clone();
            move || install(generation, detector, trigger_tx)
        })?;

        if !installed.global {
            spawn_trust_waiter(app.clone(), generation, detector, trigger_tx.clone());
        }

        let warning = if installed.global {
            None
        } else if trusted {
            Some("全局快捷键未能挂到主线程，请重启 Quka".into())
        } else {
            Some(ACCESSIBILITY_HINT.into())
        };
        if let Some(warning) = &warning {
            log::warn!("{warning}");
        } else {
            log::info!("快捷键监听已挂到主线程（其它应用前台也可触发）");
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

        InstallResult {
            global: global || has_tap,
        }
    }

    fn uninstall() {
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
        port_slot.store(
            tap.mach_port().as_concrete_TypeRef() as usize,
            Ordering::SeqCst,
        );
        let loop_source = tap.mach_port().create_runloop_source(0).ok()?;
        CFRunLoop::get_main().add_source(&loop_source, unsafe { kCFRunLoopCommonModes });
        tap.enable();
        Some(tap)
    }

    fn spawn_trust_waiter(
        app: AppHandle,
        generation: u64,
        detector: Arc<Mutex<DoubleTapDetector>>,
        trigger_tx: mpsc::SyncSender<()>,
    ) {
        std::thread::Builder::new()
            .name("quka-shortcut-trust".into())
            .spawn(move || {
                for _ in 0..180 {
                    std::thread::sleep(Duration::from_secs(1));
                    if GENERATION.load(Ordering::SeqCst) != generation {
                        return;
                    }
                    if !is_trusted() {
                        continue;
                    }
                    log::info!("已获得辅助功能权限，正在重新挂载全局快捷键");
                    let detector = detector.clone();
                    let trigger_tx = trigger_tx.clone();
                    if run_sync_on_main(&app, move || {
                        if GENERATION.load(Ordering::SeqCst) != generation {
                            return;
                        }
                        install(generation, detector, trigger_tx);
                    })
                    .is_ok()
                    {
                        if let Some(state) = app.try_state::<crate::shortcut::ShortcutState>() {
                            state.clear_error();
                        }
                    }
                    return;
                }
            })
            .ok();
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
    }
}
