use super::Language;

pub fn detect_system_language() -> Language {
    #[cfg(windows)]
    if let Some(lang) = detect_windows() {
        return lang;
    }
    detect_from_env().unwrap_or(Language::English)
}

#[cfg(windows)]
fn detect_windows() -> Option<Language> {
    use windows_sys::Win32::Globalization::GetUserDefaultUILanguage;

    const LANG_RUSSIAN: u16 = 0x19;
    let lang_id = unsafe { GetUserDefaultUILanguage() };
    let primary = lang_id & 0x3FF;
    if primary == LANG_RUSSIAN {
        Some(Language::Russian)
    } else {
        Some(Language::English)
    }
}

fn detect_from_env() -> Option<Language> {
    for key in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(value) = std::env::var(key)
            && let Some(lang) = Language::from_code(&value)
        {
            return Some(lang);
        }
    }
    None
}
