use std::fs;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::thread;

pub struct AudioPlayer {
    play_rx: Option<Receiver<PathBuf>>,
    is_playing: bool,
    playing_flag: Option<Arc<AtomicBool>>,
}

impl AudioPlayer {
    pub fn new() -> Self {
        Self {
            play_rx: None,
            is_playing: false,
            playing_flag: None,
        }
    }

    pub fn play_clip(&mut self, video_path: &Path, seek_sec: f64) {
        self.stop();

        let video_path = video_path.to_path_buf();
        let output_path = crate::config::cache_dir().join("audio").join(format!(
            "audio_clip_{}_{}.wav",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        let clip_path = output_path.clone();
        let (tx, rx) = mpsc::channel();

        thread::spawn(move || {
            let start = (seek_sec - 2.5).max(0.0);
            if crate::ffmpeg::extract_audio_clip(&video_path, start, 5.0, &clip_path).is_err() {
                let _ = tx.send(PathBuf::new());
                return;
            }
            let _ = tx.send(clip_path);
        });

        self.play_rx = Some(rx);
        self.is_playing = true;
    }

    pub fn update(&mut self) {
        if let Some(rx) = &self.play_rx {
            if let Ok(path) = rx.try_recv() {
                self.play_rx = None;
                if path.as_os_str().is_empty() {
                    self.is_playing = false;
                    return;
                }

                let playing = Arc::new(AtomicBool::new(true));
                let flag = playing.clone();
                self.playing_flag = Some(playing);
                let clip = path;

                thread::spawn(move || {
                    if let Ok(file) = fs::File::open(&clip) {
                        let reader = BufReader::new(file);
                        if let Ok((_stream, handle)) = rodio::OutputStream::try_default() {
                            if let Ok(sink) = rodio::Sink::try_new(&handle) {
                                if let Ok(source) = rodio::Decoder::new(reader) {
                                    sink.append(source);
                                    sink.sleep_until_end();
                                }
                            }
                        }
                    }
                    let _ = fs::remove_file(&clip);
                    flag.store(false, Ordering::SeqCst);
                });
            }
        }

        if let Some(ref flag) = self.playing_flag {
            if !flag.load(Ordering::SeqCst) {
                self.is_playing = false;
                self.playing_flag = None;
            }
        }
    }

    pub fn stop(&mut self) {
        self.play_rx = None;
        self.is_playing = false;
        self.playing_flag = None;
    }

    pub fn is_playing(&self) -> bool {
        self.is_playing
    }
}
