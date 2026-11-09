use crate::download::{decode_output, is_playlist_url, normalize_url};
use crate::i18n::{Tr, t, tf};
use serde_json::Value;
use std::io::Read;
use std::process::{Command, Stdio};
use std::time::Duration;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const DEBOUNCE: Duration = Duration::from_millis(250);

#[derive(Debug, Clone)]
pub struct TrackPreview {
    pub url: String,
    pub title: String,
    pub artists: Vec<String>,
    pub album: String,
    pub duration_secs: Option<u64>,
    pub thumbnail_url: Option<String>,
    pub cover: Option<iced::widget::image::Handle>,
    pub cover_requested: bool,
    pub meta_requested: bool,
    pub selected: bool,
}

impl TrackPreview {
    pub fn needs_meta_enrichment(&self) -> bool {
        self.artists.is_empty() || self.album.trim().is_empty()
    }
}

#[derive(Debug, Clone)]
pub struct TrackMeta {
    pub artists: Vec<String>,
    pub album: String,
}

#[derive(Debug, Clone)]
pub struct PreviewBundle {
    pub source_title: Option<String>,
    pub shared_album: bool,
    pub tracks: Vec<TrackPreview>,
}

#[derive(Debug, Clone)]
pub enum PreviewOutcome {
    Ready(PreviewBundle),
    Failed(String),
    Cancelled,
}

pub async fn fetch_debounced(url: String, generation: u64) -> (u64, PreviewOutcome) {
    async_std_sleep(DEBOUNCE).await;
    (generation, fetch_preview(&url))
}

pub async fn fetch_cover(
    generation: u64,
    index: usize,
    url: String,
) -> (u64, usize, Option<iced::widget::image::Handle>) {
    let handle = download_image(&url).map(iced::widget::image::Handle::from_bytes);
    (generation, index, handle)
}

pub async fn fetch_tracks_meta(
    generation: u64,
    jobs: Vec<(usize, String)>,
) -> (u64, Vec<(usize, TrackMeta)>) {
    (generation, load_tracks_meta(&jobs))
}

pub async fn fetch_shared_album_meta(
    generation: u64,
    jobs: Vec<(usize, String)>,
) -> (u64, Vec<(usize, TrackMeta)>) {
    (generation, load_shared_album_meta(&jobs))
}

fn async_std_sleep(duration: Duration) -> impl std::future::Future<Output = ()> {
    async move {
        std::thread::sleep(duration);
    }
}

fn fetch_preview(url: &str) -> PreviewOutcome {
    let url = normalize_url(url);
    if url.trim().is_empty() {
        return PreviewOutcome::Cancelled;
    }

    let playlist = is_playlist_url(&url);
    let mut cmd = Command::new("yt-dlp");
    cmd.arg("-J")
        .arg("--flat-playlist")
        .arg("--no-download")
        .arg("--ignore-errors")
        .arg("--no-warnings")
        .arg("--no-colors");
    if playlist {
        cmd.arg("--yes-playlist");
    } else {
        cmd.arg("--no-playlist");
    }
    cmd.arg("--")
        .arg(&url)
        .env("PYTHONUTF8", "1")
        .env("PYTHONIOENCODING", "utf-8")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(err) => {
            return PreviewOutcome::Failed(tf(Tr::YtdlpStartFailed, &[("err", &err.to_string())]));
        }
    };

    let mut stdout = Vec::new();
    if let Some(mut pipe) = child.stdout.take() {
        let _ = pipe.read_to_end(&mut stdout);
    }
    let mut stderr = Vec::new();
    if let Some(mut pipe) = child.stderr.take() {
        let _ = pipe.read_to_end(&mut stderr);
    }

    let status = match child.wait() {
        Ok(status) => status,
        Err(err) => {
            return PreviewOutcome::Failed(tf(Tr::YtdlpExitFailed, &[("err", &err.to_string())]));
        }
    };

    let text = String::from_utf8_lossy(&stdout);
    let text = text.trim();
    if text.is_empty() || text == "null" {
        let err = String::from_utf8_lossy(&stderr);
        let message = err
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .unwrap_or(t(Tr::YtdlpListFailed))
            .to_string();
        return PreviewOutcome::Failed(message);
    }

    let value: Value = match serde_json::from_str(text) {
        Ok(value) => value,
        Err(err) => {
            if !status.success() {
                let err_text = String::from_utf8_lossy(&stderr);
                return PreviewOutcome::Failed(
                    err_text
                        .lines()
                        .rev()
                        .find(|line| !line.trim().is_empty())
                        .unwrap_or(t(Tr::YtdlpParseFailed))
                        .to_string(),
                );
            }
            return PreviewOutcome::Failed(tf(Tr::YtdlpJsonFailed, &[("err", &err.to_string())]));
        }
    };

    match parse_bundle(&value) {
        Ok(bundle) if bundle.tracks.is_empty() => {
            PreviewOutcome::Failed(t(Tr::NoTracksInLink).into())
        }
        Ok(mut bundle) => {
            if is_album_source_url(&url) {
                bundle.shared_album = true;
                if let Some(album) = bundle.source_title.clone() {
                    for track in &mut bundle.tracks {
                        if track.album.trim().is_empty() {
                            track.album = album.clone();
                        }
                    }
                }
            }
            PreviewOutcome::Ready(bundle)
        }
        Err(message) => PreviewOutcome::Failed(message),
    }
}

