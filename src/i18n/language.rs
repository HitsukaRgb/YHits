use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Language {
    Russian = 0,
    English = 1,
}

impl Language {
    pub fn code(self) -> &'static str {
        match self {
            Self::Russian => "RU",
            Self::English => "EN",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Russian => Self::English,
            Self::English => Self::Russian,
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        let normalized = code.trim().to_ascii_lowercase();
        let primary = normalized.split(['-', '_']).next().unwrap_or(&normalized);
        match primary {
            "ru" => Some(Self::Russian),
            "en" => Some(Self::English),
            _ => None,
        }
    }

    fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::Russian,
            _ => Self::English,
        }
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(Language::English as u8);

pub fn current_language() -> Language {
    Language::from_u8(CURRENT.load(Ordering::Relaxed))
}

pub fn set_language(lang: Language) {
    CURRENT.store(lang as u8, Ordering::Relaxed);
}
