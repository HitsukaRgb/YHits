use super::state::{App, DownloadUi, Message, PreviewState};
use crate::config;
use crate::constants::{
    APP_TITLE, MAX_DOWNLOAD_WORKERS, MIN_DOWNLOAD_WORKERS, ROW_HEIGHT, ROW_SPACING,
};
use crate::download::{
    self, DownloadEvent, DownloadItem, Finish, ToolInstallResult, ToolKind, ToolStatus,
    detect_tools, install_tool, stage,
};
use crate::i18n::{Tr, current_language, detect_system_language, set_language, t, tf};
use crate::preview::{self, PreviewOutcome};
use crate::scroll::{self, ListScrollState};
use iced::{Color, Rectangle, Task, Theme};

pub fn theme(_app: &App) -> Theme {
    Theme::custom(
        APP_TITLE,
        iced::theme::Palette {
            background: Color::from_rgb8(18, 16, 28),
            text: Color::from_rgb8(238, 234, 248),
            primary: Color::from_rgb8(185, 163, 255),
            success: Color::from_rgb8(64, 186, 188),
            warning: Color::from_rgb8(220, 168, 96),
            danger: Color::from_rgb8(232, 93, 106),
        },
    )
}

impl App {
    pub fn boot() -> (Self, Task<Message>) {
        set_language(detect_system_language());
        (
            Self {
                output_dir: config::load_output_dir(),
                audio_format: config::load_audio_format(),
                download_workers: config::load_download_workers(),
                enrich_preview_meta: config::load_enrich_preview_meta(),
                format_menu_open: false,
                format_trigger_bounds: Rectangle::default(),
                url: String::new(),
                progress: None,
                tools: None,
                checking_tools: true,
                tool_popup: None,
                tool_installing: None,
                tool_install_error: None,
                control: None,
                generation: 0,
                preview_generation: 0,
                preview: PreviewState::Idle,
                track_scroll: ListScrollState::default(),
                meta_enriching: false,
                retry_items: None,
                retry_subfolder: None,
                last_subfolder: None,
            },
            Task::perform(detect_tools(), Message::ToolsChecked),
        )
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        if self.format_menu_open
            && !matches!(
                message,
                Message::ToggleFormatMenu
                    | Message::CloseFormatMenu
                    | Message::FormatTriggerBounds(_)
                    | Message::SetAudioFormat(_)
                    | Message::Noop
            )
        {
            self.format_menu_open = false;
        }

        match message {
            Message::OutputDirChanged(path) => {
                self.output_dir = path;
                self.remember_directory();
                Task::none()
            }
            Message::Browse => {
                let mut dialog =
                    rfd::FileDialog::new().set_title(t(Tr::BrowseDialogTitle));
                if let Some(directory) = config::existing_ancestor(&self.output_dir) {
                    dialog = dialog.set_directory(directory);
                }
                if let Some(path) = dialog.pick_folder() {
                    self.output_dir = path.display().to_string();
                    self.remember_directory();
                }
                Task::none()
            }
            Message::UrlChanged(url) => self.on_url_changed(url),
            Message::Download => self.start_download(),
            Message::RetryFailed => self.start_retry(),
            Message::Cancel => {
                if let Some(control) = &self.control {
                    control.cancel();
                }
                Task::none()
            }
            Message::ToggleFormatMenu => {
                if self.control.is_some() {
                    return Task::none();
                }
                self.format_menu_open = !self.format_menu_open;
                Task::none()
            }
            Message::CloseFormatMenu => {
                self.format_menu_open = false;
                Task::none()
            }
            Message::FormatTriggerBounds(bounds) => {
                self.format_trigger_bounds = bounds;
                Task::none()
            }
            Message::SetAudioFormat(format) => {
                self.audio_format = format;
                self.format_menu_open = false;
                self.remember_audio_format();
                Task::none()
            }
            Message::DecreaseDownloadWorkers => {
                if self.control.is_some() {
                    return Task::none();
                }
                let next = self.download_workers.saturating_sub(1).max(MIN_DOWNLOAD_WORKERS);
                if next != self.download_workers {
                    self.download_workers = next;
                    self.remember_download_workers();
                }
                Task::none()
            }
            Message::IncreaseDownloadWorkers => {
                if self.control.is_some() {
                    return Task::none();
                }
                let next = (self.download_workers + 1).min(MAX_DOWNLOAD_WORKERS);
                if next != self.download_workers {
                    self.download_workers = next;
                    self.remember_download_workers();
                }
                Task::none()
            }
            Message::ToggleEnrichPreviewMeta => {
                self.enrich_preview_meta = !self.enrich_preview_meta;
                self.remember_enrich_preview_meta();
                if self.enrich_preview_meta {
                    if let PreviewState::Ready(bundle) = &mut self.preview {
                        for track in &mut bundle.tracks {
                            if track.needs_meta_enrichment() {
                                track.meta_requested = false;
                            }
                        }
                    }
                    self.meta_enriching = false;
                    self.ensure_visible_assets()
                } else {
                    Task::none()
                }
            }
            Message::RecheckTools => {
                self.checking_tools = true;
                self.tool_install_error = None;
                Task::perform(detect_tools(), Message::ToolsChecked)
            }
            Message::ToolsChecked(status) => {
                self.checking_tools = false;
                self.tools = Some(status);
                let waiting_preview = matches!(
                    self.preview,
                    PreviewState::Loading | PreviewState::Failed(_)
                );
                if waiting_preview && download::is_youtube_url(self.url.trim()) {
                    return self.on_url_changed(self.url.clone());
                }
                Task::none()
            }
            Message::OpenToolPopup(kind) => {
                self.tool_popup = Some(kind);
                self.tool_install_error = None;
                Task::none()
            }
            Message::CloseToolPopup => {
                if self.tool_installing.is_some() || self.checking_tools {
                    return Task::none();
                }
                self.tool_popup = None;
                self.tool_install_error = None;
                Task::none()
            }
            Message::InstallTool(kind) => {
                if self.tool_installing.is_some() || self.checking_tools {
                    return Task::none();
                }
                self.tool_popup = Some(kind);
                self.tool_installing = Some(kind);
                self.tool_install_error = None;
                Task::perform(install_tool(kind), move |result| {
                    Message::ToolInstallFinished { kind, result }
                })
            }
            Message::ToolInstallFinished { kind, result } => {
                self.tool_installing = None;
                match result {
                    ToolInstallResult::Ok => {
                        self.tool_install_error = None;
                        self.checking_tools = true;
                        Task::perform(detect_tools(), Message::ToolsChecked)
                    }
                    ToolInstallResult::Failed(message) => {
                        self.tool_popup = Some(kind);
                        self.tool_install_error = Some(message);
                        Task::none()
                    }
                }
            }
            Message::CycleLanguage => {
                set_language(current_language().next());
                Task::none()
            }
            Message::Noop => Task::none(),
            Message::ToggleTrack(index) => {
                if self.control.is_some() {
                    return Task::none();
                }
                if let PreviewState::Ready(bundle) = &mut self.preview
                    && let Some(track) = bundle.tracks.get_mut(index)
                {
                    track.selected = !track.selected;
                }
                Task::none()
            }
            Message::SelectAll => {
                if let PreviewState::Ready(bundle) = &mut self.preview {
                    for track in &mut bundle.tracks {
                        track.selected = true;
                    }
                }
                Task::none()
            }
            Message::SelectNone => {
                if let PreviewState::Ready(bundle) = &mut self.preview {
                    for track in &mut bundle.tracks {
                        track.selected = false;
                    }
                }
                Task::none()
            }
            Message::TracksScrolled {
                offset,
                viewport,
                content,
            } => {
                self.track_scroll
                    .apply_scroll_report(offset, viewport, content);
                self.ensure_visible_assets()
            }
            Message::ScrollTracksTo(offset) => {
                self.track_scroll.offset = offset.max(0.0);
                scroll::scroll_to_y(offset)
            }
            Message::PreviewLoaded { generation, outcome } => {
                if generation != self.preview_generation {
                    return Task::none();
                }
                match outcome {
                    PreviewOutcome::Ready(bundle) => {
                        let track_count = bundle.tracks.len();
                        self.preview = PreviewState::Ready(bundle);
                        self.track_scroll = ListScrollState::default()
                            .with_content_extent(track_count, ROW_HEIGHT, ROW_SPACING);
                        self.ensure_visible_assets()
                    }
                    PreviewOutcome::Failed(message) => {
                        self.preview = PreviewState::Failed(message);
                        Task::none()
                    }
                    PreviewOutcome::Cancelled => {
                        self.preview = PreviewState::Idle;
                        Task::none()
                    }
                }
            }
            Message::CoverLoaded {
                generation,
                index,
                handle,
            } => {
                if generation != self.preview_generation {
                    return Task::none();
                }
                if let PreviewState::Ready(bundle) = &mut self.preview
                    && let Some(track) = bundle.tracks.get_mut(index)
                {
                    track.cover = handle;
                }
                Task::none()
            }
            Message::MetaBatchLoaded { generation, items } => {
                self.meta_enriching = false;
                if generation != self.preview_generation {
                    return Task::none();
                }
                if let PreviewState::Ready(bundle) = &mut self.preview {
                    for (index, meta) in items {
                        let Some(track) = bundle.tracks.get_mut(index) else {
                            continue;
                        };
                        if !meta.artists.is_empty() {
                            track.artists = meta.artists;
                        }
                        if !meta.album.trim().is_empty() {
                            track.album = meta.album;
                        }
                    }
                }
                self.ensure_visible_assets()
            }
            Message::Event { generation, event } => {
                if generation != self.generation {
                    return Task::none();
                }
                self.on_download_event(event);
                Task::none()
            }
        }
    }

