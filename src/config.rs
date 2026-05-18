use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

fn default_ui_scale() -> f32 { 1.0 }
fn default_ai_image_max_pixels() -> u32 { 48_000 }
fn default_ai_jpeg_quality() -> u8 { 80 }
fn default_ai_audio_sample_rate() -> u32 { 16_000 }
fn default_ai_audio_clip_seconds() -> f64 { 5.0 }
fn default_ai_audio_clips_per_batch() -> usize { 5 }
fn default_ai_max_extra_sample_batches() -> usize { 3 }
fn default_ai_stream_idle_timeout_seconds() -> u64 { 20 }
fn default_ai_auto_accept() -> bool { true }
fn default_ai_text_settings() -> String { crate::ai::default_ai_text_settings() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoFile {
    pub path: PathBuf,
    pub filename: String,
    pub extension: String,
    pub size: u64,
    pub duration_secs: Option<f64>,
}

impl VideoFile {
    pub fn ensure_duration(&mut self) -> f64 {
        if self.duration_secs.is_none() {
            self.duration_secs = crate::ffmpeg::get_video_duration(&self.path);
        }
        self.duration_secs.unwrap_or(0.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderProgress {
    pub identifier: String,
    pub digit_count: usize,
    pub last_processed: usize,
    pub video_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub screenshot_interval: f64,
    pub shift_lock: bool,
    #[serde(default)]
    pub tag_position_lock: bool,
    #[serde(default = "default_ui_scale")]
    pub ui_scale: f32,
    pub last_folder: Option<PathBuf>,
    pub tag_library: Vec<TagDef>,
    #[serde(default = "default_ai_auto_accept")]
    pub ai_auto_accept: bool,
    #[serde(default = "default_ai_image_max_pixels")]
    pub ai_image_max_pixels: u32,
    #[serde(default = "default_ai_jpeg_quality")]
    pub ai_jpeg_quality: u8,
    #[serde(default = "default_ai_audio_sample_rate")]
    pub ai_audio_sample_rate: u32,
    #[serde(default = "default_ai_audio_clip_seconds")]
    pub ai_audio_clip_seconds: f64,
    #[serde(default = "default_ai_audio_clips_per_batch")]
    pub ai_audio_clips_per_batch: usize,
    #[serde(default = "default_ai_max_extra_sample_batches")]
    pub ai_max_extra_sample_batches: usize,
    #[serde(default = "default_ai_stream_idle_timeout_seconds")]
    pub ai_stream_idle_timeout_seconds: u64,
    #[serde(default = "default_ai_text_settings")]
    pub ai_text_settings: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagDef {
    pub name: String,
    pub use_count: u64,
    pub last_used: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedVideoName {
    pub identifier: String,
    pub index: usize,
    pub starred: bool,
    pub labels: Vec<String>,
    pub score: Option<u8>,
    pub original_stem: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            screenshot_interval: 10.0,
            shift_lock: false,
            tag_position_lock: false,
            ui_scale: 1.0,
            last_folder: None,
            tag_library: Vec::new(),
            ai_auto_accept: default_ai_auto_accept(),
            ai_image_max_pixels: default_ai_image_max_pixels(),
            ai_jpeg_quality: default_ai_jpeg_quality(),
            ai_audio_sample_rate: default_ai_audio_sample_rate(),
            ai_audio_clip_seconds: default_ai_audio_clip_seconds(),
            ai_audio_clips_per_batch: default_ai_audio_clips_per_batch(),
            ai_max_extra_sample_batches: default_ai_max_extra_sample_batches(),
            ai_stream_idle_timeout_seconds: default_ai_stream_idle_timeout_seconds(),
            ai_text_settings: default_ai_text_settings(),
        }
    }
}

impl AppConfig {
    pub fn load() -> Self {
        let path = config_path();
        let mut config = if path.exists() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            Self::default()
        };
        config.ui_scale = config.ui_scale.clamp(0.5, 3.0);
        config.ai_image_max_pixels = config.ai_image_max_pixels.clamp(8_000, 500_000);
        config.ai_jpeg_quality = config.ai_jpeg_quality.clamp(20, 95);
        if ![8000, 16000, 22050, 44100].contains(&config.ai_audio_sample_rate) {
            config.ai_audio_sample_rate = default_ai_audio_sample_rate();
        }
        config.ai_audio_clip_seconds = config.ai_audio_clip_seconds.clamp(1.0, 10.0);
        config.ai_audio_clips_per_batch = config.ai_audio_clips_per_batch.min(10);
        config.ai_max_extra_sample_batches = config.ai_max_extra_sample_batches.min(5);
        config.ai_stream_idle_timeout_seconds = config.ai_stream_idle_timeout_seconds.clamp(5, 600);
        if config.ai_text_settings.trim().is_empty() {
            config.ai_text_settings = default_ai_text_settings();
        }
        config
    }

    pub fn save(&self) {
        let path = config_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }
}

pub fn sanitize_filename(s: &str) -> String {
    let invalid: &[char] = &['\\', '/', ':', '*', '?', '"', '<', '>', '|'];
    s.chars().filter(|c| !invalid.contains(c)).collect()
}

pub fn app_data_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn cache_dir() -> PathBuf { app_data_dir().join("cache") }
pub fn config_path() -> PathBuf { app_data_dir().join("video_tagger_config.json") }
pub fn tag_library_path() -> PathBuf { app_data_dir().join("tag_library.json") }

pub fn folder_progress_path(folder: &Path) -> PathBuf {
    let folder_name = folder
        .file_name()
        .and_then(|s| s.to_str())
        .map(sanitize_filename)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "root".to_string());
    folder.join(format!("{}_video_tagger_progress.json", folder_name))
}

pub fn simple_hash(s: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    format!("{:016x}", h.finish())
}

pub fn generate_identifier_for_folder(folder: &Path) -> String {
    let seed = format!("{}:{}", folder.to_string_lossy(), chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0));
    simple_hash(&seed).chars().take(4).collect()
}

pub fn generate_identifier() -> String {
    let seed = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0).to_string();
    simple_hash(&seed).chars().take(4).collect()
}

