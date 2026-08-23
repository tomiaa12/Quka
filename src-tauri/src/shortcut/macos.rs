use super::detect::ModifierKind;
use crate::state::AppError;

pub fn is_supported() -> bool {
    cfg!(target_os = "macos")
}

pub struct PlatformHandle {
    #[cfg(target_os = "macos")]
    generation: u64,
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
    use rdev::{listen, EventType, Key};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    static GENERATION: AtomicU64 = AtomicU64::new(0);

    pub fn start(
        kind: ModifierKind,
        on_trigger: impl Fn() + Send + Sync + 'static,
    ) -> Result<PlatformHandle, AppError> {
        let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
        let detector = Arc::new(Mutex::new(DoubleTapDetector::new(kind)));
        let (trigger_tx, trigger_rx) = std::sync::mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("quka-shortcut-fire".into())
            .spawn(move || {
                while trigger_rx.recv().is_ok() {
                    on_trigger();
                }
            })
            .map_err(|error| AppError::Shortcut(format!("快捷键注册失败：{error}")))?;
        std::thread::Builder::new()
            .name("quka-shortcut".into())
            .spawn(move || {
                let _ = listen(move |event| {
                    if GENERATION.load(Ordering::SeqCst) != generation {
                        return;
                    }
                    let (class, down) = match event.event_type {
                        EventType::KeyPress(key) => (classify(key), true),
                        EventType::KeyRelease(key) => (classify(key), false),
                        _ => return,
                    };
                    let triggered = detector
                        .lock()
                        .map(|mut item| item.on_event(class, down, Instant::now()))
                        .unwrap_or(false);
                    if triggered {
                        let _ = trigger_tx.try_send(());
                    }
                });
            })
            .map_err(|error| AppError::Shortcut(format!("快捷键注册失败：{error}")))?;
        Ok(PlatformHandle { generation })
    }

    pub fn stop(_handle: PlatformHandle) {
        GENERATION.fetch_add(1, Ordering::SeqCst);
    }

    fn classify(key: Key) -> KeyClass {
        match key {
            Key::ControlLeft | Key::ControlRight => KeyClass::Modifier(ModifierKind::Control),
            Key::Alt | Key::AltGr => KeyClass::Modifier(ModifierKind::Alt),
            Key::MetaLeft | Key::MetaRight => KeyClass::Modifier(ModifierKind::Super),
            _ => KeyClass::Other,
        }
    }
}