    fn on_url_changed(&mut self, url: String) -> Task<Message> {
        self.url = url;
        self.preview_generation = self.preview_generation.wrapping_add(1);
        let generation = self.preview_generation;

        let trimmed = self.url.trim().to_string();
        if trimmed.is_empty() {
            self.preview = PreviewState::Idle;
            self.track_scroll = ListScrollState::default();
            self.clear_retry();
            return Task::none();
        }

        if !download::is_youtube_url(&trimmed) {
            self.preview = PreviewState::Idle;
            self.track_scroll = ListScrollState::default();
            self.clear_retry();
            return Task::none();
        }

        self.preview = PreviewState::Loading;
        self.track_scroll = ListScrollState::default();
        self.meta_enriching = false;
        self.clear_retry();

        if self.checking_tools || self.tools.is_none() {
            return Task::none();
        }

        match self.tools.as_ref() {
            None => Task::none(),
            Some(status) if !status.has(ToolKind::YtDlp) => {
                self.preview = PreviewState::Failed(t(Tr::PreviewNeedsYtdlp).into());
                Task::none()
            }
            Some(_) => Task::perform(
                preview::fetch_debounced(trimmed, generation),
                |(generation, outcome)| Message::PreviewLoaded { generation, outcome },
            ),
        }
    }

