use crate::download::{
    AudioFormat, DownloadControl, DownloadEvent, DownloadItem, ToolInstallResult, ToolKind,
    ToolStatus,
};
use crate::preview::{PreviewBundle, PreviewOutcome, TrackMeta};
use crate::scroll::ListScrollState;
use iced::Rectangle;

pub struct App {
    pub output_dir: String,
    pub audio_format: AudioFormat,
    pub download_workers: usize,
    pub enrich_preview_meta: bool,
    pub format_menu_open: bool,
    pub format_trigger_bounds: Rectangle,
    pub url: String,
    pub progress: Option<DownloadUi>,
    pub tools: Option<ToolStatus>,
    pub checking_tools: bool,
    pub tool_popup: Option<ToolKind>,
    pub tool_installing: Option<ToolKind>,
    pub tool_install_error: Option<String>,
    pub control: Option<DownloadControl>,
    pub generation: u64,
    pub preview_generation: u64,
    pub preview: PreviewState,
    pub track_scroll: ListScrollState,
    pub meta_enriching: bool,
    pub retry_items: Option<Vec<DownloadItem>>,
    pub retry_subfolder: Option<String>,
    pub last_subfolder: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DownloadUi {
    pub overall: f32,
    pub done: u32,
    pub total: u32,
    pub track_percent: f32,
    pub stage: String,
}

#[derive(Debug, Clone)]
pub enum PreviewState {
    Idle,
    Loading,
    Ready(PreviewBundle),
    Failed(String),
}

#[derive(Debug, Clone)]
pub enum Message {
    OutputDirChanged(String),
    Browse,
    UrlChanged(String),
    Download,
    RetryFailed,
    Cancel,
    ToggleFormatMenu,
    CloseFormatMenu,
    FormatTriggerBounds(Rectangle),
    SetAudioFormat(AudioFormat),
    DecreaseDownloadWorkers,
    IncreaseDownloadWorkers,
    ToggleEnrichPreviewMeta,
    RecheckTools,
    ToolsChecked(ToolStatus),
    OpenToolPopup(ToolKind),
    CloseToolPopup,
    InstallTool(ToolKind),
    ToolInstallFinished {
        kind: ToolKind,
        result: ToolInstallResult,
    },
    CycleLanguage,
    Noop,
    ToggleTrack(usize),
    SelectAll,
    SelectNone,
    TracksScrolled {
        offset: f32,
        viewport: f32,
        content: f32,
    },
    ScrollTracksTo(f32),
    PreviewLoaded {
        generation: u64,
        outcome: PreviewOutcome,
    },
    CoverLoaded {
        generation: u64,
        index: usize,
        handle: Option<iced::widget::image::Handle>,
    },
    MetaBatchLoaded {
        generation: u64,
        items: Vec<(usize, TrackMeta)>,
    },
    Event {
        generation: u64,
        event: DownloadEvent,
    },
}
