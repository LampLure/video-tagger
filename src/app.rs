use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;

use eframe::egui;
use egui::{Color32, RichText, StrokeKind, Vec2};

use crate::audio::AudioPlayer;
use crate::cache::ScreenshotCache;
use crate::config::{self, AppConfig, VideoFile};
use crate::ffmpeg;
use crate::progress;
use crate::scanner;
use crate::tags::TagLibrary;

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
    thumbnail_errors: HashSet<usize>,

    current_video_index: usize,
    screenshot_interval: f64,
    screenshot_start_sec: f64,
    screenshot_paths: Vec<PathBuf>,
    screenshot_textures: HashMap<String, egui::TextureHandle>,
    screenshot_error: Option<String>,
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
        Self {
            app_mode: AppMode::Fresh,
            config: AppConfig::default(),
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
            thumbnail_errors: HashSet::new(),

            current_video_index: 0,
            screenshot_interval: 10.0,
            screenshot_start_sec: 0.0,
            screenshot_paths: Vec::new(),
            screenshot_textures: HashMap::new(),
            screenshot_error: None,
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

        if self.ffmpeg_path.is_none() && !self.ffmpeg_error {
            self.ffmpeg_path = ffmpeg::find_ffmpeg();
            if self.ffmpeg_path.is_none() {
                self.ffmpeg_dialog_open = true;
                self.ffmpeg_error = true;
            }
        }

        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Video Tagger");
                ui.separator();
                ui.label(match self.app_mode {
                    AppMode::Fresh => "请选择文件夹",
                    AppMode::Overview => "总览模式",
                    AppMode::Sorting => "分拣模式",
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("FFmpeg").clicked() {
                        self.ffmpeg_dialog_open = true;
                    }
                    if let Some(ref path) = self.ffmpeg_path {
                        ui.label(RichText::new(path.display().to_string()).small().color(Color32::from_gray(150)));
                    }
                });
            });
        });

        egui::SidePanel::left("sidebar")
            .min_width(220.0)
            .max_width(260.0)
            .resizable(false)
            .show(ctx, |ui| self.render_sidebar(ui));

        egui::CentralPanel::default().show(ctx, |ui| match self.app_mode {
            AppMode::Fresh => self.render_welcome(ui),
            AppMode::Overview => self.render_overview(ui, ctx),
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

impl VideoTaggerApp {
    fn processed_count(&self) -> usize {
        let identifier = self.folder_progress.as_ref().map(|p| p.identifier.as_str()).unwrap_or("");
        self.videos
            .iter()
            .filter(|v| config::parse_video_name(&v.filename).map(|p| p.identifier == identifier).unwrap_or(false))
            .count()
    }

    fn current_effective_interval(&self) -> f64 {
        let duration = self.videos.get(self.current_video_index).and_then(|v| v.duration_secs).unwrap_or(0.0);
        let remaining = (duration - self.screenshot_start_sec).max(0.0);
        if duration > 0.0 && (duration < self.screenshot_interval * 10.0 || remaining < self.screenshot_interval * 10.0) {
            (remaining / 10.0).max(0.1)
        } else {
            self.screenshot_interval.max(0.1)
        }
    }

    fn reset_edit_state(&mut self) {
        self.current_labels.clear();
        self.undone_labels.clear();
        self.is_star_phase = false;
        self.is_starred = false;
        self.pending_overwrite_once = false;
        self.show_star_hint = false;
        self.playing_screenshot = None;
        self.screenshot_error = None;
    }

    fn hydrate_labels_from_filename(&mut self) {
        self.current_labels.clear();
        self.undone_labels.clear();
        self.is_starred = false;
        if let Some(video) = self.videos.get(self.current_video_index) {
            if let Some(parsed) = config::parse_video_name(&video.filename) {
                self.current_labels = parsed.labels;
                self.is_starred = parsed.starred;
            }
        }
    }

    fn begin_edit_video(&mut self, index: usize, independent: bool) {
        if index >= self.videos.len() {
            return;
        }
        self.current_video_index = index;
        self.screenshot_interval = self.config.screenshot_interval;
        self.screenshot_textures.clear();
        self.reset_edit_state();
        self.load_current_screenshots();
        self.hydrate_labels_from_filename();
        self.independent_edit = if independent { Some(index) } else { None };
        self.app_mode = AppMode::Sorting;
    }

    fn render_sidebar(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.heading("控制面板");
            ui.separator();
        });

        if ui.add_sized([ui.available_width(), 32.0], egui::Button::new("选择文件夹")).clicked() {
            self.pick_folder();
        }

        if let Some(ref folder) = self.selected_folder {
            ui.add_space(6.0);
            ui.label(RichText::new("当前目录").strong());
            ui.label(RichText::new(folder.display().to_string()).small());
        }

        ui.add_space(10.0);
        let btn_text = match self.app_mode {
            AppMode::Fresh => if self.selected_folder.is_some() { "开始总览" } else { "请先选择文件夹" },
            AppMode::Overview => "进入分拣",
            AppMode::Sorting => "退出分拣",
        };
        let enabled = self.ffmpeg_path.is_some() && !(self.app_mode == AppMode::Fresh && self.selected_folder.is_none());
        if ui.add_enabled(enabled, egui::Button::new(btn_text).min_size(Vec2::new(ui.available_width(), 32.0))).clicked() {
            match self.app_mode {
                AppMode::Fresh => self.enter_overview(),
                AppMode::Overview => self.enter_sorting(),
                AppMode::Sorting => self.exit_sorting(),
            }
        }

        ui.add_space(12.0);
        ui.group(|ui| {
            ui.label(RichText::new("截图设置").strong());
            ui.horizontal(|ui| {
                ui.label("间隔秒");
                ui.add(egui::DragValue::new(&mut self.config.screenshot_interval).range(1.0..=300.0).speed(1.0));
            });
            ui.checkbox(&mut self.config.shift_lock, "覆盖文件名模式");
            ui.label(RichText::new("Shift+Enter 可临时覆盖一次").small().color(Color32::from_gray(150)));
        });

        if let Some(ref prog) = self.folder_progress {
            ui.add_space(10.0);
            ui.group(|ui| {
                ui.label(RichText::new("识别码").strong());
                ui.monospace(&prog.identifier);
                ui.label(format!("位数: {} | 视频: {}", prog.digit_count, self.videos.len()));
            });
        }

        if self.app_mode == AppMode::Sorting {
            ui.add_space(10.0);
            ui.group(|ui| {
                if let Some(video) = self.videos.get(self.current_video_index) {
                    ui.label(RichText::new("当前视频").strong());
                    ui.label(&video.filename);
                    let dur = video.duration_secs.unwrap_or(0.0);
                    if dur > 0.0 {
                        ui.label(format!("时长: {:.1}s", dur));
                    }
                    let end = self.screenshot_start_sec + self.current_effective_interval() * 9.0;
                    ui.label(format!("截图: {:.1}s - {:.1}s", self.screenshot_start_sec, end));
                    ui.label("R 后移；Shift+R 回退");
                }
            });
        }
    }

    fn pick_folder(&mut self) {
        if let Some(path) = rfd::FileDialog::new().pick_folder() {
            self.selected_folder = Some(path);
            self.app_mode = AppMode::Fresh;
            self.videos.clear();
            self.folder_progress = None;
        }
    }

    fn enter_overview(&mut self) {
        if let Some(ref folder) = self.selected_folder {
            self.videos = scanner::scan_videos(folder);
            self.overview_thumbnails.clear();
            self.thumbnail_queue.clear();
            self.thumbnail_loaded.clear();
            self.thumbnail_errors.clear();
            self.overview_search.clear();
            self.folder_progress = Some(progress::init_progress_for_folder(folder, &self.videos));
            if let Some(ref prog) = self.folder_progress {
                progress::save_progress(folder, prog);
            }
            self.app_mode = AppMode::Overview;
        }
    }

    fn enter_sorting(&mut self) {
        if self.videos.is_empty() {
            return;
        }
        let start_idx = self.folder_progress.as_ref().map(|p| p.last_processed).unwrap_or(0);
        let start_idx = start_idx.min(self.videos.len().saturating_sub(1));
        self.begin_edit_video(start_idx, false);
    }

    fn exit_sorting(&mut self) {
        self.reset_edit_state();
        self.independent_edit = None;
        self.app_mode = AppMode::Overview;
    }

    fn render_welcome(&self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(100.0);
            ui.heading(RichText::new("视频标签分拣工具").size(32.0));
            ui.add_space(20.0);
            ui.label("请先在左侧边栏选择一个包含视频的文件夹");
            ui.label("选择后点击「开始总览」，再进入分拣。");
        });
    }

    fn sorted_filtered_indices(&self) -> Vec<usize> {
        let search_lower = self.overview_search.to_lowercase();
        let mut filtered: Vec<usize> = self.videos
            .iter()
            .enumerate()
            .filter(|(_, v)| search_lower.is_empty() || v.filename.to_lowercase().contains(&search_lower))
            .map(|(i, _)| i)
            .collect();

        filtered.sort_by(|&a, &b| {
            let va = &self.videos[a];
            let vb = &self.videos[b];
            match self.overview_sort {
                SortMode::Name => va.filename.to_lowercase().cmp(&vb.filename.to_lowercase()),
                SortMode::Size => vb.size.cmp(&va.size),
                SortMode::Date => {
                    let ma = std::fs::metadata(&va.path).and_then(|m| m.modified()).ok();
                    let mb = std::fs::metadata(&vb.path).and_then(|m| m.modified()).ok();
                    mb.cmp(&ma).then_with(|| va.filename.to_lowercase().cmp(&vb.filename.to_lowercase()))
                }
            }
        });
        filtered
    }

    fn render_overview(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context) {
        ui.horizontal(|ui| {
            ui.heading("总览");
            ui.separator();
            ui.label(format!("{} 个视频", self.videos.len()));
        });
        ui.add_space(6.0);
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("搜索");
                ui.add_sized([260.0, 24.0], egui::TextEdit::singleline(&mut self.overview_search).hint_text("按文件名过滤"));
                if ui.button("清空").clicked() { self.overview_search.clear(); }
                ui.separator();
                ui.label("排序");
                egui::ComboBox::from_id_salt("sort_mode")
                    .selected_text(match self.overview_sort {
                        SortMode::Name => "文件名 A-Z",
                        SortMode::Date => "修改时间 新-旧",
                        SortMode::Size => "大小 大-小",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.overview_sort, SortMode::Name, "文件名 A-Z");
                        ui.selectable_value(&mut self.overview_sort, SortMode::Date, "修改时间 新-旧");
                        ui.selectable_value(&mut self.overview_sort, SortMode::Size, "大小 大-小");
                    });
            });
        });
        ui.add_space(8.0);

        let filtered = self.sorted_filtered_indices();
        let thumb_w = 190.0;
        let thumb_h = 142.0;
        let spacing = 10.0;
        let cols = (ui.available_width() / (thumb_w + spacing)).max(1.0) as usize;
        let total_rows = (filtered.len() + cols - 1) / cols;
        let row_height = thumb_h + 58.0 + spacing;

        egui::ScrollArea::vertical().id_salt("overview_scroll").show(ui, |ui| {
            let visible_rect = ui.clip_rect();
            let first_row = (visible_rect.top() / row_height).max(0.0) as usize;
            let last_row = ((visible_rect.bottom() / row_height).ceil() as usize + 1).min(total_rows);
            ui.add_space(first_row as f32 * row_height);
            for row in first_row..last_row {
                ui.horizontal(|ui| {
                    for col in 0..cols {
                        let idx = row * cols + col;
                        if idx >= filtered.len() { break; }
                        self.render_thumbnail_card(ui, filtered[idx], Vec2::new(thumb_w, thumb_h));
                    }
                });
                ui.add_space(spacing);
            }
            ui.add_space(total_rows.saturating_sub(last_row) as f32 * row_height);
        });
    }

    fn render_thumbnail_card(&mut self, ui: &mut egui::Ui, video_idx: usize, thumb_size: Vec2) {
        let filename = self.videos[video_idx].filename.clone();
        let processed = self.is_processed(video_idx);
        let mut open_edit = false;
        egui::Frame::group(ui.style()).fill(if processed { Color32::from_rgb(35, 48, 35) } else { Color32::from_gray(28) }).show(ui, |ui| {
            ui.set_width(thumb_size.x);
            let (rect, response) = ui.allocate_exact_size(thumb_size, egui::Sense::click());
            if let Some(texture) = self.overview_thumbnails.get(&video_idx) {
                ui.put(rect, egui::Image::new(texture).fit_to_exact_size(rect.size()));
            } else if self.thumbnail_errors.contains(&video_idx) {
                ui.painter().rect_filled(rect, 4.0, Color32::from_rgb(70, 30, 30));
                ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "Error", egui::FontId::proportional(18.0), Color32::LIGHT_RED);
            } else {
                ui.painter().rect_filled(rect, 4.0, Color32::from_gray(45));
                ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "加载中", egui::FontId::proportional(14.0), Color32::from_gray(150));
                if !self.thumbnail_loaded.contains(&video_idx) {
                    self.thumbnail_queue.push_back(video_idx);
                    self.thumbnail_loaded.insert(video_idx);
                }
            }
            if response.double_clicked() { open_edit = true; }
            ui.add_space(4.0);
            ui.label(RichText::new(if processed { "已分拣" } else { "未分拣" }).small().color(if processed { Color32::LIGHT_GREEN } else { Color32::from_gray(150) }));
            ui.label(RichText::new(filename).small());
        });
        if open_edit {
            self.begin_edit_video(video_idx, true);
        }
    }

    fn process_thumbnail_queue(&mut self, ctx: &egui::Context) {
        if let Some(video_idx) = self.thumbnail_queue.pop_front() {
            if let Some(video) = self.videos.get(video_idx) {
                let thumb_path = config::cache_dir().join("thumbs").join(format!("thumb_{}.png", video_idx));
                let _ = std::fs::create_dir_all(thumb_path.parent().unwrap());
                if ffmpeg::extract_thumbnail(&video.path, &thumb_path).is_ok() {
                    if let Ok(img_data) = std::fs::read(&thumb_path) {
                        if let Ok(img) = image::load_from_memory(&img_data) {
                            let rgba = img.to_rgba8();
                            let size = [rgba.width() as _, rgba.height() as _];
                            let pixels = rgba.into_raw();
                            let color_img = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
                            let texture = ctx.load_texture(format!("thumb_{}", video_idx), egui::ImageData::Color(color_img.into()), egui::TextureOptions::LINEAR);
                            self.overview_thumbnails.insert(video_idx, texture);
                            return;
                        }
                    }
                }
                self.thumbnail_errors.insert(video_idx);
            }
        }
    }

    fn render_sorting(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let available = ui.available_size();
        let list_width = available.x.clamp(900.0, 1600.0) * 0.20;
        let list_width = list_width.clamp(190.0, 280.0);
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.set_width((available.x - list_width - 12.0).max(600.0));
                self.render_sorting_header(ui);
                ui.add_space(6.0);
                self.render_screenshot_area(ui, ctx);
                ui.add_space(8.0);
                self.render_label_preview_bar(ui);
                ui.add_space(8.0);
                self.render_tag_grid(ui);
            });
            ui.separator();
            ui.vertical(|ui| {
                ui.set_width(list_width);
                self.render_video_list(ui);
            });
        });
    }

    fn render_sorting_header(&mut self, ui: &mut egui::Ui) {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.horizontal(|ui| {
                if let Some(video) = self.videos.get(self.current_video_index) {
                    ui.label(RichText::new(format!("{} / {}", self.current_video_index + 1, self.videos.len())).strong());
                    ui.separator();
                    ui.label(&video.filename);
                    if self.independent_edit.is_some() {
                        ui.label(RichText::new("独立编辑").color(Color32::YELLOW));
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label("Enter 标签确认 → 打星 → Enter 保存");
                });
            });
        });
    }

    fn load_current_screenshots(&mut self) {
        if self.videos.is_empty() { return; }
        let duration = self.videos[self.current_video_index].ensure_duration();
        self.screenshot_start_sec = 0.0;
        self.extract_current_screenshots(duration);
        self.prefetch_nearby_screenshots();
    }

    fn extract_current_screenshots(&mut self, duration: f64) {
        let video_path = self.videos[self.current_video_index].path.clone();
        let paths = self.screenshot_cache.get_or_extract_screenshots(&video_path, self.screenshot_start_sec, self.screenshot_interval, 10, duration);
        self.screenshot_error = if paths.is_empty() {
            Some("Error: 无法读取该视频或 ffmpeg 截图失败".to_string())
        } else { None };
        self.screenshot_paths = paths;
        self.screenshot_textures.clear();
    }

    fn advance_screenshots(&mut self, backward: bool) {
        if self.videos.is_empty() { return; }
        let duration = self.videos[self.current_video_index].ensure_duration();
        let step = self.screenshot_interval * 10.0;
        if backward {
            self.screenshot_start_sec = (self.screenshot_start_sec - step).max(0.0);
        } else {
            let max_start = if duration <= step { 0.0 } else { ((duration - 0.001) / step).floor() * step };
            self.screenshot_start_sec = (self.screenshot_start_sec + step).min(max_start);
        }
        self.extract_current_screenshots(duration);
        self.prefetch_nearby_screenshots();
    }

    fn prefetch_nearby_screenshots(&mut self) {
        if self.videos.is_empty() { return; }
        let start = self.current_video_index.saturating_sub(5);
        let end = (self.current_video_index + 5).min(self.videos.len().saturating_sub(1));
        for idx in start..=end {
            let duration = self.videos[idx].ensure_duration();
            let path = self.videos[idx].path.clone();
            let _ = self.screenshot_cache.get_or_extract_screenshots(&path, 0.0, self.screenshot_interval, 10, duration);
        }

        let duration = self.videos[self.current_video_index].duration_secs.unwrap_or(0.0);
        if duration > 300.0 {
            let step = self.screenshot_interval * 10.0;
            for offset in -5..=5 {
                let mut start_sec = self.screenshot_start_sec + offset as f64 * step;
                if start_sec < 0.0 { start_sec = 0.0; }
                let max_start = ((duration - 0.001) / step).floor().max(0.0) * step;
                start_sec = start_sec.min(max_start);
                let path = self.videos[self.current_video_index].path.clone();
                let _ = self.screenshot_cache.get_or_extract_screenshots(&path, start_sec, self.screenshot_interval, 10, duration);
            }
        }
    }

    fn render_screenshot_area(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if let Some(ref err) = self.screenshot_error {
            egui::Frame::group(ui.style()).fill(Color32::from_rgb(60, 25, 25)).show(ui, |ui| {
                ui.set_min_height(220.0);
                ui.centered_and_justified(|ui| ui.label(RichText::new(err).color(Color32::LIGHT_RED).size(22.0)));
            });
            return;
        }
        if self.screenshot_paths.is_empty() {
            ui.label("加载截图中...");
            return;
        }

        for (idx, path) in self.screenshot_paths.iter().enumerate() {
            let tex_id = format!("scr_{}_{}_{}", self.current_video_index, (self.screenshot_start_sec * 10.0) as u64, idx);
            if !self.screenshot_textures.contains_key(&tex_id) {
                if let Ok(data) = std::fs::read(path) {
                    if let Ok(img) = image::load_from_memory(&data) {
                        let rgba = img.to_rgba8();
                        let size = [rgba.width() as _, rgba.height() as _];
                        let pixels = rgba.into_raw();
                        let color_img = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
                        let texture = ctx.load_texture(tex_id.clone(), egui::ImageData::Color(color_img.into()), egui::TextureOptions::LINEAR);
                        self.screenshot_textures.insert(tex_id, texture);
                    }
                }
            }
        }

        let cols = 5;
        let gap = 8.0;
        let cell_w = ((ui.available_width() - gap * 4.0) / cols as f32).clamp(120.0, 260.0);
        let cell_h = cell_w * 9.0 / 16.0;
        let shown_interval = self.current_effective_interval();

        egui::Frame::group(ui.style()).show(ui, |ui| {
            for row in 0..2 {
                ui.horizontal(|ui| {
                    for col in 0..cols {
                        let idx = row * cols + col;
                        let (rect, response) = ui.allocate_exact_size(Vec2::new(cell_w, cell_h), egui::Sense::click());
                        let is_playing = self.playing_screenshot == Some(idx);
                        let border_color = if is_playing { Color32::YELLOW } else if response.hovered() { Color32::WHITE } else { Color32::from_gray(90) };
                        ui.painter().rect_filled(rect, 3.0, Color32::from_gray(20));
                        ui.painter().rect_stroke(rect, 3.0, egui::Stroke::new(2.0, border_color), StrokeKind::Middle);
                        let tex_id = format!("scr_{}_{}_{}", self.current_video_index, (self.screenshot_start_sec * 10.0) as u64, idx);
                        if let Some(tex) = self.screenshot_textures.get(&tex_id) {
                            ui.put(rect, egui::Image::new(tex).fit_to_exact_size(rect.size()));
                        }
                        let time_sec = self.screenshot_start_sec + idx as f64 * shown_interval;
                        ui.painter().text(rect.left_bottom() + egui::vec2(5.0, -5.0), egui::Align2::LEFT_BOTTOM, format!("{:.1}s", time_sec), egui::FontId::proportional(11.0), Color32::WHITE);
                        if response.clicked() {
                            self.audio_player.play_clip(&self.videos[self.current_video_index].path, time_sec);
                            self.playing_screenshot = Some(idx);
                        }
                        ui.add_space(gap);
                    }
                });
                ui.add_space(gap);
            }
        });

        if self.playing_screenshot.is_some() && !self.audio_player.is_playing() {
            self.playing_screenshot = None;
        }
    }

    fn render_label_preview_bar(&mut self, ui: &mut egui::Ui) {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("标签预览").strong());
                let mut remove_active: Option<usize> = None;
                for (i, label) in self.current_labels.iter().enumerate() {
                    egui::Frame::NONE.fill(Color32::from_rgb(55, 95, 175)).stroke(egui::Stroke::new(1.0, Color32::WHITE)).inner_margin(4.0).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(label);
                            if ui.small_button("x").clicked() { remove_active = Some(i); }
                        });
                    });
                }
                let mut remove_undone: Option<usize> = None;
                for (i, label) in self.undone_labels.iter().enumerate().rev() {
                    egui::Frame::NONE.fill(Color32::from_gray(55)).stroke(egui::Stroke::new(1.0, Color32::from_gray(95))).inner_margin(4.0).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(label).strikethrough().color(Color32::from_gray(170)));
                            if ui.small_button("x").clicked() { remove_undone = Some(i); }
                        });
                    });
                }
                if self.current_labels.is_empty() && self.undone_labels.is_empty() {
                    ui.label(RichText::new("无标签，直接 Enter 将进入无星确认").small().color(Color32::from_gray(150)));
                }
                if let Some(i) = remove_active { self.current_labels.remove(i); self.undone_labels.clear(); }
                if let Some(i) = remove_undone { self.undone_labels.remove(i); }
            });
        });
    }

    fn render_tag_grid(&mut self, ui: &mut egui::Ui) {
        let tag_names = self.tag_library.sorted_names();
        let cols = 9;
        let rows = 3;
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("标签栏").strong());
                ui.label(RichText::new("方向键移动，数字 1-9 选择当前行").small().color(Color32::from_gray(150)));
            });
            ui.add_space(4.0);
            let btn_w = ((ui.available_width() - 8.0 * (cols as f32 - 1.0)) / cols as f32).max(72.0);
            for row in 0..rows {
                ui.horizontal(|ui| {
                    for col in 0..cols {
                        let idx = row * cols + col;
                        let is_selected = self.tag_row == row && self.tag_col == col;
                        if idx < tag_names.len() {
                            let tag = &tag_names[idx];
                            let fill = if is_selected { Color32::from_rgb(90, 125, 215) } else { Color32::from_gray(45) };
                            let resp = ui.add(egui::Button::new(RichText::new(format!("{} {}", col + 1, tag)).size(12.0)).fill(fill).min_size(Vec2::new(btn_w, 28.0)));
                            if resp.clicked() { self.add_label(tag.clone()); }
                            if resp.secondary_clicked() { self.tag_library.remove_tag(tag); self.tag_library.save(); }
                        } else if idx == tag_names.len() && !self.editing_new_tag {
                            if ui.add(egui::Button::new("+").min_size(Vec2::new(btn_w, 28.0))).clicked() {
                                self.editing_new_tag = true;
                                self.new_tag_text.clear();
                            }
                        } else if self.editing_new_tag && idx == tag_names.len() {
                            let resp = ui.add_sized([btn_w, 28.0], egui::TextEdit::singleline(&mut self.new_tag_text).hint_text("新标签"));
                            if resp.lost_focus() { self.finish_new_tag(); }
                        } else {
                            ui.add_sized(Vec2::new(btn_w, 28.0), egui::Label::new(""));
                        }
                    }
                });
                ui.add_space(5.0);
            }
        });
    }

    fn is_processed(&self, index: usize) -> bool {
        let identifier = self.folder_progress.as_ref().map(|p| p.identifier.as_str()).unwrap_or("");
        self.videos.get(index)
            .and_then(|v| config::parse_video_name(&v.filename))
            .map(|p| p.identifier == identifier)
            .unwrap_or(false)
    }

    fn render_video_list(&mut self, ui: &mut egui::Ui) {
        ui.heading("视频列表");
        ui.label(RichText::new("点击回看并修改").small().color(Color32::from_gray(150)));
        ui.separator();
        let item_height = 42.0;
        egui::ScrollArea::vertical().id_salt("video_list_scroll").show_rows(ui, item_height, self.videos.len(), |ui, range| {
            for i in range {
                let is_current = i == self.current_video_index;
                let processed = self.is_processed(i);
                let name = self.videos[i].filename.clone();
                let fill = if is_current { Color32::from_rgb(60, 100, 180) } else if processed { Color32::from_rgb(35, 70, 35) } else { Color32::from_gray(35) };
                let text = format!("{}{}\n{}", if is_current { "▶ " } else { "" }, i + 1, name);
                let resp = ui.add_sized(Vec2::new(ui.available_width(), item_height - 4.0), egui::Button::new(RichText::new(text).small()).fill(fill));
                if resp.clicked() {
                    self.begin_edit_video(i, false);
                }
            }
        });
    }

    fn add_label(&mut self, label: String) {
        self.current_labels.push(label);
        self.undone_labels.clear();
    }

    fn undo_label(&mut self) {
        if let Some(label) = self.current_labels.pop() {
            self.undone_labels.push(label);
        }
    }

    fn redo_label(&mut self) {
        if let Some(label) = self.undone_labels.pop() {
            self.current_labels.push(label);
        }
    }

    fn finish_new_tag(&mut self) {
        if !self.new_tag_text.trim().is_empty() {
            self.tag_library.add_tag(&self.new_tag_text);
            self.tag_library.save();
            self.new_tag_text.clear();
        }
        self.editing_new_tag = false;
    }

    fn confirm_labels_and_enter_star(&mut self, overwrite_once: bool) {
        if self.is_star_phase {
            self.finalize_current_video();
            return;
        }
        self.pending_overwrite_once = overwrite_once;
        self.is_star_phase = true;
        self.show_star_hint = true;
    }

    fn finalize_current_video(&mut self) {
        if self.videos.is_empty() { return; }
        let video = self.videos[self.current_video_index].clone();
        let extension = video.extension.clone();
        let original_basename = video.filename.clone();
        let should_rename = !self.current_labels.is_empty() || self.is_starred;

        if let Some(ref prog) = self.folder_progress {
            let overwrite = self.config.shift_lock || self.pending_overwrite_once;
            let new_name = config::format_video_name(&prog.identifier, self.current_video_index, prog.digit_count, &self.current_labels, self.is_starred, &original_basename, &extension, overwrite);
            let parent = video.path.parent().unwrap_or(std::path::Path::new("."));
            let mut final_path = parent.join(&new_name);

            if should_rename {
                scanner::resolve_name_conflict(&mut final_path);
                if std::fs::rename(&video.path, &final_path).is_ok() {
                    if let Some(updated) = self.videos.get_mut(self.current_video_index) {
                        updated.path = final_path.clone();
                        updated.filename = final_path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or(new_name);
                        updated.extension = final_path.extension().map(|s| s.to_string_lossy().to_string()).unwrap_or(extension);
                    }
                    self.overview_thumbnails.remove(&self.current_video_index);
                    self.thumbnail_loaded.remove(&self.current_video_index);
                }
            }
            if !self.current_labels.is_empty() {
                self.tag_library.record_usage(&self.current_labels);
                self.tag_library.save();
            }
        }

        if let Some(ref mut prog) = self.folder_progress {
            prog.last_processed = prog.last_processed.max(self.current_video_index + 1);
            if let Some(ref folder) = self.selected_folder { progress::save_progress(folder, prog); }
        }

        let independent = self.independent_edit.is_some();
        let next_idx = self.current_video_index + 1;
        if independent {
            self.reset_edit_state();
            self.independent_edit = None;
            self.app_mode = AppMode::Overview;
            return;
        }
        if next_idx >= self.videos.len() {
            self.reset_edit_state();
            self.app_mode = AppMode::Overview;
            self.show_completion = true;
            return;
        }
        self.begin_edit_video(next_idx, false);
    }

    fn render_ffmpeg_dialog(&mut self, ctx: &egui::Context) {
        if !self.ffmpeg_dialog_open { return; }
        egui::Window::new("FFmpeg 设置").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            if let Some(ref path) = self.ffmpeg_path {
                ui.label(format!("已找到: {}", path.display()));
                if ui.button("重新扫描").clicked() {
                    self.ffmpeg_path = ffmpeg::find_ffmpeg();
                }
            } else {
                ui.label("未找到 ffmpeg，请安装或手动指定路径:");
                ui.horizontal(|ui| { ui.label("路径:"); ui.text_edit_singleline(&mut self.ffmpeg_custom_path); });
                if ui.button("浏览").clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_file() { self.ffmpeg_custom_path = path.to_string_lossy().to_string(); }
                }
                if ui.button("确认").clicked() {
                    let p = PathBuf::from(&self.ffmpeg_custom_path);
                    if p.exists() {
                        ffmpeg::set_ffmpeg_path(p.clone());
                        self.ffmpeg_path = Some(p);
                        self.ffmpeg_error = false;
                        self.ffmpeg_dialog_open = false;
                    }
                }
            }
            ui.separator();
            if ui.button("关闭").clicked() { self.ffmpeg_dialog_open = false; }
        });
    }

    fn render_completion_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_completion { return; }
        egui::Window::new("完成").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.label("全部分拣完成！");
            if ui.button("确定").clicked() { self.show_completion = false; }
        });
    }

    fn render_star_hint_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_star_hint { return; }
        egui::Window::new("打星确认").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_TOP, [0.0, 80.0]).auto_sized().show(ctx, |ui| {
            ui.label(if self.is_starred { "★ 已打星 | 任意键切换 | Enter 保存" } else { "☆ 未打星 | 任意键打星 | Enter 保存" });
            if ui.button("隐藏提示").clicked() { self.show_star_hint = false; }
        });
    }

    fn handle_keyboard_input(&mut self, ctx: &egui::Context) {
        let input = ctx.input(|i| i.clone());
        if self.editing_new_tag {
            if input.key_pressed(egui::Key::Enter) { self.finish_new_tag(); }
            return;
        }

        if self.is_star_phase {
            if input.key_pressed(egui::Key::Enter) {
                self.finalize_current_video();
                return;
            }
            let any_key = input.events.iter().any(|e| matches!(e, egui::Event::Key { pressed: true, key, .. } if *key != egui::Key::Enter));
            if any_key { self.is_starred = !self.is_starred; }
            return;
        }

        if input.key_pressed(egui::Key::R) { self.advance_screenshots(input.modifiers.shift); }
        if input.key_pressed(egui::Key::Z) && !input.modifiers.ctrl { self.undo_label(); }
        if input.key_pressed(egui::Key::Y) && !input.modifiers.ctrl { self.redo_label(); }
        if input.key_pressed(egui::Key::Enter) {
            self.confirm_labels_and_enter_star(input.modifiers.shift);
            return;
        }

        if input.key_pressed(egui::Key::ArrowUp) { self.tag_row = self.tag_row.saturating_sub(1); }
        if input.key_pressed(egui::Key::ArrowDown) { self.tag_row = (self.tag_row + 1).min(2); }
        if input.key_pressed(egui::Key::ArrowLeft) { self.tag_col = self.tag_col.saturating_sub(1); }
        if input.key_pressed(egui::Key::ArrowRight) { self.tag_col = (self.tag_col + 1).min(8); }

        let tag_names = self.tag_library.sorted_names();
        let num_keys = [egui::Key::Num1, egui::Key::Num2, egui::Key::Num3, egui::Key::Num4, egui::Key::Num5, egui::Key::Num6, egui::Key::Num7, egui::Key::Num8, egui::Key::Num9];
        for (n, key) in num_keys.iter().enumerate() {
            if input.key_pressed(*key) {
                let tag_idx = self.tag_row * 9 + n;
                if tag_idx < tag_names.len() { self.add_label(tag_names[tag_idx].clone()); }
            }
        }
    }
}
