use super::detect::{DoubleTapDetector, KeyClass, ModifierKind};
use crate::state::AppError;

pub fn is_supported() -> bool {
    cfg!(target_os = "windows")
}

pub struct PlatformHandle {
    #[cfg(windows)]
    thread_id: u32,
    #[cfg(windows)]
    join: Option<std::thread::JoinHandle<()>>,
}

pub fn start(kind: ModifierKind, on_trigger: impl Fn() + Send + Sync + 'static) -> Result<PlatformHandle, AppError> {
    #[cfg(windows)]
    {
        native::start(kind, on_trigger)
    }
    #[cfg(not(windows))]
    {
        let _ = (kind, on_trigger);
        Err(AppError::Shortcut("当前平台不是 Windows".into()))
    }
}

pub fn stop(handle: PlatformHandle) {
    #[cfg(windows)]
    native::stop(handle);
    #[cfg(not(windows))]
    let _ = handle;
}

#[cfg(windows)]
mod native {
    use super::{DoubleTapDetector, KeyClass, ModifierKind, PlatformHandle};
    use crate::state::AppError;
    use std::cell::RefCell;
    use std::sync::mpsc;
    use std::time::Instant;
    use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        VIRTUAL_KEY, VK_CONTROL, VK_LCONTROL, VK_LMENU, VK_LWIN, VK_MENU, VK_RCONTROL, VK_RMENU,
        VK_RWIN,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, GetMessageW, PostThreadMessageW, SetWindowsHookExW,
        TranslateMessage, UnhookWindowsHookEx, HC_ACTION, HHOOK, KBDLLHOOKSTRUCT, MSG,
        WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN, WM_SYSKEYUP,
    };

    struct TlsHook {
        detector: DoubleTapDetector,
        trigger: mpsc::SyncSender<()>,
        hook: isize,
    }

    thread_local! {
        static TLS: RefCell<Option<TlsHook>> = RefCell::new(None);
    }

    pub fn start(
        kind: ModifierKind,
        on_trigger: impl Fn() + Send + Sync + 'static,
    ) -> Result<PlatformHandle, AppError> {
        let (ready_tx, ready_rx) = mpsc::channel();
        let join = std::thread::Builder::new()
            .name("quka-shortcut".into())
            .spawn(move || {
                if let Err(error) = run_hook_thread(kind, on_trigger, &ready_tx) {
                    log::error!("快捷键注册失败：{error}");
                    let _ = ready_tx.send(Err(error));
                }
            })
            .map_err(|error| AppError::Shortcut(error.to_string()))?;

        match ready_rx.recv() {
            Ok(Ok(thread_id)) => Ok(PlatformHandle {
                thread_id,
                join: Some(join),
            }),
            Ok(Err(error)) => {
                let _ = join.join();
                Err(error)
            }
            Err(error) => {
                let _ = join.join();
                Err(AppError::Shortcut(error.to_string()))
            }
        }
    }

    pub fn stop(mut handle: PlatformHandle) {
        unsafe {
            let _ = PostThreadMessageW(handle.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(join) = handle.join.take() {
            let _ = join.join();
        }
    }

    fn run_hook_thread(
        kind: ModifierKind,
        on_trigger: impl Fn() + Send + Sync + 'static,
        ready_tx: &mpsc::Sender<Result<u32, AppError>>,
    ) -> Result<(), AppError> {
        let (trigger_tx, trigger_rx) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("quka-shortcut-fire".into())
            .spawn(move || {
                while trigger_rx.recv().is_ok() {
                    on_trigger();
                }
            })
            .map_err(|error| AppError::Shortcut(error.to_string()))?;

        let hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), None, 0) }
            .map_err(|error| AppError::Shortcut(format!("快捷键注册失败：{error}")))?;
        TLS.with(|slot| {
            *slot.borrow_mut() = Some(TlsHook {
                detector: DoubleTapDetector::new(kind),
                trigger: trigger_tx,
                hook: hook.0 as isize,
            });
        });
        let _ = ready_tx.send(Ok(unsafe { GetCurrentThreadId() }));

        unsafe {
            let mut message = MSG::default();
            while GetMessageW(&mut message, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }

        TLS.with(|slot| {
            if let Some(context) = slot.borrow_mut().take() {
                unsafe {
                    let _ = UnhookWindowsHookEx(HHOOK(context.hook as *mut core::ffi::c_void));
                }
            }
        });
        Ok(())
    }

    unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code == HC_ACTION as i32 && lparam.0 != 0 {
            let info = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
            let down = matches!(wparam.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
            let up = matches!(wparam.0 as u32, WM_KEYUP | WM_SYSKEYUP);
            if down || up {
                TLS.with(|slot| {
                    if let Some(tls) = slot.borrow_mut().as_mut() {
                        if tls.detector.on_event(
                            classify(VIRTUAL_KEY(info.vkCode as u16)),
                            down,
                            Instant::now(),
                        ) {
                            let _ = tls.trigger.try_send(());
                        }
                    }
                });
            }
        }
        CallNextHookEx(None, code, wparam, lparam)
    }

    fn classify(key: VIRTUAL_KEY) -> KeyClass {
        if key == VK_CONTROL || key == VK_LCONTROL || key == VK_RCONTROL {
            KeyClass::Modifier(ModifierKind::Control)
        } else if key == VK_MENU || key == VK_LMENU || key == VK_RMENU {
            KeyClass::Modifier(ModifierKind::Alt)
        } else if key == VK_LWIN || key == VK_RWIN {
            KeyClass::Modifier(ModifierKind::Super)
        } else {
            KeyClass::Other
        }
    }
}