fn parse_bundle(value: &Value) -> Result<PreviewBundle, String> {
    let raw_title = value.get("title").and_then(Value::as_str).unwrap_or("");
    let titled_album = is_album_playlist_title(raw_title);
    let source_title = {
        let cleaned = clean_album_prefix(raw_title);
        (!cleaned.is_empty()).then_some(cleaned)
    };

    let webpage = value
        .get("webpage_url")
        .or_else(|| value.get("original_url"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let shared_album = titled_album || is_album_source_url(webpage);

    let mut tracks = if value.get("_type").and_then(Value::as_str) == Some("playlist")
        || value.get("entries").is_some()
    {
        value
            .get("entries")
            .and_then(Value::as_array)
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(parse_track)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    } else {
        parse_track(value).into_iter().collect()
    };

    if shared_album
        && let Some(album) = source_title.as_ref()
    {
        for track in &mut tracks {
            if track.album.trim().is_empty() {
                track.album = album.clone();
            }
        }
    }

    Ok(PreviewBundle {
        source_title,
        shared_album,
        tracks,
    })
}

fn parse_track(value: &Value) -> Option<TrackPreview> {
    if value.is_null() {
        return None;
    }

    let id = value
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())?
        .to_string();

    let raw_title = value
        .get("track")
        .and_then(Value::as_str)
        .or_else(|| value.get("title").and_then(Value::as_str))
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| t(Tr::UntitledTrack));

    let (title, artists) = resolve_title_artists(value, raw_title);
    let album = album_of(value);
    let duration_secs = value
        .get("duration")
        .and_then(Value::as_f64)
        .filter(|secs| secs.is_finite() && *secs > 0.0)
        .map(|secs| secs.round() as u64);
    let thumbnail_url = thumbnail_of(value, &id);
    let url = track_url(&id);

    Some(TrackPreview {
        url,
        title,
        artists,
        album,
        duration_secs,
        thumbnail_url,
        cover: None,
        cover_requested: false,
        meta_requested: false,
        selected: true,
    })
}

fn track_url(id: &str) -> String {
    format!("https://www.youtube.com/watch?v={id}")
}

const META_SEP: char = '\u{1f}';
const META_CHUNK_SIZE: usize = 1;
const META_MAX_WORKERS: usize = 8;
pub const META_BATCH_CAP: usize = META_CHUNK_SIZE * META_MAX_WORKERS;

fn is_album_playlist_title(title: &str) -> bool {
    let trimmed = title.trim();
    ["Album - ", "Album – ", "Album — "]
        .iter()
        .any(|prefix| trimmed.starts_with(prefix))
}

fn is_album_source_url(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    lower.contains("list=olak") || lower.contains("/browse/mpreb")
}

fn load_shared_album_meta(jobs: &[(usize, String)]) -> Vec<(usize, TrackMeta)> {
    let Some((_, seed_url)) = jobs.first() else {
        return Vec::new();
    };
    let Some((_, meta)) = load_tracks_meta_chunk(&[(0, seed_url.clone())])
        .into_iter()
        .next()
    else {
        return Vec::new();
    };
    jobs.iter()
        .map(|(index, _)| (*index, meta.clone()))
        .collect()
}

