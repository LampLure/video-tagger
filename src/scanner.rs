use std::path::PathBuf;
use walkdir::WalkDir;

use crate::config::VideoFile;

const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "mkv", "avi", "mov", "flv", "webm", "ts", "m4v", "wmv", "mpg", "mpeg", "3gp", "ogv",
];

pub fn scan_videos(folder: &PathBuf) -> Vec<VideoFile> {
    let mut videos = Vec::new();

    let video_exts: Vec<&str> = VIDEO_EXTENSIONS.to_vec();
    for entry in WalkDir::new(folder)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }

        let path = entry.path().to_path_buf();
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        if video_exts.contains(&ext.as_str()) {
            let filename = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);

            videos.push(VideoFile {
                path,
                filename,
                extension: ext,
                size,
                duration_secs: None,
            });
        }
    }

    // Sort by filename
    videos.sort_by(|a, b| natord::compare_ignore_case(&a.filename, &b.filename));

    videos
}

pub fn resolve_name_conflict(output_path: &mut PathBuf) {
    if !output_path.exists() {
        return;
    }

    let parent = output_path.parent().unwrap_or(std::path::Path::new(".")).to_path_buf();
    let stem = output_path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let ext = output_path
        .extension()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();

    let mut counter = 1;
    loop {
        let new_name = format!("{}[{}].{}", stem, counter, ext);
        *output_path = parent.join(&new_name);
        if !output_path.exists() {
            break;
        }
        counter += 1;
    }
}

// Simple but adequate natural sort comparison
mod natord {
    pub fn compare_ignore_case(a: &str, b: &str) -> std::cmp::Ordering {
        let a_lower = a.to_lowercase();
        let b_lower = b.to_lowercase();
        compare(&a_lower, &b_lower)
    }

    fn compare(a: &str, b: &str) -> std::cmp::Ordering {
        let a_chars: Vec<char> = a.chars().collect();
        let b_chars: Vec<char> = b.chars().collect();
        let mut ai = 0;
        let mut bi = 0;

        while ai < a_chars.len() && bi < b_chars.len() {
            let ac = a_chars[ai];
            let bc = b_chars[bi];

            if ac.is_ascii_digit() && bc.is_ascii_digit() {
                let mut a_num = String::new();
                let mut b_num = String::new();

                while ai < a_chars.len() && a_chars[ai].is_ascii_digit() {
                    a_num.push(a_chars[ai]);
                    ai += 1;
                }
                while bi < b_chars.len() && b_chars[bi].is_ascii_digit() {
                    b_num.push(b_chars[bi]);
                    bi += 1;
                }

                let a_val: u64 = a_num.parse().unwrap_or(0);
                let b_val: u64 = b_num.parse().unwrap_or(0);
                match a_val.cmp(&b_val) {
                    std::cmp::Ordering::Equal => continue,
                    other => return other,
                }
            } else {
                match ac.cmp(&bc) {
                    std::cmp::Ordering::Equal => {
                        ai += 1;
                        bi += 1;
                    }
                    other => return other,
                }
            }
        }

        a_chars.len().cmp(&b_chars.len())
    }
}
