#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiLocale {
    ZhCn,
    En,
}

impl UiLocale {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ZhCn => "zh-CN",
            Self::En => "en",
        }
    }
}

pub fn resolve(mode: &str) -> UiLocale {
    let value = mode.trim().to_ascii_lowercase();
    if value == "zh-cn" || value == "zh" || value.starts_with("zh-") {
        return UiLocale::ZhCn;
    }
    if value == "en" || value.starts_with("en-") {
        return UiLocale::En;
    }
    system_locale()
}

fn system_locale() -> UiLocale {
    let detected = sys_locale::get_locale().unwrap_or_default();
    if detected.to_ascii_lowercase().starts_with("zh") {
        UiLocale::ZhCn
    } else {
        UiLocale::En
    }
}

pub fn settings_title(mode: &str) -> &'static str {
    match resolve(mode) {
        UiLocale::ZhCn => "Quka 设置",
        UiLocale::En => "Quka Settings",
    }
}

pub fn tray_open(mode: &str) -> &'static str {
    match resolve(mode) {
        UiLocale::ZhCn => {
            if cfg!(target_os = "macos") {
                "搜索"
            } else {
                "打开搜索"
            }
        }
        UiLocale::En => {
            if cfg!(target_os = "macos") {
                "Search"
            } else {
                "Open Search"
            }
        }
    }
}

pub fn tray_settings(mode: &str) -> &'static str {
    match resolve(mode) {
        UiLocale::ZhCn => "设置",
        UiLocale::En => "Settings",
    }
}

pub fn tray_rescan(mode: &str) -> &'static str {
    match resolve(mode) {
        UiLocale::ZhCn => {
            if cfg!(target_os = "macos") {
                "重新扫描"
            } else {
                "重新扫描应用"
            }
        }
        UiLocale::En => {
            if cfg!(target_os = "macos") {
                "Rescan"
            } else {
                "Rescan Apps"
            }
        }
    }
}

pub fn tray_quit(mode: &str) -> &'static str {
    match resolve(mode) {
        UiLocale::ZhCn => "退出",
        UiLocale::En => "Quit",
    }
}

#[cfg(test)]
mod tests {
    use super::{resolve, settings_title, UiLocale};

    #[test]
    fn resolves_explicit_and_system_aliases() {
        assert_eq!(resolve("zh-CN"), UiLocale::ZhCn);
        assert_eq!(resolve("en"), UiLocale::En);
        assert_eq!(resolve("en-US"), UiLocale::En);
        assert_eq!(settings_title("en"), "Quka Settings");
        assert_eq!(settings_title("zh-CN"), "Quka 设置");
    }
}
