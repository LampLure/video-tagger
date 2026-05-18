use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::mpsc;

use eframe::egui;
use egui::{Color32, RichText, StrokeKind, Vec2};

use crate::audio::AudioPlayer;
use crate::cache::ScreenshotCache;
use crate::config::{self, AppConfig, VideoFile};
use crate::ffmpeg;
use crate::progress;
use crate::scanner;
use crate::tags::TagLibrary;

mod behavior;
mod ui;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    Fresh,
    Overview,
    Sorting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortMode {
    Name,
    Date,
    Size,
}

pub enum ThumbnailResult {
    Loaded { index: usize, size: [usize; 2], rgba: Vec<u8> },
    Failed { index: usize, reason: String },
}

pub enum ScreenshotResult {
    Loaded { request_id: u64, key: String, paths: Vec<PathBuf> },
    Prefetched { key: String, paths: Vec<PathBuf> },
    Failed { request_id: u64, reason: String },
}

pub struct VideoTaggerApp {
    pub app_mode: AppMode,
    config: AppConfig,
    tag_library: TagLibrary,

    selected_folder: Option<PathBuf>,
    videos: Vec<VideoFile>,
    folder_progress: Option<crate::config::FolderProgress>,
    ffmpeg_path: Option<PathBuf>,
    ffmpeg_error: bool,

    overview_search: String,
    overview_sort: SortMode,
    overview_thumbnails: HashMap<usize, egui::TextureHandle>,
    thumbnail_queue: VecDeque<usize>,
    thumbnail_loaded: HashSet<usize>,
    thumbnail_errors: HashMap<usize, String>,
    thumbnail_inflight: HashSet<usize>,
    thumbnail_rx: Option<mpsc::Receiver<ThumbnailResult>>,
    thumbnail_tx: mpsc::Sender<ThumbnailResult>,

    current_video_index: usize,
    screenshot_interval: f64,
    screenshot_start_sec: f64,
    screenshot_paths: Vec<PathBuf>,
    screenshot_textures: HashMap<String, egui::TextureHandle>,
    screenshot_error: Option<String>,
    screenshot_loading: bool,
    screenshot_request_id: u64,
    screenshot_cached_ranges: HashMap<String, Vec<PathBuf>>,
    screenshot_prefetching: HashSet<String>,
    screenshot_rx: Option<mpsc::Receiver<ScreenshotResult>>,
    screenshot_tx: mpsc::Sender<ScreenshotResult>,

    current_labels: Vec<String>,
    undone_labels: Vec<String>,
    is_star_phase: bool,
    is_starred: bool,
    pending_overwrite_once: bool,

    tag_row: usize,
    tag_col: usize,
    editing_new_tag: bool,
    new_tag_text: String,

    audio_player: AudioPlayer,
    playing_screenshot: Option<usize>,
    screenshot_cache: ScreenshotCache,

    show_completion: bool,
    show_star_hint: bool,
    independent_edit: Option<usize>,

    ffmpeg_custom_path: String,
    ffmpeg_dialog_open: bool,
}

impl Default for VideoTaggerApp {
    fn default() -> Self {
        let (thumbnail_tx, thumbnail_rx) = mpsc::channel();
        let (screenshot_tx, screenshot_rx) = mpsc::channel();
        Self {
            app_mode: AppMode::Fresh,
            config: AppConfig::load(),
            tag_library: TagLibrary::load(),
            selected_folder: None,
            videos: Vec::new(),
            folder_progress: None,
            ffmpeg_path: None,
            ffmpeg_error: false,

            overview_search: String::new(),
            overview_sort: SortMode::Name,
            overview_thumbnails: HashMap::new(),
            thumbnail_queue: VecDeque::new(),
            thumbnail_loaded: HashSet::new(),
            thumbnail_errors: HashMap::new(),
            thumbnail_inflight: HashSet::new(),
            thumbnail_rx: Some(thumbnail_rx),
            thumbnail_tx,

            current_video_index: 0,
            screenshot_interval: 10.0,
            screenshot_start_sec: 0.0,
            screenshot_paths: Vec::new(),
            screenshot_textures: HashMap::new(),
            screenshot_error: None,
            screenshot_loading: false,
            screenshot_request_id: 0,
            screenshot_cached_ranges: HashMap::new(),
            screenshot_prefetching: HashSet::new(),
            screenshot_rx: Some(screenshot_rx),
            screenshot_tx,

            current_labels: Vec::new(),
            undone_labels: Vec::new(),
            is_star_phase: false,
            is_starred: false,
            pending_overwrite_once: false,

            tag_row: 0,
            tag_col: 0,
            editing_new_tag: false,
            new_tag_text: String::new(),

            audio_player: AudioPlayer::new(),
            playing_screenshot: None,
            screenshot_cache: ScreenshotCache::new(500),

            show_completion: false,
            show_star_hint: false,
            independent_edit: None,

            ffmpeg_custom_path: String::new(),
            ffmpeg_dialog_open: false,
        }
    }
}

impl eframe::App for VideoTaggerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.audio_player.update();
        if self.playing_screenshot.is_some() && !self.audio_player.is_playing() {
            self.playing_screenshot = None;
        }

        if self.ffmpeg_path.is_none() && !self.ffmpeg_error {
            self.ffmpeg_path = ffmpeg::find_ffmpeg();
            if self.ffmpeg_path.is_none() {
                self.ffmpeg_error = true;
                self.ffmpeg_dialog_open = true;
            }
        }

        self.poll_thumbnail_results(ctx);
        self.poll_screenshot_results(ctx);

        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| self.render_top_bar(ui));

        egui::SidePanel::left("sidebar")
            .resizable(false)
            .min_width(190.0)
            .max_width(220.0)
            .show(ctx, |ui| self.render_sidebar(ui));

        let central_frame = if self.app_mode == AppMode::Sorting {
            egui::Frame::none().fill(Color32::from_gray(24))
        } else {
            egui::Frame::central_panel(&ctx.style())
        };

        egui::CentralPanel::default().frame(central_frame).show(ctx, |ui| match self.app_mode {
            AppMode::Fresh => self.render_welcome(ui),
            AppMode::Overview => self.render_overview(ui),
            AppMode::Sorting => self.render_sorting(ui, ctx),
        });

        if self.app_mode == AppMode::Sorting && !self.videos.is_empty() {
            egui::TopBottomPanel::bottom("progress_bar").show(ctx, |ui| {
                let total = self.videos.len();
                let done = self.processed_count();
                let frac = done as f32 / total as f32;
                ui.add(
                    egui::ProgressBar::new(frac)
                        .desired_width(ui.available_width())
                        .text(format!("已完成 {}/{} | 当前第 {} 个", done, total, self.current_video_index + 1)),
                );
            });
        }

        self.render_ffmpeg_dialog(ctx);
        self.render_completion_dialog(ctx);
        self.render_star_hint_dialog(ctx);
        self.process_thumbnail_queue(ctx);

        if self.app_mode == AppMode::Sorting {
            self.handle_keyboard_input(ctx);
        }
    }
}