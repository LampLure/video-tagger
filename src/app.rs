use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::process::Child;
use std::sync::mpsc;
use std::time::Duration;

use eframe::egui;
use egui::{Color32, RichText, StrokeKind, Vec2};

use crate::ai::{self, AiAnalysisResult, AiEvent, AiFailureRecord, AiRuntimeConfig, AiServiceProps, AiVideoJob};
use crate::audio::AudioPlayer;
use crate::cache::ScreenshotCache;
use crate::config::{self, AppConfig, VideoFile};
use crate::ffmpeg;
use crate::progress;
use crate::scanner;
use crate::tags::{TagLibrary, MAX_TAG_CATEGORIES, MAX_TAGS_PER_CATEGORY, STAR_CATEGORY_NAME};

mod behavior;
mod ai_behavior;
mod ai_ui;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiServiceState {
    Disconnected,
    Starting,
    ConnectedOwned,
    ConnectedExternal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiBatchState {
    Idle,
    Running,
    AwaitingConfirmation,
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
    selected_screenshot_index: usize,

    current_labels: Vec<String>,
    current_score: Option<u8>,
    undone_labels: Vec<String>,
    is_starred: bool,
    pending_overwrite_once: bool,

    active_category_index: usize,
    selected_tag_index: usize,
    editing_new_tag: bool,
    new_tag_text: String,
    editing_new_category: bool,
    new_category_text: String,

    audio_player: AudioPlayer,
    playing_screenshot: Option<usize>,
    screenshot_cache: ScreenshotCache,

    show_completion: bool,
    independent_edit: Option<usize>,

    ffmpeg_custom_path: String,
    ffmpeg_dialog_open: bool,
    ui_scale_percent_input: String,
    video_list_follow_index: Option<usize>,

    ai_mode: bool,
    ai_scripts: Vec<PathBuf>,
    ai_selected_script: usize,
    ai_service_state: AiServiceState,
    ai_model_process: Option<Child>,
    ai_service_props: Option<AiServiceProps>,
    ai_batch_state: AiBatchState,
    ai_log: Vec<String>,
    ai_failures: Vec<AiFailureRecord>,
    ai_success_count: usize,
    ai_pending_result: Option<AiAnalysisResult>,
    ai_notice: Option<String>,
    ai_confirm_cancel: bool,
    ai_work_id: u64,
    ai_rx: Option<mpsc::Receiver<AiEvent>>,
    ai_tx: mpsc::Sender<AiEvent>,
}

impl Default for VideoTaggerApp {
    fn default() -> Self {
        let (thumbnail_tx, thumbnail_rx) = mpsc::channel();
        let (screenshot_tx, screenshot_rx) = mpsc::channel();
        let (ai_tx, ai_rx) = mpsc::channel();
        let config = AppConfig::load();
        let ui_scale_percent_input = format!("{}", (config.ui_scale.clamp(0.5, 3.0) * 100.0).round() as i32);
        let ai_scripts = ai::list_model_scripts();
        Self {
            app_mode: AppMode::Fresh,
            config,
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
            selected_screenshot_index: 0,

            current_labels: Vec::new(),
            current_score: None,
            undone_labels: Vec::new(),
            is_starred: false,
            pending_overwrite_once: false,

            active_category_index: 0,
            selected_tag_index: 0,
            editing_new_tag: false,
            new_tag_text: String::new(),
            editing_new_category: false,
            new_category_text: String::new(),

            audio_player: AudioPlayer::new(),
            playing_screenshot: None,
            screenshot_cache: ScreenshotCache::new(500),

            show_completion: false,
            independent_edit: None,

            ffmpeg_custom_path: String::new(),
            ffmpeg_dialog_open: false,
            ui_scale_percent_input,
            video_list_follow_index: None,

            ai_mode: false,
            ai_scripts,
            ai_selected_script: 0,
            ai_service_state: AiServiceState::Disconnected,
            ai_model_process: None,
            ai_service_props: None,
            ai_batch_state: AiBatchState::Idle,
            ai_log: Vec::new(),
            ai_failures: Vec::new(),
            ai_success_count: 0,
            ai_pending_result: None,
            ai_notice: None,
            ai_confirm_cancel: false,
            ai_work_id: 0,
            ai_rx: Some(ai_rx),
            ai_tx,
        }
    }
}

impl Drop for VideoTaggerApp {
    fn drop(&mut self) {
        if let Some(mut child) = self.ai_model_process.take() {
            ai::stop_process_tree(&mut child);
        }
    }
}

impl eframe::App for VideoTaggerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let scale = self.config.ui_scale.clamp(0.5, 3.0);
        if (scale - ctx.pixels_per_point()).abs() > f32::EPSILON {
            ctx.set_pixels_per_point(scale);
        }

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
        self.poll_ai_events(ctx);

        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| self.render_top_bar(ui));
        self.render_ai_mode_toolbar(ctx);

        egui::TopBottomPanel::top("ui_scale_bar").show(ctx, |ui| {
            egui::Frame::none()
                .fill(if self.ai_mode { Color32::from_rgb(18, 28, 42) } else { Color32::from_gray(18) })
                .inner_margin(egui::Margin::symmetric(16, 6))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("UI 缩放").small().color(Color32::from_gray(160)));
                        let response = ui.add_sized(
                            [56.0, 24.0],
                            egui::TextEdit::singleline(&mut self.ui_scale_percent_input)
                                .hint_text("100")
                                .char_limit(3),
                        );
                        ui.label(RichText::new("%  范围 50-300，回车或点击应用生效").small().color(Color32::from_gray(130)));

                        let apply_requested = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        let apply_clicked = ui.button("应用").clicked();
                        if apply_requested || apply_clicked {
                            let parsed = self.ui_scale_percent_input.trim().parse::<f32>().unwrap_or(100.0);
                            let percent = parsed.clamp(50.0, 300.0);
                            self.ui_scale_percent_input = format!("{}", percent.round() as i32);
                            self.config.ui_scale = percent / 100.0;
                            self.config.save();
                            ctx.set_pixels_per_point(self.config.ui_scale);
                        }

                        if ui.button("重置100%").clicked() {
                            self.config.ui_scale = 1.0;
                            self.ui_scale_percent_input = "100".to_string();
                            self.config.save();
                            ctx.set_pixels_per_point(1.0);
                        }
                        ui.separator();
                        let help = if self.ai_mode {
                            "AI 模式：启动模型后点击 AI 分析；等待确认时 Space 接受 / Delete 重生"
                        } else {
                            "Space 确认标签 / Q-E 翻页 / WASD 选图 / X 播放音频"
                        };
                        ui.label(RichText::new(help).small().color(Color32::from_gray(130)));
                    });
                });
        });

        egui::SidePanel::left("sidebar")
            .resizable(false)
            .min_width(210.0)
            .max_width(if self.ai_mode { 320.0 } else { 260.0 })
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    self.render_sidebar(ui);
                    if self.ai_mode {
                        self.render_ai_sidebar_settings(ui);
                    }
                });
            });

        let central_frame = if self.app_mode == AppMode::Sorting {
            egui::Frame::none().fill(if self.ai_mode { Color32::from_rgb(20, 27, 38) } else { Color32::from_gray(24) })
        } else {
            egui::Frame::central_panel(&ctx.style())
        };

        egui::CentralPanel::default().frame(central_frame).show(ctx, |ui| match self.app_mode {
            AppMode::Fresh => self.render_welcome(ui),
            AppMode::Overview => self.render_overview(ui),
            AppMode::Sorting => self.render_sorting(ui, ctx),
        });

        if self.ai_mode && self.app_mode == AppMode::Sorting && !self.videos.is_empty() {
            let screen = ctx.screen_rect();
            let sidebar_w = 320.0;
            let workspace_w = (screen.width() - sidebar_w).max(1.0);
            let list_w = (workspace_w * 0.26).clamp(280.0, 410.0).min((workspace_w - 760.0).max(260.0));
            let x = sidebar_w + 12.0;
            let w = (screen.width() - sidebar_w - list_w - 36.0).max(420.0);
            let y = (screen.bottom() - 260.0).max(360.0);
            egui::Area::new("ai_output_workspace_panel".into())
                .order(egui::Order::Foreground)
                .fixed_pos(egui::pos2(x, y))
                .show(ctx, |ui| {
                    ui.set_width(w);
                    self.render_ai_output_area(ui);
                });
        }

        if self.app_mode == AppMode::Sorting && !self.videos.is_empty() {
            egui::TopBottomPanel::bottom("progress_bar").show(ctx, |ui| {
                let total = self.videos.len();
                let done = self.processed_count();
                let frac = done as f32 / total as f32;
                ui.horizontal(|ui| {
                    ui.add(
                        egui::ProgressBar::new(frac)
                            .desired_width((ui.available_width() - 120.0).max(120.0))
                            .text(format!("已完成 {}/{} | 当前第 {} 个", done, total, self.current_video_index + 1)),
                    );
                    if ui.button("跳过当前视频").clicked() {
                        self.skip_current_video();
                    }
                });
            });
        }

        self.render_ffmpeg_dialog(ctx);
        self.render_completion_dialog(ctx);
        self.render_ai_notice(ctx);
        self.render_ai_cancel_dialog(ctx);
        self.render_ai_control_window(ctx);
        self.process_thumbnail_queue(ctx);

        if self.app_mode == AppMode::Sorting {
            if !self.handle_ai_keyboard_input(ctx) {
                self.handle_keyboard_input(ctx);
            }
        }
    }
}
