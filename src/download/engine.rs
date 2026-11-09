use futures::Stream;
use futures::channel::mpsc::{UnboundedSender, unbounded};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::i18n::{Tr, t, tf};

const SEP: char = '\u{1f}';
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const CONCURRENT_FRAGMENTS: &str = "4";
const MAX_DOWNLOAD_WORKERS: usize = 8;

pub mod stage {
    pub const PREPARING: &str = "preparing";
    pub const DOWNLOADING: &str = "downloading";
    pub const PROCESSING: &str = "processing";
    pub const DONE: &str = "done";
}

fn download_worker_count(item_count: usize) -> usize {
    let cpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let target = cpus.saturating_sub(1).max(2);
    target.min(MAX_DOWNLOAD_WORKERS).min(item_count.max(1))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioFormat {
    #[default]
    M4a,
    Mp3,
    Opus,
    Flac,
    Wav,
}

impl AudioFormat {
    pub const ALL: [Self; 5] = [Self::M4a, Self::Mp3, Self::Opus, Self::Flac, Self::Wav];

    pub fn label(self) -> &'static str {
        match self {
            Self::M4a => "M4A",
            Self::Mp3 => "MP3",
            Self::Opus => "Opus",
            Self::Flac => "FLAC",
            Self::Wav => "WAV",
        }
    }

    pub fn ytdlp_name(self) -> &'static str {
        match self {
            Self::M4a => "m4a",
            Self::Mp3 => "mp3",
            Self::Opus => "opus",
            Self::Flac => "flac",
            Self::Wav => "wav",
        }
    }

    pub fn format_selector(self) -> &'static str {
        match self {
            Self::M4a => "bestaudio[ext=m4a]/bestaudio/best",
            Self::Opus => "bestaudio[ext=webm]/bestaudio/best",
            Self::Mp3 | Self::Flac | Self::Wav => "bestaudio/best",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolKind {
    YtDlp,
    Ffmpeg,
}

impl ToolKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::YtDlp => "yt-dlp",
            Self::Ffmpeg => "ffmpeg",
        }
    }

    pub fn winget_id(self) -> &'static str {
        match self {
            Self::YtDlp => "yt-dlp.yt-dlp",
            Self::Ffmpeg => "Gyan.FFmpeg",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolStatus {
    pub ytdlp: Option<String>,
    pub ffmpeg: Option<String>,
}

impl ToolStatus {
    pub fn detect() -> Self {
        refresh_path();
        Self {
            ytdlp: command_version("yt-dlp", &["--version"], parse_ytdlp_version),
            ffmpeg: command_version("ffmpeg", &["-version"], parse_ffmpeg_version),
        }
    }

    pub fn ready(&self) -> bool {
        self.ytdlp.is_some() && self.ffmpeg.is_some()
    }

    pub fn has(&self, kind: ToolKind) -> bool {
        match kind {
            ToolKind::YtDlp => self.ytdlp.is_some(),
            ToolKind::Ffmpeg => self.ffmpeg.is_some(),
        }
    }

    pub fn version(&self, kind: ToolKind) -> Option<&str> {
        match kind {
            ToolKind::YtDlp => self.ytdlp.as_deref(),
            ToolKind::Ffmpeg => self.ffmpeg.as_deref(),
        }
    }

}

pub async fn detect_tools() -> ToolStatus {
    let (tx, rx) = futures::channel::oneshot::channel();
    thread::spawn(move || {
        let _ = tx.send(ToolStatus::detect());
    });
    rx.await.unwrap_or(ToolStatus {
        ytdlp: None,
        ffmpeg: None,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolInstallResult {
    Ok,
    Failed(String),
}

pub async fn install_tool(kind: ToolKind) -> ToolInstallResult {
    let (tx, rx) = futures::channel::oneshot::channel();
    thread::spawn(move || {
        let _ = tx.send(install_tool_blocking(kind));
    });
    rx.await.unwrap_or_else(|_| {
        ToolInstallResult::Failed(t(Tr::ToolInstallWaitFailed).into())
    })
}

fn install_tool_blocking(kind: ToolKind) -> ToolInstallResult {
    let mut cmd = Command::new("winget");
    cmd.args([
        "install",
        "--id",
        kind.winget_id(),
        "-e",
        "--accept-package-agreements",
        "--accept-source-agreements",
    ])
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = cmd.creation_flags(CREATE_NO_WINDOW);
    }

    match cmd.output() {
        Ok(output) if output.status.success() => {
            refresh_path();
            ToolInstallResult::Ok
        }
        Ok(output) => {
            let stdout = decode_output(&output.stdout);
            let stderr = decode_output(&output.stderr);
            let detail = [stdout.trim(), stderr.trim()]
                .into_iter()
                .find(|line| !line.is_empty())
                .unwrap_or("winget failed")
                .to_string();
            refresh_path();
            if ToolStatus::detect().has(kind) {
                ToolInstallResult::Ok
            } else {
                ToolInstallResult::Failed(detail)
            }
        }
        Err(err) => ToolInstallResult::Failed(tf(
            Tr::ToolWingetStartFailed,
            &[("err", &err.to_string())],
        )),
    }
}

fn refresh_path() {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut cmd = Command::new("powershell");
        cmd.args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "[Environment]::GetEnvironmentVariable('Path','Machine') + ';' + [Environment]::GetEnvironmentVariable('Path','User')",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW);
        if let Ok(output) = cmd.output()
            && output.status.success()
        {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                unsafe { std::env::set_var("PATH", path) };
            }
        }
    }
}