fn load_tracks_meta(jobs: &[(usize, String)]) -> Vec<(usize, TrackMeta)> {
    if jobs.is_empty() {
        return Vec::new();
    }
    if jobs.len() <= META_CHUNK_SIZE {
        return load_tracks_meta_chunk(jobs);
    }

    let mut out = Vec::with_capacity(jobs.len());
    let chunks: Vec<&[(usize, String)]> = jobs.chunks(META_CHUNK_SIZE).collect();
    for wave in chunks.chunks(META_MAX_WORKERS) {
        std::thread::scope(|scope| {
            let handles: Vec<_> = wave
                .iter()
                .map(|chunk| scope.spawn(|| load_tracks_meta_chunk(chunk)))
                .collect();
            for handle in handles {
                if let Ok(mut part) = handle.join() {
                    out.append(&mut part);
                }
            }
        });
    }
    out
}

fn load_tracks_meta_chunk(jobs: &[(usize, String)]) -> Vec<(usize, TrackMeta)> {
    if jobs.is_empty() {
        return Vec::new();
    }

    // `%(artists|)l` avoids the literal "NA" yt-dlp prints for missing Music artists.
    let template = format!("%(id)s{META_SEP}%(artists|)l{META_SEP}%(album)s");
    let mut cmd = Command::new("yt-dlp");
    cmd.arg("--skip-download")
        .arg("--no-playlist")
        .arg("--ignore-errors")
        .arg("--no-warnings")
        .arg("--no-colors")
        .arg("--print")
        .arg(template)
        .arg("--")
        .env("PYTHONUTF8", "1")
        .env("PYTHONIOENCODING", "utf-8")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    for (_, url) in jobs {
        cmd.arg(url);
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let Ok(output) = cmd.output() else {
        return Vec::new();
    };
    let text = decode_output(&output.stdout);
    let mut by_id = std::collections::HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split(META_SEP);
        let Some(id) = parts.next().map(str::trim).filter(|id| !id.is_empty()) else {
            continue;
        };
        let artists_raw = parts.next().unwrap_or("").trim();
        let album_raw = parts.next().unwrap_or("").trim();
        let mut artists = Vec::new();
        if !artists_raw.is_empty()
            && !artists_raw.eq_ignore_ascii_case("na")
            && artists_raw != "None"
        {
            push_artist_parts(&mut artists, artists_raw);
        }
        let album = if album_raw.is_empty()
            || album_raw.eq_ignore_ascii_case("na")
            || album_raw == "None"
        {
            String::new()
        } else {
            clean_album_prefix(album_raw)
        };
        if artists.is_empty() && album.is_empty() {
            continue;
        }
        by_id.insert(id.to_string(), TrackMeta { artists, album });
    }

    let mut out = Vec::new();
    for (index, url) in jobs {
        let Some(id) = video_id_from_url(url) else {
            continue;
        };
        if let Some(meta) = by_id.remove(&id) {
            out.push((*index, meta));
        }
    }
    out
}

fn video_id_from_url(url: &str) -> Option<String> {
    let url = url.trim();
    if let Some(v) = url.split("v=").nth(1) {
        let id = v.split(['&', '#', '/']).next()?.trim();
        if !id.is_empty() {
            return Some(id.to_string());
        }
    }
    if let Some(rest) = url.split("youtu.be/").nth(1) {
        let id = rest.split(['?', '&', '#', '/']).next()?.trim();
        if !id.is_empty() {
            return Some(id.to_string());
        }
    }
    None
}

fn resolve_title_artists(value: &Value, raw_title: &str) -> (String, Vec<String>) {
    let dedicated = dedicated_artists_of(value);
    if !dedicated.is_empty() {
        return (raw_title.to_string(), dedicated);
    }

    // Custom playlists often use "Artist - Title" while channel/uploader is a reuploader.
    if let Some((artist, song)) = split_artist_title(raw_title) {
        let mut artists = Vec::new();
        push_artist_parts(&mut artists, &artist);
        if !artists.is_empty() && !song.is_empty() {
            return (song, artists);
        }
    }

    (raw_title.to_string(), channel_artists_of(value))
}