pub fn compute_digit_count(video_count: usize) -> usize { video_count.max(1).to_string().len() }

pub fn parse_video_name(stem: &str) -> Option<ParsedVideoName> {
    let mut rest = stem;
    if !rest.starts_with('[') { return None; }

    let id_end = rest.find(']')?;
    let identifier = rest.get(1..id_end)?.to_string();
    if identifier.len() != 4 || !identifier.chars().all(|c| c.is_ascii_hexdigit()) { return None; }
    rest = rest.get(id_end + 1..)?;

    let num_str;
    if rest.starts_with('[') {
        let num_end = rest.find(']')?;
        num_str = rest.get(1..num_end)?.to_string();
        rest = rest.get(num_end + 1..)?;
    } else {
        let digit_len = rest.chars().take_while(|c| c.is_ascii_digit()).count();
        if digit_len == 0 { return None; }
        num_str = rest.get(..digit_len)?.to_string();
        rest = rest.get(digit_len..)?;
    }

    if num_str.is_empty() || !num_str.chars().all(|c| c.is_ascii_digit()) { return None; }
    let index = num_str.parse::<usize>().ok()?;

    let mut starred = false;
    let mut labels = Vec::new();
    while rest.starts_with('[') {
        let end = match rest.find(']') { Some(end) => end, None => break };
        let token = rest.get(1..end).unwrap_or_default().to_string();
        rest = rest.get(end + 1..).unwrap_or_default();
        if token == "★" { starred = true; }
        else if !token.is_empty() { labels.push(token); }
    }

    let mut score = None;
    if rest.starts_with("{point") {
        if let Some(end) = rest.find('}') {
            let token = rest.get(6..end).unwrap_or_default();
            if token.len() == 3 && token.chars().all(|c| c.is_ascii_digit()) {
                score = token.parse::<u8>().ok().map(|s| s.min(100));
                rest = rest.get(end + 1..).unwrap_or_default();
            }
        }
    }

    Some(ParsedVideoName { identifier, index, starred, labels, score, original_stem: rest.to_string() })
}

pub fn format_video_name(
    identifier: &str,
    index: usize,
    digit_count: usize,
    labels: &[String],
    starred: bool,
    original_basename: &str,
    extension: &str,
    overwrite: bool,
) -> String {
    let num = format!("{:0width$}", index + 1, width = digit_count);
    let star = if starred { "[★]" } else { "" };
    let mut score: Option<u8> = None;
    let label_str: String = labels
        .iter()
        .filter_map(|label| {
            let clean = sanitize_filename(label);
            if clean.len() == 8 && clean.starts_with("point") && clean[5..].chars().all(|c| c.is_ascii_digit()) {
                score = clean[5..].parse::<u8>().ok().map(|s| s.min(100));
                None
            } else if clean.is_empty() {
                None
            } else {
                Some(format!("[{}]", clean))
            }
        })
        .collect();
    let score_str = score.map(|s| format!("{{point{:03}}}", s.min(100))).unwrap_or_default();
    let ext = extension.trim_start_matches('.');

    if overwrite {
        format!("[{identifier}][{num}]{star}{label_str}{score_str}.{ext}")
    } else {
        let original = parse_video_name(original_basename)
            .map(|p| p.original_stem)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| original_basename.to_string());
        format!("[{identifier}][{num}]{star}{label_str}{score_str}{original}.{ext}")
    }
}
