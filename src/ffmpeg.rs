use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{OnceLock, RwLock};
use std::time::{Duration, Instant};

static FFMPEG_PATH: OnceLock<RwLock<PathBuf>> = OnceLock::new();

fn ffmpeg_lock() -> &'static RwLock<PathBuf> {
    FFMPEG_PATH.get_or_init(|| RwLock::new(PathBuf::from("ffmpeg")))
}

pub fn set_ffmpeg_path(path: PathBuf) {
    if let Ok(mut current) = ffmpeg_lock().write() { *current = path; }
}

fn ffmpeg_command() -> Command {
    let path = ffmpeg_lock().read().map(|p| p.clone()).unwrap_or_else(|_| PathBuf::from("ffmpeg"));
    Command::new(path)
}

fn run_with_timeout(mut cmd: Command, timeout: Duration) -> Result<bool, String> {
    let mut child = cmd.spawn().map_err(|e| format!("启动 ffmpeg 失败: {}", e))?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status.success()),
            Ok(None) => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!("ffmpeg 超时 {} 秒", timeout.as_secs()));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(format!("等待 ffmpeg 失败: {}", e)),
        }
    }
}

fn run_with_timeout_capture(mut cmd: Command, timeout: Duration) -> Result<(bool, String), String> {
    cmd.stdout(Stdio::null()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("启动 ffmpeg 失败: {}", e))?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut stderr = String::new();
                if let Some(mut pipe) = child.stderr.take() {
                    let _ = pipe.read_to_string(&mut stderr);
                }
                return Ok((status.success(), stderr.trim().to_string()));
            }
            Ok(None) => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    let mut stderr = String::new();
                    if let Some(mut pipe) = child.stderr.take() {
                        let _ = pipe.read_to_string(&mut stderr);
                    }
                    return Err(format!("ffmpeg 超时 {} 秒{}", timeout.as_secs(), if stderr.trim().is_empty() { String::new() } else { format!(": {}", stderr.trim()) }));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(format!("等待 ffmpeg 失败: {}", e)),
        }
    }
}

fn is_executable_ffmpeg(path: &Path) -> bool {
    Command::new(path).arg("-version").output().map(|o| o.status.success()).unwrap_or(false)
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
    let output = ffmpeg_command().args(["-i", &path.to_string_lossy()]).output().ok()?;
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
    } else { None }
}

pub fn extract_screenshots(video_path: &Path, start_sec: f64, interval: f64, count: usize, output_dir: &Path, prefix: &str) -> Result<Vec<PathBuf>, String> {
    std::fs::create_dir_all(output_dir).map_err(|e| e.to_string())?;
    let mut paths = Vec::new();
    for i in 0..count {
        let time_sec = start_sec + (i as f64) * interval;
        let output_path = output_dir.join(format!("{}_{:04}.png", prefix, i));
        if usable_image_file(&output_path) { paths.push(output_path); continue; }
        let mut cmd = ffmpeg_command();
        cmd.args([
            "-hide_banner", "-loglevel", "error", "-y", "-ss", &format!("{:.3}", time_sec),
            "-i", &video_path.to_string_lossy(), "-vframes", "1", "-q:v", "3", "-vf",
            "scale=320:180:force_original_aspect_ratio=decrease,pad=320:180:(ow-iw)/2:(oh-ih)/2",
            &output_path.to_string_lossy(),
        ]);
        let status = run_with_timeout(cmd, Duration::from_secs(8))?;
        if status && usable_image_file(&output_path) { paths.push(output_path); }
    }
    Ok(paths)
}

fn usable_image_file(path: &Path) -> bool {
    std::fs::metadata(path).map(|m| m.is_file() && m.len() > 1024).unwrap_or(false)
}

