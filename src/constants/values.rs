use iced::Font;

pub const APP_TITLE: &str = "YHits";
pub const APP_ICON: &[u8] = include_bytes!("../assets/icon.png");
pub const LOG_FONT: Font = Font::with_name("Consolas");
pub const COVER_SIZE: f32 = 48.0;
pub const COVER_RADIUS: f32 = 8.0;
pub const ROW_HEIGHT: f32 = 68.0;
pub const ROW_SPACING: f32 = 6.0;
pub const TOOL_CAPSULE_WIDTH: f32 = 78.0;
pub const TOOL_CAPSULE_HEIGHT: f32 = 26.0;
pub const LANG_CAPSULE_WIDTH: f32 = 44.0;
pub const TOOL_POPUP_WIDTH: f32 = 360.0;
pub const TOOL_POPUP_DETAIL_HEIGHT: f32 = 72.0;
pub const FORMAT_PICKER_WIDTH: f32 = 148.0;
pub const MIN_DOWNLOAD_WORKERS: usize = 1;
pub const MAX_DOWNLOAD_WORKERS: usize = 8;
pub const DEFAULT_DOWNLOAD_WORKERS: usize = 2;