    pub fn download_label(&self) -> String {
        match &self.preview {
            PreviewState::Ready(bundle) => {
                let selected = bundle.tracks.iter().filter(|track| track.selected).count();
                if selected == 0 {
                    t(Tr::Download).into()
                } else if selected == bundle.tracks.len() {
                    tf(Tr::DownloadAll, &[("count", &selected.to_string())])
                } else {
                    tf(Tr::DownloadSelected, &[("count", &selected.to_string())])
                }
            }
            _ => t(Tr::Download).into(),
        }
    }

    pub fn download_message(&self) -> Option<Message> {
        self.can_download().then_some(Message::Download)
    }

    fn can_download(&self) -> bool {
        if self.control.is_some()
            || !self.tools.as_ref().is_some_and(ToolStatus::ready)
            || self.output_dir.trim().is_empty()
        {
            return false;
        }

        match &self.preview {
            PreviewState::Ready(bundle) => bundle.tracks.iter().any(|track| track.selected),
            PreviewState::Loading => false,
            _ => download::is_youtube_url(&self.url),
        }
    }

    fn selected_items(&self) -> Vec<DownloadItem> {
        match &self.preview {
            PreviewState::Ready(bundle) => bundle
                .tracks
                .iter()
                .filter(|track| track.selected)
                .map(|track| DownloadItem {
                    url: track.url.clone(),
                })
                .collect(),
            _ => {
                if download::is_youtube_url(&self.url) {
                    vec![DownloadItem {
                        url: download::normalize_url(&self.url),
                    }]
                } else {
                    Vec::new()
                }
            }
        }
    }