fn run_thumbnail_seek(video_path: &Path, seek_time: f64, output_path: &Path, accurate: bool) -> Result<(), String> {
    let mut cmd = ffmpeg_command();
    cmd.arg("-hide_banner").arg("-loglevel").arg("error").arg("-y");
    if !accurate { cmd.arg("-ss").arg(format!("{:.3}", seek_time)); }
    cmd.arg("-i").arg(video_path);
    if accurate { cmd.arg("-ss").arg(format!("{:.3}", seek_time)); }
    cmd.args([
        "-an", "-frames:v", "1", "-q:v", "3", "-vf",
        "scale=320:180:force_original_aspect_ratio=decrease,pad=320:180:(ow-iw)/2:(oh-ih)/2",
    ]);
    cmd.arg(output_path);
    let success = run_with_timeout(cmd, Duration::from_secs(6))?;
    if success && usable_image_file(output_path) { Ok(()) } else { Err(format!("抽帧失败 seek={:.1}s", seek_time)) }
}

pub fn extract_thumbnail(video_path: &Path, output_path: &Path) -> Result<(), String> {
    if usable_image_file(output_path) { return Ok(()); }
    if output_path.exists() { let _ = std::fs::remove_file(output_path); }
    if let Some(parent) = output_path.parent() { std::fs::create_dir_all(parent).map_err(|e| e.to_string())?; }
    let duration = get_video_duration(video_path).unwrap_or(60.0).max(1.0);
    let mut candidates = Vec::new();
    for ratio in [0.10, 0.25, 0.40, 0.55, 0.70] {
        candidates.push((duration * ratio).clamp(0.1, (duration - 0.1).max(0.1)));
    }
    candidates.push(1.0_f64.min((duration - 0.1).max(0.1)));
    let mut last_err = String::new();
    for seek in candidates {
        match run_thumbnail_seek(video_path, seek, output_path, false) { Ok(()) => return Ok(()), Err(e) => last_err = e }
        let _ = std::fs::remove_file(output_path);
        match run_thumbnail_seek(video_path, seek, output_path, true) { Ok(()) => return Ok(()), Err(e) => last_err = e }
        let _ = std::fs::remove_file(output_path);
    }
    Err(if last_err.is_empty() { "ffmpeg thumbnail extraction failed".into() } else { last_err })
}

pub fn extract_audio_clip(video_path: &Path, start_sec: f64, duration_secs: f64, output_path: &Path) -> Result<(), String> {
    extract_audio_clip_internal(video_path, start_sec, duration_secs, 22050, output_path)
}

pub fn extract_ai_audio_clip(video_path: &Path, start_sec: f64, duration_secs: f64, sample_rate: u32, output_path: &Path) -> Result<(), String> {
    extract_audio_clip_internal(video_path, start_sec, duration_secs, sample_rate, output_path)
}

fn extract_audio_clip_internal(video_path: &Path, start_sec: f64, duration_secs: f64, sample_rate: u32, output_path: &Path) -> Result<(), String> {
    if let Some(parent) = output_path.parent() { std::fs::create_dir_all(parent).map_err(|e| e.to_string())?; }
    let sr = sample_rate.to_string();
    let mut cmd = ffmpeg_command();
    cmd.args([
        "-hide_banner", "-loglevel", "error", "-y", "-ss", &format!("{:.3}", start_sec.max(0.0)),
        "-i", &video_path.to_string_lossy(), "-t", &format!("{:.3}", duration_secs), "-ac", "1", "-ar",
        &sr, "-acodec", "pcm_s16le", &output_path.to_string_lossy(),
    ]);
    let (status, stderr) = run_with_timeout_capture(cmd, Duration::from_secs(8))?;
    if status { Ok(()) } else { Err(if stderr.is_empty() { "ffmpeg audio extraction failed".into() } else { format!("ffmpeg audio extraction failed: {}", stderr) }) }
}

