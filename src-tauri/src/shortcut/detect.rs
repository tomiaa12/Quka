use std::time::{Duration, Instant};

use crate::state::AppError;

pub const DOUBLE_TAP_MS: u64 = 400;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModifierKind {
    Control,
    Alt,
    Super,
}

impl ModifierKind {
    pub fn storage_id(self) -> &'static str {
        match self {
            Self::Control => "DoubleCtrl",
            Self::Alt => "DoubleAlt",
            Self::Super => "DoubleCommand",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Control => "双击 Ctrl",
            Self::Alt => "双击 Alt",
            Self::Super => {
                if cfg!(target_os = "macos") {
                    "双击 Command"
                } else {
                    "双击 Win"
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyClass {
    Modifier(ModifierKind),
    Other,
}

#[derive(Debug, Clone)]
pub struct DoubleTapDetector {
    target: ModifierKind,
    timeout: Duration,
    last_up: Option<Instant>,
    down: bool,
    polluted: bool,
}

impl DoubleTapDetector {
    pub fn new(target: ModifierKind) -> Self {
        Self {
            target,
            timeout: Duration::from_millis(DOUBLE_TAP_MS),
            last_up: None,
            down: false,
            polluted: false,
        }
    }

    pub fn on_event(&mut self, class: KeyClass, down: bool, now: Instant) -> bool {
        match class {
            KeyClass::Other => {
                if down {
                    self.polluted = true;
                    self.last_up = None;
                }
                false
            }
            KeyClass::Modifier(kind) if kind != self.target => {
                if down {
                    self.polluted = true;
                    self.last_up = None;
                }
                false
            }
            KeyClass::Modifier(_) => {
                if down {
                    self.down = true;
                    self.polluted = false;
                    return false;
                }
                if !self.down {
                    return false;
                }
                self.down = false;
                if self.polluted {
                    self.polluted = false;
                    self.last_up = None;
                    return false;
                }
                if let Some(previous) = self.last_up {
                    if now.duration_since(previous) <= self.timeout {
                        self.last_up = None;
                        return true;
                    }
                }
                self.last_up = Some(now);
                false
            }
        }
    }
}

pub fn default_shortcut() -> String {
    if cfg!(target_os = "macos") {
        ModifierKind::Super.storage_id().into()
    } else {
        ModifierKind::Control.storage_id().into()
    }
}

pub fn parse_shortcut(value: &str) -> Result<ModifierKind, AppError> {
    let normalized = value
        .trim()
        .to_ascii_lowercase()
        .replace(' ', "")
        .replace('＋', "+");
    match normalized.as_str() {
        "doublectrl" | "doublecontrol" | "双击ctrl" | "ctrl" => Ok(ModifierKind::Control),
        "doublealt" | "双击alt" | "alt" => Ok(ModifierKind::Alt),
        "doublewin" | "doublecommand" | "doublesuper" | "双击command" | "双击win" | "command"
        | "win" | "super" => Ok(ModifierKind::Super),
        "ctrl+space" | "control+space" | "command+space" | "cmd+space" => Ok(if cfg!(target_os = "macos") {
            ModifierKind::Super
        } else {
            ModifierKind::Control
        }),
        _ => Err(AppError::Shortcut(format!(
            "快捷键注册失败：不支持 {value}"
        ))),
    }
}

pub fn normalize_shortcut(value: &str) -> String {
    parse_shortcut(value)
        .map(|kind| kind.storage_id().to_string())
        .unwrap_or_else(|_| default_shortcut())
}

pub fn shortcut_label(value: &str) -> String {
    parse_shortcut(value)
        .map(ModifierKind::label)
        .unwrap_or("双击 Ctrl")
        .into()
}

pub fn next_shortcut(value: &str) -> String {
    match parse_shortcut(value).unwrap_or(ModifierKind::Control) {
        ModifierKind::Control => ModifierKind::Alt.storage_id().into(),
        ModifierKind::Alt => ModifierKind::Super.storage_id().into(),
        ModifierKind::Super => ModifierKind::Control.storage_id().into(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        default_shortcut, next_shortcut, normalize_shortcut, parse_shortcut, DoubleTapDetector,
        KeyClass, ModifierKind,
    };
    use std::time::{Duration, Instant};

    fn tap(detector: &mut DoubleTapDetector, now: Instant) -> bool {
        detector.on_event(KeyClass::Modifier(ModifierKind::Control), true, now);
        detector.on_event(
            KeyClass::Modifier(ModifierKind::Control),
            false,
            now + Duration::from_millis(20),
        )
    }

    #[test]
    fn parses_double_ctrl() {
        assert_eq!(parse_shortcut("双击 Ctrl").unwrap(), ModifierKind::Control);
        assert_eq!(parse_shortcut("DoubleCtrl").unwrap(), ModifierKind::Control);
        assert_eq!(normalize_shortcut("Ctrl+Space"), default_shortcut());
    }

    #[test]
    fn cycles_shortcuts() {
        assert_eq!(next_shortcut("DoubleCtrl"), "DoubleAlt");
        assert_eq!(next_shortcut("DoubleAlt"), "DoubleCommand");
        assert_eq!(next_shortcut("DoubleCommand"), "DoubleCtrl");
    }

    #[test]
    fn detects_double_tap() {
        let mut detector = DoubleTapDetector::new(ModifierKind::Control);
        let start = Instant::now();
        assert!(!tap(&mut detector, start));
        assert!(tap(&mut detector, start + Duration::from_millis(180)));
    }

    #[test]
    fn ignores_slow_second_tap() {
        let mut detector = DoubleTapDetector::new(ModifierKind::Control);
        let start = Instant::now();
        assert!(!tap(&mut detector, start));
        assert!(!tap(&mut detector, start + Duration::from_millis(800)));
    }

    #[test]
    fn ignores_ctrl_plus_other_key() {
        let mut detector = DoubleTapDetector::new(ModifierKind::Control);
        let start = Instant::now();
        detector.on_event(KeyClass::Modifier(ModifierKind::Control), true, start);
        detector.on_event(KeyClass::Other, true, start + Duration::from_millis(10));
        detector.on_event(KeyClass::Other, false, start + Duration::from_millis(20));
        assert!(!detector.on_event(
            KeyClass::Modifier(ModifierKind::Control),
            false,
            start + Duration::from_millis(30)
        ));
        assert!(!tap(&mut detector, start + Duration::from_millis(80)));
    }
}
