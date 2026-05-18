use std::path::PathBuf;

use crate::config::{self, FolderProgress, VideoFile};

pub fn load_progress(folder: &PathBuf) -> Option<FolderProgress> {
    let path = config::folder_progress_path(folder);
    if path.exists() {
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
    } else {
        None
    }
}

pub fn save_progress(folder: &PathBuf, progress: &FolderProgress) {
    let path = config::folder_progress_path(folder);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(progress) {
        let _ = std::fs::write(&path, json);
    }
}

pub fn detect_progress_from_filenames(
    videos: &[VideoFile],
    identifier: &str,
) -> Option<(usize, usize)> {
    // Find the maximum processed index from filenames matching the identifier
    let mut max_index: Option<usize> = None;

    for video in videos {
        let name = &video.filename;
        if name.starts_with(&format!("[{}]", identifier)) {
            // Try to extract the numeric index
            if let Some(num_part) = name.split(']').nth(1) {
                let num_str: String = num_part
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect();
                if let Ok(idx) = num_str.parse::<usize>() {
                    max_index = Some(max_index.map_or(idx, |m| m.max(idx)));
                }
            }
        }
    }

    max_index.map(|idx| (idx, videos.len()))
}

pub fn init_progress(videos: &[VideoFile], identifier: &str) -> FolderProgress {
    let digit_count = config::compute_digit_count(videos.len());

    // Check filenames for existing progress
    let last_processed = detect_progress_from_filenames(videos, identifier)
        .map(|(idx, _)| idx)
        .unwrap_or(0);

    FolderProgress {
        identifier: identifier.into(),
        digit_count,
        last_processed,
        video_count: videos.len(),
    }
}
