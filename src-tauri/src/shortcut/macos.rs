use super::detect::ModifierKind;
use crate::state::AppError;

pub fn is_supported() -> bool {
    cfg!(target_os = "macos")
}

pub struct PlatformHandle {
    #[cfg(target_os = "macos")]
    run_loop: Option<core_foundation::runloop::CFRunLoop>,
}

pub fn start(kind: ModifierKind, on_trigger: impl Fn() + Send + Sync + 'static) -> Result<PlatformHandle, AppError> {
    #[cfg(target_os = "macos")]
    {
        native::start(kind, on_trigger)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (kind, on_trigger);
        Err(AppError::Shortcut("当前平台不是 macOS".into()))
    }
}

pub fn stop(handle: PlatformHandle) {
    #[cfg(target_os = "macos")]
    native::stop(handle);
    #[cfg(not(target_os = "macos"))]
    let _ = handle;
}

#[cfg(target_os = "macos")]
mod native {
    use super::{ModifierKind, PlatformHandle};
    use crate::shortcut::detect::{DoubleTapDetector, KeyClass};
    use crate::state::AppError;
    use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};
    use core_graphics::event::{
        CallbackResult, CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions,
        CGEventTapPlacement, CGEventType, EventField, KeyCode,
    };
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{mpsc, Arc, Mutex};
    use std::time::Instant;

    static GENERATION: AtomicU64 = AtomicU64::new(0);

    pub fn start(
        kind: ModifierKind,
        on_trigger: impl Fn() + Send + Sync + 'static,
    ) -> Result<PlatformHandle, AppError> {
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

        let (ready_tx, ready_rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("quka-shortcut".into())
            .spawn(move || {
                if let Err(error) = run_tap(generation, detector, trigger_tx, &ready_tx) {
                    log::error!("{error}");
                    let _ = ready_tx.send(Err(error));
                }
            })
            .map_err(|error| AppError::Shortcut(format!("快捷键注册失败：{error}")))?;

        match ready_rx.recv() {
            Ok(Ok(run_loop)) => Ok(PlatformHandle {
                run_loop: Some(run_loop),
            }),
            Ok(Err(error)) => Err(error),
            Err(error) => Err(AppError::Shortcut(format!("快捷键注册失败：{error}"))),
        }
    }

    pub fn stop(handle: PlatformHandle) {
        GENERATION.fetch_add(1, Ordering::SeqCst);
        if let Some(run_loop) = handle.run_loop {
            run_loop.stop();
        }
    }

    fn run_tap(
        generation: u64,
        detector: Arc<Mutex<DoubleTapDetector>>,
        trigger_tx: mpsc::SyncSender<()>,
        ready_tx: &mpsc::Sender<Result<CFRunLoop, AppError>>,
    ) -> Result<(), AppError> {
        let tap = create_tap(
            CGEventTapLocation::Session,
            generation,
            detector.clone(),
            trigger_tx.clone(),
        )
        .or_else(|_| create_tap(CGEventTapLocation::HID, generation, detector, trigger_tx))
        .map_err(|()| {
            AppError::Shortcut(
                "快捷键注册失败：请在系统设置 › 隐私与安全性 › 辅助功能 中允许 Quka".into(),
            )
        })?;

        let loop_source = tap
            .mach_port()
            .create_runloop_source(0)
            .map_err(|()| AppError::Shortcut("快捷键注册失败：无法接入系统事件循环".into()))?;
        let run_loop = CFRunLoop::get_current();
        run_loop.add_source(&loop_source, unsafe { kCFRunLoopCommonModes });
        tap.enable();
        ready_tx
            .send(Ok(run_loop.clone()))
            .map_err(|error| AppError::Shortcut(error.to_string()))?;
        CFRunLoop::run_current();
        Ok(())
    }

    fn create_tap(
        location: CGEventTapLocation,
        generation: u64,
        detector: Arc<Mutex<DoubleTapDetector>>,
        trigger_tx: mpsc::SyncSender<()>,
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
                if GENERATION.load(Ordering::SeqCst) != generation {
                    return CallbackResult::Keep;
                }
                if let Some((class, down)) = classify_event(event_type, event) {
                    let triggered = detector
                        .lock()
                        .map(|mut item| item.on_event(class, down, Instant::now()))
                        .unwrap_or(false);
                    if triggered {
                        let _ = trigger_tx.try_send(());
                    }
                }
                CallbackResult::Keep
            },
        )
    }

    fn classify_event(
        event_type: CGEventType,
        event: &core_graphics::event::CGEvent,
    ) -> Option<(KeyClass, bool)> {
        if event
            .get_integer_value_field(EventField::KEYBOARD_EVENT_AUTOREPEAT)
            != 0
        {
            return None;
        }
        let keycode = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE) as u16;
        let class = classify(keycode);
        let down = match event_type {
            CGEventType::KeyDown => true,
            CGEventType::KeyUp => false,
            CGEventType::FlagsChanged => modifier_is_down(keycode, event.get_flags()),
            _ => return None,
        };
        Some((class, down))
    }

    fn modifier_is_down(keycode: u16, flags: CGEventFlags) -> bool {
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