fn dedicated_artists_of(value: &Value) -> Vec<String> {
    let mut names = Vec::new();

    if let Some(artists) = value.get("artists").and_then(Value::as_array) {
        for artist in artists {
            let Some(raw) = artist
                .as_str()
                .or_else(|| artist.get("name").and_then(Value::as_str))
            else {
                continue;
            };
            push_artist_parts(&mut names, raw);
        }
    }
    for key in ["artist", "creator"] {
        if let Some(raw) = value
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
        {
            push_artist_parts(&mut names, raw);
        }
    }

    names
}

fn channel_artists_of(value: &Value) -> Vec<String> {
    let mut names = Vec::new();
    for key in ["channel", "uploader"] {
        if let Some(raw) = value
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
        {
            push_artist_parts(&mut names, raw);
            if !names.is_empty() {
                break;
            }
        }
    }
    names
}

fn split_artist_title(title: &str) -> Option<(String, String)> {
    let trimmed = title.trim();
    for sep in [" - ", " – ", " — "] {
        if let Some((left, right)) = trimmed.split_once(sep) {
            let artist = left.trim();
            let song = right.trim();
            if !artist.is_empty() && !song.is_empty() {
                return Some((artist.to_string(), song.to_string()));
            }
        }
    }
    None
}

fn push_artist_parts(names: &mut Vec<String>, raw: &str) {
    for part in raw.split([',', ';', '/', '|']) {
        let name = clean_artist_name(part);
        if name.is_empty() {
            continue;
        }
        if !names
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(&name))
        {
            names.push(name);
        }
    }
}

fn clean_artist_name(raw: &str) -> String {
    let name = raw.trim().trim_matches(['"', '\'']);
    if name.is_empty() {
        return String::new();
    }
    for sep in [" - ", " – ", " — "] {
        if let Some((left, right)) = name.rsplit_once(sep) {
            if right.eq_ignore_ascii_case("topic") {
                return left.trim().to_string();
            }
        }
    }
    name.to_string()
}

fn album_of(value: &Value) -> String {
    let raw = match value.get("album") {
        Some(Value::String(text)) => text.as_str(),
        Some(Value::Object(object)) => object
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        _ => "",
    };
    clean_album_prefix(raw)
}

fn clean_album_prefix(value: &str) -> String {
    let trimmed = value.trim();
    let stripped = ["Album - ", "Album – ", "Album — "]
        .iter()
        .find_map(|prefix| trimmed.strip_prefix(prefix))
        .unwrap_or(trimmed)
        .trim();
    stripped.to_string()
}

fn thumbnail_of(value: &Value, id: &str) -> Option<String> {
    if let Some(thumbnails) = value.get("thumbnails").and_then(Value::as_array) {
        let mut best: Option<(i64, &str)> = None;
        for thumb in thumbnails {
            let url = thumb.get("url").and_then(Value::as_str)?;
            let width = thumb.get("width").and_then(Value::as_i64).unwrap_or(0);
            let score = if (180..500).contains(&width) {
                1_000 + width
            } else if width > 0 {
                width
            } else {
                1
            };
            if best.is_none_or(|(best_score, _)| score >= best_score) {
                best = Some((score, url));
            }
        }
        if let Some((_, url)) = best {
            return Some(url.to_string());
        }
    }

    if let Some(url) = value
        .get("thumbnail")
        .and_then(Value::as_str)
        .filter(|url| !url.is_empty())
    {
        return Some(url.to_string());
    }

    if id.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-') {
        return Some(format!("https://i.ytimg.com/vi/{id}/mqdefault.jpg"));
    }

    None
}

fn download_image(url: &str) -> Option<Vec<u8>> {
    let response = ureq::get(url)
        .set("User-Agent", "yt-audio/0.1")
        .call()
        .ok()?;
    let mut bytes = Vec::new();
    response.into_reader().read_to_end(&mut bytes).ok()?;
    if bytes.is_empty() {
        return None;
    }
    Some(square_cover_png(&bytes).unwrap_or(bytes))
}

