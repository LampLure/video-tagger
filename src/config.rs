use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
        self.duration_secs.unwrap_or(600.0)
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
    pub last_folder: Option<PathBuf>,
    pub tag_library: Vec<TagDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagDef {
    pub name: String,
    pub use_count: u64,
    pub last_used: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            screenshot_interval: 10.0,
            shift_lock: false,
            last_folder: None,
            tag_library: Vec::new(),
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

pub fn cache_dir() -> PathBuf {
    app_data_dir().join("cache")
}

pub fn config_path() -> PathBuf {
    app_data_dir().join("video_tagger_config.json")
}

pub fn tag_library_path() -> PathBuf {
    app_data_dir().join("tag_library.json")
}

pub fn folder_progress_path(folder: &PathBuf) -> PathBuf {
    let hash = simple_hash(folder.to_string_lossy().as_ref());
    app_data_dir().join(format!("progress_{}.json", hash))
}

fn simple_hash(s: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    format!("{:016x}", h.finish())
}

pub fn generate_identifier() -> String {
    let seed = chrono::Utc::now()
        .timestamp_nanos_opt()
        .unwrap_or(0)
        .to_string();
    let hash = simple_hash(&seed);
    hash.chars().take(4).collect()
}

pub fn compute_digit_count(video_count: usize) -> usize {
    if video_count < 10 {
        return 1;
    }
    if video_count < 100 {
        return 2;
    }
    if video_count < 1000 {
        return 3;
    }
    if video_count < 10000 {
        return 4;
    }
    5
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
    let label_str: String = labels.iter().map(|l| format!("[{}]", l)).collect();

    if overwrite {
        format!("[{identifier}]{num}{star}{label_str}.{extension}")
    } else {
        format!("[{identifier}]{num}{star}{label_str}{original_basename}.{extension}")
    }
}
