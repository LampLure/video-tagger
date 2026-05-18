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

pub fn detect_identifier_from_filenames(videos: &[VideoFile]) -> Option<String> {
    let mut counts = std::collections::HashMap::<String, usize>::new();
    for video in videos {
        if let Some(parsed) = config::parse_video_name(&video.filename) {
            *counts.entry(parsed.identifier).or_insert(0) += 1;
        }
    }
    counts
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)))
        .map(|(identifier, _)| identifier)
}

pub fn detect_progress_from_filenames(
    videos: &[VideoFile],
    identifier: &str,
) -> Option<(usize, usize)> {
    let mut max_index: Option<usize> = None;

    for video in videos {
        if let Some(parsed) = config::parse_video_name(&video.filename) {
            if parsed.identifier == identifier {
                max_index = Some(max_index.map_or(parsed.index, |m| m.max(parsed.index)));
            }
        }
    }

    max_index.map(|idx| (idx, videos.len()))
}

pub fn init_progress_for_folder(folder: &PathBuf, videos: &[VideoFile]) -> FolderProgress {
    let digit_count = config::compute_digit_count(videos.len());

    if let Some(mut progress) = load_progress(folder) {
        progress.digit_count = progress.digit_count.max(digit_count);
        progress.video_count = videos.len();
        if let Some((last_processed, _)) = detect_progress_from_filenames(videos, &progress.identifier) {
            progress.last_processed = progress.last_processed.max(last_processed);
        }
        return progress;
    }

    let identifier = detect_identifier_from_filenames(videos)
        .unwrap_or_else(|| config::generate_identifier_for_folder(folder));
    init_progress(videos, &identifier)
}

pub fn init_progress(videos: &[VideoFile], identifier: &str) -> FolderProgress {
    let digit_count = config::compute_digit_count(videos.len());
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