fn square_cover_png(bytes: &[u8]) -> Option<Vec<u8>> {
    use image::imageops::{FilterType, crop_imm};
    use std::io::Cursor;

    let img = image::load_from_memory(bytes).ok()?.into_rgba8();
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return None;
    }
    let side = w.min(h);
    let x = (w - side) / 2;
    let y = (h - side) / 2;
    let square = crop_imm(&img, x, y, side, side).to_image();
    let mut resized = image::imageops::resize(&square, 96, 96, FilterType::Triangle);
    round_corners(&mut resized, 16);
    let mut out = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(resized)
        .write_to(&mut out, image::ImageFormat::Png)
        .ok()?;
    Some(out.into_inner())
}

fn round_corners(img: &mut image::RgbaImage, radius: u32) {
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 || radius == 0 {
        return;
    }
    let radius = radius.min(w / 2).min(h / 2) as f32;
    let r2 = radius * radius;
    for y in 0..h {
        for x in 0..w {
            let dx = if (x as f32) < radius {
                radius - x as f32
            } else if (x as f32) > w as f32 - 1.0 - radius {
                x as f32 - (w as f32 - 1.0 - radius)
            } else {
                0.0
            };
            let dy = if (y as f32) < radius {
                radius - y as f32
            } else if (y as f32) > h as f32 - 1.0 - radius {
                y as f32 - (h as f32 - 1.0 - radius)
            } else {
                0.0
            };
            if dx * dx + dy * dy > r2 {
                img.put_pixel(x, y, image::Rgba([0, 0, 0, 0]));
            }
        }
    }
}

