use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{OnceLock, RwLock};

static FFMPEG_PATH: OnceLock<RwLock<PathBuf>> = OnceLock::new();

fn ffmpeg_lock() -> &'static RwLock<PathBuf> {
    FFMPEG_PATH.get_or_init(|| RwLock::new(PathBuf::from("ffmpeg")))
}

pub fn set_ffmpeg_path(path: PathBuf) {
    if let Ok(mut current) = ffmpeg_lock().write() {
        *current = path;
    }
}

fn ffmpeg_command() -> Command {
    let path = ffmpeg_lock()
        .read()
        .map(|p| p.clone())
        .unwrap_or_else(|_| PathBuf::from("ffmpeg"));
    Command::new(path)
}

fn is_executable_ffmpeg(path: &Path) -> bool {
    Command::new(path)
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn find_ffmpeg() -> Option<PathBuf> {
    let mut candidates = vec![
        PathBuf::from("ffmpeg"),
        PathBuf::from("ffmpeg.exe"),
        PathBuf::from(r"C:\ffmpeg\bin\ffmpeg.exe"),
        PathBuf::from(r"C:\ffmpeg\ffmpeg.exe"),
        PathBuf::from("/usr/bin/ffmpeg"),
        PathBuf::from("/usr/local/bin/ffmpeg"),
        PathBuf::from("/opt/homebrew/bin/ffmpeg"),
    ];

    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            candidates.push(dir.join(if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" }));
        }
    }

    for p in candidates {
        if is_executable_ffmpeg(&p) {
            set_ffmpeg_path(p.clone());
            return Some(p);
        }
    }

    None
}

pub fn get_video_duration(path: &Path) -> Option<f64> {
    let output = ffmpeg_command()
        .args(["-i", &path.to_string_lossy()])
        .output()
        .ok()?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    for line in stderr.lines() {
        if line.contains("Duration:") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if let Some(pos) = parts.iter().position(|&p| p == "Duration:") {
                if let Some(time_str) = parts.get(pos + 1) {
                    let t = time_str.trim_end_matches(',');
                    return parse_duration(t);
                }
            }
        }
    }
    None
}

fn parse_duration(s: &str) -> Option<f64> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() == 3 {
        let hours: f64 = parts[0].parse().ok()?;
        let minutes: f64 = parts[1].parse().ok()?;
        let seconds: f64 = parts[2].parse().ok()?;
        Some(hours * 3600.0 + minutes * 60.0 + seconds)
    } else {
        None
    }
}

pub fn extract_screenshots(
    video_path: &Path,
    start_sec: f64,
    interval: f64,
    count: usize,
    output_dir: &Path,
    prefix: &str,
) -> Result<Vec<PathBuf>, String> {
    std::fs::create_dir_all(output_dir).map_err(|e| e.to_string())?;

    let mut paths = Vec::new();
    for i in 0..count {
        let time_sec = start_sec + (i as f64) * interval;
        let output_path = output_dir.join(format!("{}_{:04}.png", prefix, i));

        if output_path.exists() {
            paths.push(output_path);
            continue;
        }

        let status = ffmpeg_command()
            .args([
                "-y",
                "-ss",
                &format!("{:.3}", time_sec),
                "-i",
                &video_path.to_string_lossy(),
                "-vframes",
                "1",
                "-q:v",
                "3",
                "-vf",
                "scale=320:180:force_original_aspect_ratio=decrease,pad=320:180:(ow-iw)/2:(oh-ih)/2",
                &output_path.to_string_lossy(),
            ])
            .status()
            .map_err(|e| format!("ffmpeg error: {}", e))?;

        if status.success() {
            paths.push(output_path);
        }
    }
    Ok(paths)
}

pub fn extract_thumbnail(video_path: &Path, output_path: &Path) -> Result<(), String> {
    if output_path.exists() {
        return Ok(());
    }

    let duration = get_video_duration(video_path).unwrap_or(60.0).max(1.0);
    let seek_time = (duration * 0.3).clamp(0.1, duration.max(0.1) - 0.1);

    let status = ffmpeg_command()
        .args([
            "-y",
            "-ss",
            &format!("{:.3}", seek_time),
            "-i",
            &video_path.to_string_lossy(),
            "-vframes",
            "1",
            "-q:v",
            "4",
            "-vf",
            "scale=320:180:force_original_aspect_ratio=decrease,pad=320:180:(ow-iw)/2:(oh-ih)/2",
            &output_path.to_string_lossy(),
        ])
        .status()
        .map_err(|e| format!("ffmpeg error: {}", e))?;

    if status.success() {
        Ok(())
    } else {
        Err("ffmpeg thumbnail extraction failed".into())
    }
}

pub fn extract_audio_clip(
    video_path: &Path,
    start_sec: f64,
    duration_secs: f64,
    output_path: &Path,
) -> Result<(), String> {
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let status = ffmpeg_command()
        .args([
            "-y",
            "-ss",
            &format!("{:.3}", start_sec.max(0.0)),
            "-i",
            &video_path.to_string_lossy(),
            "-t",
            &format!("{:.3}", duration_secs),
            "-ac",
            "1",
            "-ar",
            "22050",
            "-acodec",
            "pcm_s16le",
            &output_path.to_string_lossy(),
        ])
        .status()
        .map_err(|e| format!("ffmpeg audio error: {}", e))?;

    if status.success() {
        Ok(())
    } else {
        Err("ffmpeg audio extraction failed".into())
    }
}