    fn download_subfolder(&self) -> Option<String> {
        match &self.preview {
            PreviewState::Ready(bundle) => {
                let selected = bundle.tracks.iter().filter(|track| track.selected).count();
                if selected > 1 {
                    bundle
                        .source_title
                        .as_ref()
                        .map(|title| download::sanitize_folder_name(title))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn clear_retry(&mut self) {
        self.retry_items = None;
        self.retry_subfolder = None;
    }

    fn start_download(&mut self) -> Task<Message> {
        if !self.can_download() {
            return Task::none();
        }
        let items = self.selected_items();
        if items.is_empty() {
            return Task::none();
        }
        let subfolder = self.download_subfolder();
        self.clear_retry();
        self.begin_download(items, subfolder)
    }

    fn start_retry(&mut self) -> Task<Message> {
        if self.control.is_some() || !self.tools.as_ref().is_some_and(ToolStatus::ready) {
            return Task::none();
        }
        let Some(items) = self.retry_items.take() else {
            return Task::none();
        };
        let subfolder = self.retry_subfolder.take();
        if items.is_empty() {
            return Task::none();
        }
        self.begin_download(items, subfolder)
    }

    fn begin_download(
        &mut self,
        items: Vec<DownloadItem>,
        subfolder: Option<String>,
    ) -> Task<Message> {
        let directory = std::path::PathBuf::from(self.output_dir.trim());
        if std::fs::create_dir_all(&directory).is_err() {
            return Task::none();
        }

        self.remember_directory();
        self.last_subfolder = subfolder.clone();
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        let count = items.len() as u32;
        self.progress = Some(DownloadUi {
            overall: 0.0,
            done: 0,
            total: count,
            track_percent: 0.0,
            stage: stage::PREPARING.into(),
        });

        let (control, stream) = download::start(
            items,
            directory,
            subfolder,
            self.audio_format,
            self.download_workers,
        );
        self.control = Some(control);
        Task::run(stream, move |event| Message::Event { generation, event })
    }

    fn on_download_event(&mut self, event: DownloadEvent) {
        match event {
            DownloadEvent::Progress {
                track_percent,
                stage: next_stage,
                item_total,
                tracks_done,
            } => {
                let total = item_total
                    .or_else(|| self.progress.as_ref().map(|p| p.total))
                    .unwrap_or(1)
                    .max(1);
                let mut ui = self.progress.take().unwrap_or(DownloadUi {
                    overall: 0.0,
                    done: 0,
                    total,
                    track_percent: 0.0,
                    stage: stage::DOWNLOADING.into(),
                });
                ui.total = total;
                if let Some(done) = tracks_done {
                    ui.done = ui.done.max(done.min(total));
                }
                if let Some(percent) = track_percent {
                    ui.track_percent = percent;
                }
                if let Some(next_stage) = next_stage {
                    ui.stage = next_stage;
                }

                let frac = if ui.done >= ui.total || ui.stage == stage::DONE {
                    0.0
                } else if ui.stage == stage::DOWNLOADING {
                    (ui.track_percent / 100.0).clamp(0.0, 0.999)
                } else {
                    0.999
                };
                ui.overall = ((ui.done as f32) + frac) / (ui.total as f32) * 100.0;
                if ui.done >= ui.total {
                    ui.overall = 100.0;
                    ui.track_percent = 100.0;
                }
                self.progress = Some(ui);
            }
            DownloadEvent::Finished(finish) => {
                self.control = None;
                match finish {
                    Finish::Success => {
                        self.clear_retry();
                        if let Some(progress) = &mut self.progress {
                            progress.done = progress.total;
                            progress.overall = 100.0;
                            progress.stage = stage::DONE.into();
                        }
                    }
                    Finish::Partial { failed } => {
                        self.retry_subfolder = self.last_subfolder.clone();
                        self.retry_items = (!failed.is_empty()).then_some(failed);
                        self.progress = None;
                    }
                    Finish::Failed | Finish::Cancelled => {
                        self.clear_retry();
                        self.progress = None;
                    }
                }
            }
        }
    }

    fn ensure_visible_assets(&mut self) -> Task<Message> {
        let PreviewState::Ready(bundle) = &mut self.preview else {
            return Task::none();
        };
        let generation = self.preview_generation;
        let (start, end) = self.track_scroll.visible_range(
            bundle.tracks.len(),
            ROW_HEIGHT,
            ROW_SPACING,
            scroll::LIST_OVERSCAN_BACK,
            scroll::LIST_OVERSCAN_AHEAD,
        );

        let mut tasks = Vec::new();
        for index in start..end {
            let Some(track) = bundle.tracks.get_mut(index) else {
                continue;
            };
            if !track.cover_requested {
                if let Some(url) = track.thumbnail_url.clone() {
                    track.cover_requested = true;
                    tasks.push(Task::perform(
                        preview::fetch_cover(generation, index, url),
                        |(generation, index, handle)| Message::CoverLoaded {
                            generation,
                            index,
                            handle,
                        },
                    ));
                } else {
                    track.cover_requested = true;
                }
            }
        }

        if self.enrich_preview_meta && !self.meta_enriching {
            if bundle.shared_album {
                let mut jobs = Vec::new();
                for index in 0..bundle.tracks.len() {
                    let Some(track) = bundle.tracks.get_mut(index) else {
                        continue;
                    };
                    if !track.meta_requested && track.needs_meta_enrichment() {
                        track.meta_requested = true;
                        jobs.push((index, track.url.clone()));
                    }
                }
                if !jobs.is_empty() {
                    self.meta_enriching = true;
                    tasks.push(Task::perform(
                        preview::fetch_shared_album_meta(generation, jobs),
                        |(generation, items)| Message::MetaBatchLoaded { generation, items },
                    ));
                }
            } else {
                let (meta_start, meta_end) = self.track_scroll.visible_range(
                    bundle.tracks.len(),
                    ROW_HEIGHT,
                    ROW_SPACING,
                    scroll::LIST_OVERSCAN_BACK,
                    scroll::LIST_OVERSCAN_AHEAD + 8,
                );
                let mut jobs = Vec::new();
                for index in meta_start..meta_end {
                    if jobs.len() >= preview::META_BATCH_CAP {
                        break;
                    }
                    let Some(track) = bundle.tracks.get_mut(index) else {
                        continue;
                    };
                    if !track.meta_requested && track.needs_meta_enrichment() {
                        track.meta_requested = true;
                        jobs.push((index, track.url.clone()));
                    }
                }
                if !jobs.is_empty() {
                    self.meta_enriching = true;
                    tasks.push(Task::perform(
                        preview::fetch_tracks_meta(generation, jobs),
                        |(generation, items)| Message::MetaBatchLoaded { generation, items },
                    ));
                }
            }
        }

        Task::batch(tasks)
    }

    fn remember_directory(&mut self) {
        let _ = config::save_output_dir(self.output_dir.trim());
    }

    fn remember_audio_format(&mut self) {
        let _ = config::save_audio_format(self.audio_format);
    }

    fn remember_download_workers(&mut self) {
        let _ = config::save_download_workers(self.download_workers);
    }

    fn remember_enrich_preview_meta(&mut self) {
        let _ = config::save_enrich_preview_meta(self.enrich_preview_meta);
    }

    pub fn language_code(&self) -> &'static str {
        current_language().code()
    }
}