pub fn format_duration(secs: u64) -> String {
    let hours = secs / 3600;
    let minutes = (secs % 3600) / 60;
    let seconds = secs % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_playlist_entries_and_album_prefix() {
        let value = json!({
            "_type": "playlist",
            "title": "Album - Harbor Lights",
            "entries": [
                {
                    "id": "fxTrack01",
                    "title": "Amber Pier",
                    "artists": [{"name": "Lumen Quay"}],
                    "album": {"name": "Album - Harbor Lights"},
                    "duration": 201,
                    "thumbnails": [
                        {"url": "https://cdn.example.test/cover-sm.jpg", "width": 120},
                        {"url": "https://cdn.example.test/cover-md.jpg", "width": 320}
                    ]
                },
                null
            ]
        });

        let bundle = parse_bundle(&value).unwrap();
        assert_eq!(bundle.source_title.as_deref(), Some("Harbor Lights"));
        assert!(bundle.shared_album);
        assert_eq!(bundle.tracks.len(), 1);
        let track = &bundle.tracks[0];
        assert_eq!(track.title, "Amber Pier");
        assert_eq!(track.artists, vec!["Lumen Quay".to_string()]);
        assert_eq!(track.album, "Harbor Lights");
        assert_eq!(track.duration_secs, Some(201));
        assert_eq!(
            track.thumbnail_url.as_deref(),
            Some("https://cdn.example.test/cover-md.jpg")
        );
        assert!(track.selected);
        assert!(track.url.contains("fxTrack01"));
    }

    #[test]
    fn seeds_album_title_onto_flat_rows() {
        let value = json!({
            "_type": "playlist",
            "title": "Album - Harbor Lights",
            "entries": [
                {
                    "id": "fxFlatAlbum01",
                    "title": "Amber Pier",
                    "uploader": "Lumen Quay - Topic",
                    "channel": "Lumen Quay - Topic"
                }
            ]
        });
        let bundle = parse_bundle(&value).unwrap();
        assert!(bundle.shared_album);
        assert_eq!(bundle.tracks[0].album, "Harbor Lights");
        assert_eq!(bundle.tracks[0].artists, vec!["Lumen Quay".to_string()]);
        assert!(!bundle.tracks[0].needs_meta_enrichment());
    }

    #[test]
    fn joins_and_dedupes_artists() {
        let track = parse_track(&json!({
            "id": "fxDuo",
            "title": "Shared Signal",
            "artists": [
                {"name": "Northline"},
                "Southline",
                {"name": "northline"}
            ]
        }))
        .unwrap();
        assert_eq!(
            track.artists,
            vec!["Northline".to_string(), "Southline".to_string()]
        );

        let track = parse_track(&json!({
            "id": "fxBlob",
            "title": "Sample",
            "artist": "Northline; Southline, northline / Southline"
        }))
        .unwrap();
        assert_eq!(
            track.artists,
            vec!["Northline".to_string(), "Southline".to_string()]
        );

        let track = parse_track(&json!({
            "id": "fxDupArtist",
            "title": "Quiet Compass",
            "artists": ["Северный маяк", "Северный маяк"],
            "artist": "Северный маяк, Северный маяк",
            "album": "Карты без маршрута"
        }))
        .unwrap();
        assert_eq!(track.artists, vec!["Северный маяк".to_string()]);
        assert_eq!(track.album, "Карты без маршрута");
    }

    #[test]
    fn strips_youtube_music_topic_suffix() {
        let track = parse_track(&json!({
            "id": "fxTopic",
            "title": "Fixture Song",
            "artist": "Северный маяк - Topic; Северный маяк - Topic"
        }))
        .unwrap();
        assert_eq!(track.artists, vec!["Северный маяк".to_string()]);
    }

    #[test]
    fn flat_playlist_uses_channel_as_provisional_artist() {
        let track = parse_track(&json!({
            "id": "fxFlat01",
            "title": "Shared Signal",
            "uploader": "Northline",
            "channel": "Northline",
            "url": "https://music.youtube.com/watch?v=fxFlat01"
        }))
        .unwrap();
        assert_eq!(track.artists, vec!["Northline".to_string()]);
        assert!(track.album.is_empty());
        assert_eq!(track.url, "https://www.youtube.com/watch?v=fxFlat01");
        assert!(track.needs_meta_enrichment());
    }

    #[test]
    fn custom_playlist_splits_artist_title_over_reuploader() {
        let track = parse_track(&json!({
            "id": "fxReup01",
            "title": "Death Grips - Fyrd Up",
            "uploader": "MKULTRA PSYOP",
            "channel": "MKULTRA PSYOP",
            "url": "https://music.youtube.com/watch?v=fxReup01"
        }))
        .unwrap();
        assert_eq!(track.title, "Fyrd Up");
        assert_eq!(track.artists, vec!["Death Grips".to_string()]);
        assert!(track.album.is_empty());
        assert!(track.needs_meta_enrichment());
    }

    #[test]
    fn dedicated_artists_keep_dashed_title() {
        let track = parse_track(&json!({
            "id": "fxDash01",
            "title": "Glass - Orchard",
            "artists": ["Северный маяк"],
            "album": "Карты без маршрута"
        }))
        .unwrap();
        assert_eq!(track.title, "Glass - Orchard");
        assert_eq!(track.artists, vec!["Северный маяк".to_string()]);
        assert_eq!(track.album, "Карты без маршрута");
    }

    #[test]
    fn flat_playlist_strips_topic_suffix_from_channel() {
        let track = parse_track(&json!({
            "id": "fxFlatTopic",
            "title": "Glass Orchard (Remix)",
            "uploader": "Северный маяк - Topic",
            "channel": "Северный маяк - Topic",
            "url": "https://music.youtube.com/watch?v=fxFlatTopic"
        }))
        .unwrap();
        assert_eq!(track.artists, vec!["Северный маяк".to_string()]);
        assert!(track.album.is_empty());
        assert!(track.needs_meta_enrichment());
    }

    #[test]
    fn keeps_both_artists_for_collab_extract() {
        let track = parse_track(&json!({
            "id": "fxCollab01",
            "title": "Glass Orchard (Remix)",
            "artist": "Северный маяк, Тихая бухта",
            "artists": ["Северный маяк", "Тихая бухта"],
            "creator": "Северный маяк, Тихая бухта",
            "uploader": "Северный маяк - Topic",
            "channel": "Северный маяк - Topic"
        }))
        .unwrap();
        assert_eq!(
            track.artists,
            vec!["Северный маяк".to_string(), "Тихая бухта".to_string()]
        );
    }

    #[test]
    fn formats_compact_durations() {
        assert_eq!(format_duration(65), "1:05");
        assert_eq!(format_duration(3661), "1:01:01");
    }
}