fn command_version(
    program: &str,
    args: &[&str],
    parse: fn(&str) -> Option<String>,
) -> Option<String> {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let output = cmd.output().ok()?;
    if !output.status.success() && output.stdout.is_empty() && output.stderr.is_empty() {
        return None;
    }
    let text = {
        let stdout = decode_output(&output.stdout);
        if stdout.trim().is_empty() {
            decode_output(&output.stderr)
        } else {
            stdout
        }
    };
    parse(&text).or_else(|| {
        text.lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .map(str::to_string)
    })
}

fn parse_ytdlp_version(raw: &str) -> Option<String> {
    raw.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

fn parse_ffmpeg_version(raw: &str) -> Option<String> {
    let line = raw.lines().map(str::trim).find(|line| !line.is_empty())?;
    let rest = line.strip_prefix("ffmpeg version").unwrap_or(line).trim();
    let version = rest
        .split_whitespace()
        .next()
        .filter(|token| !token.is_empty())?;
    Some(version.to_string())
}

#[derive(Debug, Clone, PartialEq)]
pub enum DownloadEvent {
    Progress {
        track_percent: Option<f32>,
        stage: Option<String>,
        item_total: Option<u32>,
        tracks_done: Option<u32>,
    },
    Finished(Finish),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Finish {
    Success,
    Partial { failed: Vec<DownloadItem> },
    Failed,
    Cancelled,
}

pub struct DownloadControl {
    cancel_flag: Arc<AtomicBool>,
    active_pids: Arc<Mutex<Vec<u32>>>,
}

impl DownloadControl {
    pub fn cancel(&self) {
        self.cancel_flag.store(true, Ordering::SeqCst);
        if let Ok(pids) = self.active_pids.lock() {
            for &pid in pids.iter() {
                if pid != 0 {
                    kill_tree(pid);
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadItem {
    pub url: String,
}

pub fn start(
    items: Vec<DownloadItem>,
    output_dir: PathBuf,
    subfolder: Option<String>,
    audio_format: AudioFormat,
) -> (
    DownloadControl,
    impl Stream<Item = DownloadEvent> + Send + 'static,
) {
    let cancel_flag = Arc::new(AtomicBool::new(false));
    let active_pids = Arc::new(Mutex::new(Vec::new()));
    let control = DownloadControl {
        cancel_flag: Arc::clone(&cancel_flag),
        active_pids: Arc::clone(&active_pids),
    };
    let (tx, rx) = unbounded();

    thread::spawn(move || {
        let outcome = run_download(
            items,
            output_dir,
            subfolder.as_deref(),
            audio_format,
            cancel_flag,
            active_pids,
            tx.clone(),
        );
        let _ = tx.unbounded_send(DownloadEvent::Finished(outcome));
    });

    (control, rx)
}

fn download_target_dir(output_dir: &Path, subfolder: Option<&str>) -> PathBuf {
    match subfolder.map(sanitize_folder_name).filter(|name| !name.is_empty()) {
        Some(folder) => output_dir.join(folder),
        None => output_dir.to_path_buf(),
    }
}

fn cleanup_download_leftovers(dir: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };

    let mut removed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() || !is_download_leftover(&path) {
            continue;
        }
        if std::fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    removed
}

fn is_download_leftover(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let lower = name.to_ascii_lowercase();

    const TEMP_SUFFIXES: &[&str] = &[
        ".part",
        ".ytdl",
        ".aria2",
        ".tmp",
        ".temp",
        ".frag",
        ".frag.urls",
        ".info.json",
        ".description",
        ".annotations.xml",
        ".live_chat.json",
        ".meta",
        ".dump",
        ".unknown_video",
    ];
    if TEMP_SUFFIXES.iter().any(|suffix| lower.ends_with(suffix)) {
        return true;
    }
    if lower.contains(".part") || lower.contains(".frag") || lower.contains(".temp.") {
        return true;
    }

    if matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some("webp" | "jpg" | "jpeg" | "png")
    ) {
        return true;
    }

    if has_format_id_component(&lower) {
        return true;
    }

    if matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some("m4a" | "mp3" | "opus" | "webm" | "m4b" | "ogg" | "flac" | "wav")
    ) {
        if let Ok(meta) = path.metadata() {
            return meta.len() == 0;
        }
    }
    false
}

fn has_format_id_component(name: &str) -> bool {
    let bytes = name.as_bytes();
    let mut i = 0;
    while i + 3 < bytes.len() {
        if bytes[i] == b'.' && (bytes[i + 1] == b'f' || bytes[i + 1] == b'F') {
            let mut j = i + 2;
            if j < bytes.len() && bytes[j].is_ascii_digit() {
                while j < bytes.len() && bytes[j].is_ascii_digit() {
                    j += 1;
                }
                if j < bytes.len() && bytes[j] == b'.' {
                    return true;
                }
            }
        }
        i += 1;
    }
    false
}

pub fn sanitize_folder_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|ch| match ch {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.');
    if trimmed.is_empty() {
        "playlist".into()
    } else {
        trimmed.chars().take(80).collect()
    }
}

pub fn is_youtube_url(url: &str) -> bool {
    let rest = without_scheme(url.trim());
    if rest.is_empty() || rest.contains(char::is_whitespace) {
        return false;
    }

    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.trim_start_matches("www.");
    matches!(
        host,
        "youtube.com" | "m.youtube.com" | "music.youtube.com" | "youtu.be" | "youtube-nocookie.com"
    )
}

pub fn is_playlist_url(url: &str) -> bool {
    if !is_youtube_url(url) {
        return false;
    }

    let lower = without_scheme(url.trim()).to_ascii_lowercase();
    let is_watch = lower.contains("/watch") || lower.contains("youtu.be/");
    let is_playlist_path = lower.contains("/playlist") || lower.contains("/browse/");
    is_playlist_path || (lower.contains("list=") && !is_watch)
}

pub fn normalize_url(url: &str) -> String {
    let url = url.trim();
    if url.starts_with("https://") || url.starts_with("http://") {
        url.to_string()
    } else {
        format!("https://{url}")
    }
}

fn without_scheme(url: &str) -> &str {
    url.strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url)
}

fn run_download(
    items: Vec<DownloadItem>,
    output_dir: PathBuf,
    subfolder: Option<&str>,
    audio_format: AudioFormat,
    cancel_flag: Arc<AtomicBool>,
    active_pids: Arc<Mutex<Vec<u32>>>,
    tx: UnboundedSender<DownloadEvent>,
) -> Finish {
    if cancel_flag.load(Ordering::SeqCst) {
        return Finish::Cancelled;
    }

    if items.is_empty() {
        return Finish::Failed;
    }

    let target_dir = download_target_dir(&output_dir, subfolder);
    let _ = std::fs::create_dir_all(&target_dir);
    let _ = cleanup_download_leftovers(&target_dir);

    let total = items.len() as u32;
    let tracks_done = Arc::new(AtomicU32::new(0));
    let failures = Arc::new(AtomicU32::new(0));
    let saved_any = Arc::new(AtomicBool::new(false));
    let cancelled = Arc::new(AtomicBool::new(false));
    let failed_items = Arc::new(Mutex::new(Vec::<DownloadItem>::new()));

    let _ = tx.unbounded_send(DownloadEvent::Progress {
        track_percent: Some(0.0),
        stage: Some(stage::PREPARING.into()),
        item_total: Some(total),
        tracks_done: Some(0),
    });

    let workers = download_worker_count(items.len());
    let next_index = Arc::new(AtomicU32::new(0));
    let items = Arc::new(items);
    let output_dir = Arc::new(output_dir);
    let subfolder = Arc::new(subfolder.map(str::to_string));
    let mut joins = Vec::with_capacity(workers);

    for _ in 0..workers {
        let cancel_flag = Arc::clone(&cancel_flag);
        let active_pids = Arc::clone(&active_pids);
        let tx = tx.clone();
        let tracks_done = Arc::clone(&tracks_done);
        let failures = Arc::clone(&failures);
        let saved_any = Arc::clone(&saved_any);
        let cancelled = Arc::clone(&cancelled);
        let failed_items = Arc::clone(&failed_items);
        let next_index = Arc::clone(&next_index);
        let items = Arc::clone(&items);
        let output_dir = Arc::clone(&output_dir);
        let subfolder = Arc::clone(&subfolder);

        joins.push(thread::spawn(move || {
            loop {
                if cancel_flag.load(Ordering::SeqCst) {
                    cancelled.store(true, Ordering::SeqCst);
                    break;
                }
                let index = next_index.fetch_add(1, Ordering::SeqCst) as usize;
                if index >= items.len() {
                    break;
                }
                let item = &items[index];
                let _ = tx.unbounded_send(DownloadEvent::Progress {
                    track_percent: Some(0.0),
                    stage: Some(stage::DOWNLOADING.into()),
                    item_total: Some(total),
                    tracks_done: Some(tracks_done.load(Ordering::Relaxed)),
                });

                match run_one(
                    item,
                    output_dir.as_path(),
                    subfolder.as_ref().as_deref(),
                    audio_format,
                    &cancel_flag,
                    &active_pids,
                    &tx,
                    total,
                    &tracks_done,
                ) {
                    Finish::Success => {
                        saved_any.store(true, Ordering::Relaxed);
                        let done = tracks_done.fetch_add(1, Ordering::Relaxed) + 1;
                        let _ = tx.unbounded_send(DownloadEvent::Progress {
                            track_percent: Some(100.0),
                            stage: Some(stage::DONE.into()),
                            item_total: Some(total),
                            tracks_done: Some(done),
                        });
                    }
                    Finish::Partial { failed } => {
                        saved_any.store(true, Ordering::Relaxed);
                        failures.fetch_add(1, Ordering::Relaxed);
                        tracks_done.fetch_add(1, Ordering::Relaxed);
                        if let Ok(mut list) = failed_items.lock() {
                            list.extend(failed);
                        }
                    }
                    Finish::Failed => {
                        failures.fetch_add(1, Ordering::Relaxed);
                        if let Ok(mut list) = failed_items.lock() {
                            list.push(item.clone());
                        }
                    }
                    Finish::Cancelled => {
                        cancelled.store(true, Ordering::SeqCst);
                        break;
                    }
                }
            }
        }));
    }

    for handle in joins {
        let _ = handle.join();
    }

    let _ = cleanup_download_leftovers(&target_dir);

    if cancelled.load(Ordering::SeqCst) || cancel_flag.load(Ordering::SeqCst) {
        return Finish::Cancelled;
    }

    let done = tracks_done.load(Ordering::Relaxed);
    let failed = failures.load(Ordering::Relaxed);
    let failed_list = failed_items
        .lock()
        .map(|mut list| std::mem::take(&mut *list))
        .unwrap_or_default();
    if saved_any.load(Ordering::Relaxed) && failed == 0 && done >= total {
        Finish::Success
    } else if saved_any.load(Ordering::Relaxed) {
        Finish::Partial {
            failed: failed_list,
        }
    } else {
        Finish::Failed
    }
}

fn run_one(
    item: &DownloadItem,
    output_dir: &Path,
    subfolder: Option<&str>,
    audio_format: AudioFormat,
    cancel_flag: &AtomicBool,
    active_pids: &Mutex<Vec<u32>>,
    tx: &UnboundedSender<DownloadEvent>,
    item_total: u32,
    tracks_done: &Arc<AtomicU32>,
) -> Finish {
    let mut child = match ytdlp_command(
        &[item.url.as_str()],
        output_dir,
        subfolder,
        audio_format,
    )
    .spawn()
    {
        Ok(child) => child,
        Err(_) => {
            return Finish::Failed;
        }
    };

    let child_pid = child.id();
    register_pid(active_pids, child_pid);
    let _job = bind_kill_on_exit(&child);

    if cancel_flag.load(Ordering::SeqCst) {
        kill_tree(child_pid);
        let _ = child.wait();
        unregister_pid(active_pids, child_pid);
        return Finish::Cancelled;
    }

    let saved = Arc::new(AtomicU32::new(0));
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stderr_tx = tx.clone();
    let stderr_saved = Arc::clone(&saved);
    let context = ProgressContext {
        item_total,
        tracks_done: Arc::clone(tracks_done),
    };
    let stderr_context = context.clone();
    let stderr_thread = thread::spawn(move || {
        if let Some(stderr) = stderr {
            pump(stderr, &stderr_tx, &stderr_saved, stderr_context);
        }
    });

    if let Some(stdout) = stdout {
        pump(stdout, tx, &saved, context);
    }
    let _ = stderr_thread.join();

    let status = match child.wait() {
        Ok(status) => status,
        Err(_) => {
            unregister_pid(active_pids, child_pid);
            return Finish::Failed;
        }
    };
    unregister_pid(active_pids, child_pid);

    if cancel_flag.load(Ordering::SeqCst) {
        return Finish::Cancelled;
    }

    if status.success() || saved.load(Ordering::Relaxed) > 0 {
        Finish::Success
    } else {
        Finish::Failed
    }
}

#[derive(Clone)]
struct ProgressContext {
    item_total: u32,
    tracks_done: Arc<AtomicU32>,
}

fn register_pid(active_pids: &Mutex<Vec<u32>>, pid: u32) {
    if let Ok(mut pids) = active_pids.lock() {
        pids.push(pid);
    }
}

fn unregister_pid(active_pids: &Mutex<Vec<u32>>, pid: u32) {
    if let Ok(mut pids) = active_pids.lock() {
        pids.retain(|&existing| existing != pid);
    }
}

fn ytdlp_command(
    urls: &[&str],
    output_dir: &Path,
    subfolder: Option<&str>,
    audio_format: AudioFormat,
) -> Command {
    let template = output_template(output_dir, subfolder);
    let progress = format!("download:YTDLP{SEP}%(progress._percent_str)s");
    let postprocess = "postprocess:POST";

    let mut cmd = Command::new("yt-dlp");
    cmd.arg("--no-colors")
        .arg("--newline")
        .arg("--progress")
        .arg("--windows-filenames")
        .arg("-f")
        .arg(audio_format.format_selector())
        .arg("-x")
        .arg("--audio-format")
        .arg(audio_format.ytdlp_name())
        .arg("-N")
        .arg(CONCURRENT_FRAGMENTS)
        .arg("--embed-thumbnail")
        .arg("--embed-metadata")
        .arg("--no-embed-chapters")
        .arg("--no-embed-info-json")
        .arg("--no-overwrites")
        .arg("--replace-in-metadata")
        .arg("title")
        .arg("^Album\\s*[-–—]\\s*")
        .arg("")
        .arg("--parse-metadata")
        .arg("title:%(artist)s - %(title)s")
        .arg("--parse-metadata")
        .arg(":(?P<meta_synopsis>)")
        .arg("--parse-metadata")
        .arg("%(webpage_url)s:%(meta_comment)s")
        .arg("--replace-in-metadata")
        .arg("album")
        .arg("^Album\\s*[-–—]\\s*")
        .arg("")
        // Clear genre: YouTube category "Music" must not become a tag.
        .arg("--parse-metadata")
        .arg(":(?P<meta_genre>)");

    // Prefer this video's artists over title-parsed / Topic-channel rows.
    // `%(artists|)l` stays empty when missing — plain `%(artists)l` would write "NA"
    // and wipe a correct artist parsed from "Artist - Title" on custom playlists.
    cmd.arg("--parse-metadata")
        .arg("%(artists|)l:%(artist)s")
        .arg("--replace-in-metadata")
        .arg("artist")
        .arg(r"^(.+?)(?:[,;]\s*\1)+[,;]?\s*$")
        .arg(r"\1")
        .arg("--replace-in-metadata")
        .arg("artist")
        .arg(r"(?i)\s*[-–—]\s*Topic\s*$")
        .arg("")
        .arg("--parse-metadata")
        .arg("%(artist)s:%(meta_artist)s");

    cmd.arg("--retries")
        .arg("10")
        .arg("--fragment-retries")
        .arg("10")
        .arg("--progress-template")
        .arg(progress)
        .arg("--progress-template")
        .arg(postprocess)
        .arg("--no-playlist");

    if urls.len() > 1 {
        cmd.arg("--ignore-errors");
    }

    cmd.arg("-o").arg(template).arg("--");
    for url in urls {
        cmd.arg(url);
    }
    cmd.env("PYTHONUTF8", "1")
        .env("PYTHONIOENCODING", "utf-8")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = cmd.creation_flags(CREATE_NO_WINDOW);
    }

    cmd
}

fn output_template(dir: &Path, subfolder: Option<&str>) -> String {
    let base = dir.to_string_lossy().replace('\\', "/");
    let base = base.trim_end_matches('/');
    match subfolder.map(sanitize_folder_name).filter(|name| !name.is_empty()) {
        Some(folder) => format!("{base}/{folder}/%(title)s.%(ext)s"),
        None => format!("{base}/%(title)s.%(ext)s"),
    }
}

pub fn decode_output(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => decode_ansi(bytes),
    }
}

#[cfg(windows)]
fn decode_ansi(bytes: &[u8]) -> String {
    use windows_sys::Win32::Globalization::MultiByteToWideChar;

    if bytes.is_empty() {
        return String::new();
    }

    unsafe {
        let wide_len = MultiByteToWideChar(
            0,
            0,
            bytes.as_ptr(),
            bytes.len() as i32,
            std::ptr::null_mut(),
            0,
        );
        if wide_len <= 0 {
            return String::from_utf8_lossy(bytes).into_owned();
        }

        let mut wide = vec![0u16; wide_len as usize];
        let written = MultiByteToWideChar(
            0,
            0,
            bytes.as_ptr(),
            bytes.len() as i32,
            wide.as_mut_ptr(),
            wide_len,
        );
        if written <= 0 {
            return String::from_utf8_lossy(bytes).into_owned();
        }
        String::from_utf16_lossy(&wide[..written as usize])
    }
}

#[cfg(not(windows))]
fn decode_ansi(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn pump(
    reader: impl Read,
    tx: &UnboundedSender<DownloadEvent>,
    saved: &AtomicU32,
    context: ProgressContext,
) {
    let mut reader = BufReader::new(reader);
    let mut buf = Vec::new();
    let mut last_progress = None;

    loop {
        buf.clear();
        match reader.read_until(b'\n', &mut buf) {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                while matches!(buf.last(), Some(b'\n' | b'\r')) {
                    buf.pop();
                }
                if buf.is_empty() {
                    continue;
                }

                let line = decode_output(&buf);
                if line.contains("[ExtractAudio]") || line.contains("already been downloaded") {
                    saved.fetch_add(1, Ordering::Relaxed);
                }
                for event in events_for_line(&line, context.clone()) {
                    if let DownloadEvent::Progress {
                        track_percent: Some(percent),
                        ..
                    } = &event
                    {
                        let now = std::time::Instant::now();
                        let too_soon = last_progress.is_some_and(|previous| {
                            now.duration_since(previous) < Duration::from_millis(80)
                        });
                        if too_soon && *percent < 100.0 {
                            continue;
                        }
                        last_progress = Some(now);
                    }

                    if tx.unbounded_send(event).is_err() {
                        return;
                    }
                }
            }
        }
    }
}

fn events_for_line(line: &str, context: ProgressContext) -> Vec<DownloadEvent> {
    let line = line.trim();
    if line.is_empty() {
        return Vec::new();
    }

    if let Some(rest) = line.strip_prefix("YTDLP")
        && let Some(rest) = rest.strip_prefix(SEP)
    {
        return vec![DownloadEvent::Progress {
            track_percent: parse_percent(rest),
            stage: Some(stage::DOWNLOADING.into()),
            item_total: Some(context.item_total),
            tracks_done: Some(context.tracks_done.load(Ordering::Relaxed)),
        }];
    }

    if line == "POST" {
        return vec![DownloadEvent::Progress {
            track_percent: Some(100.0),
            stage: Some(stage::PROCESSING.into()),
            item_total: Some(context.item_total),
            tracks_done: Some(context.tracks_done.load(Ordering::Relaxed)),
        }];
    }

    if let Some(percent) = parse_default_progress(line) {
        return vec![DownloadEvent::Progress {
            track_percent: percent,
            stage: Some(stage::DOWNLOADING.into()),
            item_total: Some(context.item_total),
            tracks_done: Some(context.tracks_done.load(Ordering::Relaxed)),
        }];
    }

    if phase_status(line) {
        return vec![DownloadEvent::Progress {
            track_percent: Some(100.0),
            stage: Some(stage::PROCESSING.into()),
            item_total: Some(context.item_total),
            tracks_done: Some(context.tracks_done.load(Ordering::Relaxed)),
        }];
    }

    Vec::new()
}

fn parse_default_progress(line: &str) -> Option<Option<f32>> {
    let rest = line.trim().strip_prefix("[download]")?.trim();
    if rest.starts_with("Downloading") || rest.starts_with("Destination") {
        return None;
    }
    let (percent_token, _) = rest.split_once('%')?;
    let percent_token = percent_token.trim();
    if percent_token.is_empty() {
        return None;
    }
    if !percent_token
        .chars()
        .all(|ch| ch.is_ascii_digit() || ch == '.' || ch == ' ')
        && !percent_token.eq_ignore_ascii_case("unknown")
    {
        return None;
    }

    Some(parse_percent(percent_token))
}

fn phase_status(line: &str) -> bool {
    line.contains("[ExtractAudio]")
        || line.contains("[EmbedThumbnail]")
        || line.contains("[Metadata]")
}

fn parse_percent(token: &str) -> Option<f32> {
    let token = token.trim().trim_end_matches('%').trim();
    if token.is_empty() || token.eq_ignore_ascii_case("unknown") || token.eq_ignore_ascii_case("na")
    {
        return None;
    }
    token
        .parse::<f32>()
        .ok()
        .map(|value| value.clamp(0.0, 100.0))
}

fn kill_tree(pid: u32) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = Command::new("taskkill")
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .status();
    }
    #[cfg(not(windows))]
    {
        let _ = Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .status();
    }
}

