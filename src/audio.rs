use std::fs;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::thread;

pub struct AudioPlayer {
    play_rx: Option<Receiver<PathBuf>>,
    is_playing: bool,
    current_sink: Option<rodio::Sink>,
    current_stream: Option<rodio::OutputStream>,
    current_clip: Option<PathBuf>,
}

impl AudioPlayer {
    pub fn new() -> Self {
        Self {
            play_rx: None,
            is_playing: false,
            current_sink: None,
            current_stream: None,
            current_clip: None,
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

                match Self::start_sink(&path) {
                    Ok((stream, sink)) => {
                        self.current_clip = Some(path);
                        self.current_stream = Some(stream);
                        self.current_sink = Some(sink);
                        self.is_playing = true;
                    }
                    Err(_) => {
                        let _ = fs::remove_file(&path);
                        self.is_playing = false;
                    }
                }
            }
        }

        if self.current_sink.as_ref().map(|sink| sink.empty()).unwrap_or(false) {
            self.cleanup_finished_clip();
            self.is_playing = false;
        }
    }

    fn start_sink(path: &Path) -> Result<(rodio::OutputStream, rodio::Sink), String> {
        let file = fs::File::open(path).map_err(|e| e.to_string())?;
        let reader = BufReader::new(file);
        let (stream, handle) = rodio::OutputStream::try_default().map_err(|e| e.to_string())?;
        let sink = rodio::Sink::try_new(&handle).map_err(|e| e.to_string())?;
        let source = rodio::Decoder::new(reader).map_err(|e| e.to_string())?;
        sink.append(source);
        Ok((stream, sink))
    }

    fn cleanup_finished_clip(&mut self) {
        self.current_sink.take();
        self.current_stream.take();
        if let Some(path) = self.current_clip.take() {
            let _ = fs::remove_file(path);
        }
    }

    pub fn stop(&mut self) {
        self.play_rx = None;
        if let Some(sink) = self.current_sink.take() {
            sink.stop();
        }
        self.current_stream.take();
        if let Some(path) = self.current_clip.take() {
            let _ = fs::remove_file(path);
        }
        self.is_playing = false;
    }

    pub fn is_playing(&self) -> bool {
        self.is_playing
    }
}
