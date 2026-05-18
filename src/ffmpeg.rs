use std::path::{Path, PathBuf};
use std::process::Command;

pub fn find_ffmpeg() -> Option<PathBuf> {
    // Check common locations
    let common_paths = [
        "ffmpeg.exe",
        "ffmpeg",
        r"C:\ffmpeg\bin\ffmpeg.exe",
        r"C:\ffmpeg\ffmpeg.exe",
    ];

    for p in &common_paths {
        if Path::new(p).exists() {
            return Some(PathBuf::from(p));
        }
    }

    // Check PATH
    if let Ok(output) = Command::new("where").arg("ffmpeg").output() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Some(line) = stdout.lines().next() {
            let path = PathBuf::from(line.trim());
            if path.exists() {
                return Some(path);
            }
        }
    }

    None
}

pub fn get_video_duration(path: &Path) -> Option<f64> {
    let output = Command::new("ffmpeg")
        .args(["-i", &path.to_string_lossy()])
        .output()
        .ok()?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    for line in stderr.lines() {
        if line.contains("Duration:") {
            // Duration: 00:01:30.50
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
    // "01:30:50.50" or "00:01:30.50"
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

        let status = Command::new("ffmpeg")
            .args([
                "-y",
                "-ss",
                &format!("{}", time_sec),
                "-i",
                &video_path.to_string_lossy(),
                "-vframes",
                "1",
                "-q:v",
                "3",
                "-s",
                "320x180",
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

    // Try to grab frame at 10% of video
    let duration = get_video_duration(video_path).unwrap_or(60.0);
    let seek_time = (duration * 0.1).min(30.0);

    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-ss",
            &format!("{}", seek_time),
            "-i",
            &video_path.to_string_lossy(),
            "-vframes",
            "1",
            "-q:v",
            "4",
            "-s",
            "320x180",
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
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-ss",
            &format!("{:.3}", start_sec),
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

pub fn has_audio_track(video_path: &Path) -> bool {
    if let Ok(output) = Command::new("ffmpeg")
        .args(["-i", &video_path.to_string_lossy()])
        .output()
    {
        let stderr = String::from_utf8_lossy(&output.stderr);
        stderr.contains("Audio:")
    } else {
        false
    }
}