struct JobGuard {
    #[cfg(windows)]
    handle: windows_sys::Win32::Foundation::HANDLE,
}

impl Drop for JobGuard {
    fn drop(&mut self) {
        #[cfg(windows)]
        unsafe {
            if !self.handle.is_null() {
                let _ = windows_sys::Win32::Foundation::CloseHandle(self.handle);
            }
        }
    }
}

fn bind_kill_on_exit(child: &Child) -> JobGuard {
    #[cfg(windows)]
    {
        bind_windows_job(child)
    }
    #[cfg(not(windows))]
    {
        let _ = child;
        JobGuard {}
    }
}

#[cfg(windows)]
fn bind_windows_job(child: &Child) -> JobGuard {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject,
    };

    unsafe {
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job.is_null() {
            return JobGuard {
                handle: std::ptr::null_mut(),
            };
        }

        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let configured = SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            (&raw const info).cast(),
            std::mem::size_of_val(&info) as u32,
        );
        if configured == 0 {
            let _ = CloseHandle(job);
            return JobGuard {
                handle: std::ptr::null_mut(),
            };
        }

        let assigned = AssignProcessToJobObject(job, child.as_raw_handle());
        if assigned == 0 {
            let _ = CloseHandle(job);
            return JobGuard {
                handle: std::ptr::null_mut(),
            };
        }

        JobGuard { handle: job }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_youtube_links() {
        assert!(is_youtube_url(
            "https://www.youtube.com/watch?v=fxVideo0001"
        ));
        assert!(is_youtube_url("https://youtu.be/fxVideo0001"));
        assert!(is_youtube_url("music.youtube.com/watch?v=fxVideo0001"));
        assert!(!is_youtube_url("https://example.com/watch?v=fxVideo0001"));
        assert!(!is_youtube_url("not a url"));
    }

    #[test]
    fn detects_playlists_and_leaves_watch_links_as_tracks() {
        assert!(is_playlist_url(
            "https://www.youtube.com/playlist?list=PLfixtureList01"
        ));
        assert!(is_playlist_url(
            "https://music.youtube.com/playlist?list=PLfixtureAlbum01"
        ));
        assert!(is_playlist_url(
            "https://music.youtube.com/browse/MPREbfixture01"
        ));
        assert!(!is_playlist_url(
            "https://www.youtube.com/watch?v=fxVideo0001&list=PLfixtureList01"
        ));
        assert!(!is_playlist_url(
            "https://youtu.be/fxVideo0001?list=PLfixtureList01"
        ));
        assert!(!is_playlist_url(
            "https://music.youtube.com/watch?v=fxVideo0001"
        ));
    }

    fn ctx() -> ProgressContext {
        ProgressContext {
            item_total: 3,
            tracks_done: Arc::new(AtomicU32::new(0)),
        }
    }

    #[test]
    fn parses_progress_marker() {
        let line = format!("YTDLP{SEP}  42.5%");
        let events = events_for_line(&line, ctx());
        assert_eq!(
            events,
            vec![DownloadEvent::Progress {
                track_percent: Some(42.5),
                stage: Some(stage::DOWNLOADING.into()),
                item_total: Some(3),
                tracks_done: Some(0),
            }]
        );
    }

    #[test]
    fn parses_default_percent_and_ignores_item_noise() {
        assert!(events_for_line("[download] Downloading item 2 of 10", ctx()).is_empty());

        let events =
            events_for_line("[download]  15.0% of 3.00MiB at 1.00MiB/s ETA 00:02", ctx());
        assert_eq!(
            events,
            vec![DownloadEvent::Progress {
                track_percent: Some(15.0),
                stage: Some(stage::DOWNLOADING.into()),
                item_total: Some(3),
                tracks_done: Some(0),
            }]
        );
    }

    #[cfg(windows)]
    #[test]
    fn decodes_windows_ansi_cyrillic() {
        let ansi = [
            0xD4u8, 0xE8, 0xEA, 0xF1, 0xF2, 0xF3, 0xF0, 0xE0, 0x20, 0x2D, 0x20, 0xD2, 0xF0, 0xE5,
            0xEA,
        ];
        assert_eq!(decode_output(&ansi), "Фикстура - Трек");
    }

    #[test]
    fn keeps_utf8_output() {
        assert_eq!(decode_output("Уже UTF-8".as_bytes()), "Уже UTF-8");
    }

    #[test]
    fn maps_metadata_phase_to_processing() {
        let events = events_for_line("[Metadata] Adding metadata to 'song.opus'", ctx());
        assert_eq!(
            events,
            vec![DownloadEvent::Progress {
                track_percent: Some(100.0),
                stage: Some(stage::PROCESSING.into()),
                item_total: Some(3),
                tracks_done: Some(0),
            }]
        );
    }

    #[test]
    fn detects_download_leftover_names() {
        assert!(is_download_leftover(Path::new(r"C:\Music\Song.m4a.part")));
        assert!(is_download_leftover(Path::new(r"C:\Music\Song.m4a.ytdl")));
        assert!(is_download_leftover(Path::new(
            r"C:\Music\Song.m4a.part-Frag1"
        )));
        assert!(is_download_leftover(Path::new(r"C:\Music\Song.webp")));
        assert!(is_download_leftover(Path::new(r"C:\Music\Song.jpg")));
        assert!(is_download_leftover(Path::new(r"C:\Music\Song.info.json")));
        assert!(is_download_leftover(Path::new(r"C:\Music\Song.f251.webm")));
        assert!(is_download_leftover(Path::new(r"C:\Music\Song.f140.m4a")));
        assert!(is_download_leftover(Path::new(r"C:\Music\Song.temp.m4a")));
        assert!(is_download_leftover(Path::new(r"C:\Music\Song.aria2")));
        assert!(!is_download_leftover(Path::new(r"C:\Music\Song.m4a")));
        assert!(!is_download_leftover(Path::new(r"C:\Music\notes.txt")));
    }

    #[test]
    fn cleans_leftovers_but_keeps_finished_audio() {
        let dir = std::env::temp_dir().join(format!(
            "yhits-leftover-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let keep = dir.join("Done.m4a");
        let part = dir.join("Fail.m4a.part");
        let ytdl = dir.join("Fail.m4a.ytdl");
        let webp = dir.join("Fail.webp");
        let jpg = dir.join("Fail.jpg");
        let fmt = dir.join("Fail.f251.webm");
        let json = dir.join("Fail.info.json");
        let empty = dir.join("Empty.m4a");
        std::fs::write(&keep, b"audio").unwrap();
        std::fs::write(&part, b"partial").unwrap();
        std::fs::write(&ytdl, b"{}").unwrap();
        std::fs::write(&webp, b"img").unwrap();
        std::fs::write(&jpg, b"img").unwrap();
        std::fs::write(&fmt, b"stream").unwrap();
        std::fs::write(&json, b"{}").unwrap();
        std::fs::write(&empty, b"").unwrap();

        assert_eq!(cleanup_download_leftovers(&dir), 7);
        assert!(keep.exists());
        assert!(!part.exists());
        assert!(!ytdl.exists());
        assert!(!webp.exists());
        assert!(!jpg.exists());
        assert!(!fmt.exists());
        assert!(!json.exists());
        assert!(!empty.exists());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