fn extract_ai_raw_frame(video_path: &Path, time_sec: f64, output_path: &Path, accurate: bool) -> Result<(), String> {
    if output_path.exists() { let _ = std::fs::remove_file(output_path); }
    let mut cmd = ffmpeg_command();
    cmd.arg("-hide_banner").arg("-loglevel").arg("error").arg("-y");
    if !accurate { cmd.arg("-ss").arg(format!("{:.3}", time_sec.max(0.0))); }
    cmd.arg("-i").arg(video_path);
    if accurate { cmd.arg("-ss").arg(format!("{:.3}", time_sec.max(0.0))); }
    cmd.arg("-an").arg("-frames:v").arg("1").arg("-q:v").arg("2").arg(output_path);
    let (status, stderr) = run_with_timeout_capture(cmd, Duration::from_secs(10))?;
    if status && usable_image_file(output_path) {
        Ok(())
    } else {
        Err(if stderr.is_empty() { format!("ffmpeg AI raw frame extraction failed at {:.3}s", time_sec) } else { format!("ffmpeg AI raw frame extraction failed at {:.3}s: {}", time_sec, stderr) })
    }
}

fn resize_and_encode_ai_jpeg(raw_path: &Path, max_pixels: u32, jpeg_quality: u8, output_path: &Path) -> Result<(), String> {
    let mut img = image::open(raw_path).map_err(|e| format!("AI 抽帧图片解码失败: {}", e))?;
    let max_pixels = max_pixels.clamp(8_000, 500_000) as u64;
    let pixels = img.width() as u64 * img.height() as u64;
    if pixels > max_pixels {
        let scale = (max_pixels as f64 / pixels as f64).sqrt();
        let new_w = ((img.width() as f64 * scale).round() as u32).max(2);
        let new_h = ((img.height() as f64 * scale).round() as u32).max(2);
        img = img.resize(new_w, new_h, image::imageops::FilterType::Triangle);
    }
    let rgb = img.to_rgb8();
    if output_path.exists() { let _ = std::fs::remove_file(output_path); }
    let file = std::fs::File::create(output_path).map_err(|e| format!("创建 AI JPEG 失败: {}", e))?;
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(file, jpeg_quality.clamp(20, 95));
    encoder.encode_image(&rgb).map_err(|e| format!("编码 AI JPEG 失败: {}", e))?;
    if usable_image_file(output_path) { Ok(()) } else { Err("AI JPEG 输出文件无效或过小".into()) }
}

pub fn extract_ai_image(video_path: &Path, time_sec: f64, max_pixels: u32, jpeg_quality: u8, output_path: &Path) -> Result<(), String> {
    if let Some(parent) = output_path.parent() { std::fs::create_dir_all(parent).map_err(|e| e.to_string())?; }
    if output_path.exists() { let _ = std::fs::remove_file(output_path); }
    let raw_path = output_path.with_extension("raw_ai_frame.jpg");
    let duration = get_video_duration(video_path).unwrap_or_else(|| time_sec.max(1.0) + 1.0).max(0.2);
    let base = time_sec.clamp(0.0, (duration - 0.35).max(0.0));
    let candidates = [
        base,
        (base - 0.25).max(0.0),
        (base + 0.25).min((duration - 0.35).max(0.0)),
        (duration * 0.5).clamp(0.0, (duration - 0.35).max(0.0)),
        0.1_f64.min((duration - 0.35).max(0.0)),
    ];
    let mut last_err = String::new();
    for seek in candidates {
        for accurate in [false, true] {
            match extract_ai_raw_frame(video_path, seek, &raw_path, accurate)
                .and_then(|_| resize_and_encode_ai_jpeg(&raw_path, max_pixels, jpeg_quality, output_path)) {
                Ok(()) => {
                    let _ = std::fs::remove_file(&raw_path);
                    return Ok(());
                }
                Err(err) => {
                    last_err = err;
                    let _ = std::fs::remove_file(output_path);
                    let _ = std::fs::remove_file(&raw_path);
                }
            }
        }
    }
    Err(if last_err.is_empty() { "ffmpeg AI image extraction failed".into() } else { format!("ffmpeg AI image extraction failed: {}", last_err) })
}