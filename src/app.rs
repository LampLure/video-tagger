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

    // Overview
    overview_search: String,
    overview_sort: SortMode,
    overview_thumbnails: HashMap<usize, egui::TextureHandle>,
    thumbnail_queue: VecDeque<usize>,
    thumbnail_loaded: HashSet<usize>,

    // Sorting
    current_video_index: usize,
    screenshot_interval: f64,
    screenshot_start_sec: f64,
    screenshot_paths: Vec<PathBuf>,
    screenshot_textures: HashMap<String, egui::TextureHandle>,
    current_labels: Vec<String>,
    undo_stack: Vec<Vec<String>>,
    redo_stack: Vec<Vec<String>>,
    is_star_phase: bool,
    is_starred: bool,

    // Tag grid
    tag_row: usize,
    tag_col: usize,

    // New tag editing
    editing_new_tag: bool,
    new_tag_text: String,

    // Audio
    audio_player: AudioPlayer,
    playing_screenshot: Option<usize>,

    // Cache
    screenshot_cache: ScreenshotCache,

    // Dialogs
    show_completion: bool,
    show_star_hint: bool,

    // Independent edit
    independent_edit: Option<usize>,

    // FFmpeg dialog
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

            current_video_index: 0,
            screenshot_interval: 10.0,
            screenshot_start_sec: 0.0,
            screenshot_paths: Vec::new(),
            screenshot_textures: HashMap::new(),
            current_labels: Vec::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            is_star_phase: false,
            is_starred: false,

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

        // Check ffmpeg on first frame
        if self.ffmpeg_path.is_none() && !self.ffmpeg_error {
            // Try saved path first, then scan
            if !self.config.last_folder.is_none() {
                // Try PATH scan first
            }
            self.ffmpeg_path = ffmpeg::find_ffmpeg();
            if self.ffmpeg_path.is_none() {
                self.ffmpeg_dialog_open = true;
                self.ffmpeg_error = true;
            }
        }

        // Top bar
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.heading("Video Tagger");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("FFmpeg").clicked() {
                        self.ffmpeg_dialog_open = true;
                    }
                });
            });
        });

        // Main layout
        egui::SidePanel::left("sidebar")
            .min_width(180.0)
            .resizable(false)
            .show(ctx, |ui| {
                self.render_sidebar(ui);
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            match self.app_mode {
                AppMode::Fresh => self.render_welcome(ui),
                AppMode::Overview => self.render_overview(ui, ctx),
                AppMode::Sorting => self.render_sorting(ui, ctx),
            };
        });

        // Bottom progress bar
        if self.app_mode == AppMode::Sorting
            && !self.videos.is_empty()
        {
            egui::TopBottomPanel::bottom("progress_bar").show(ctx, |ui| {
                let total = self.videos.len();
                let done = self.current_video_index;
                let frac = done as f32 / total as f32;
                ui.add(
                    egui::ProgressBar::new(frac)
                        .desired_width(ui.available_width())
                        .text(format!("{}/{}", done, total)),
                );
            });
        }

        // Dialogs
        self.render_ffmpeg_dialog(ctx);
        self.render_completion_dialog(ctx);
        self.render_star_hint_dialog(ctx);

        // Process thumbnail queue (one per frame)
        self.process_thumbnail_queue(ctx);

        // Keyboard handling for sorting mode
        if self.app_mode == AppMode::Sorting {
            self.handle_keyboard_input(ctx);
        }
    }
}

impl VideoTaggerApp {
    // ========== Sidebar ==========

    fn render_sidebar(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.heading("控制面板");
            ui.separator();
        });

        ui.add_space(8.0);

        if ui.button("选择文件夹").clicked() {
            self.pick_folder();
        }

        if let Some(ref folder) = self.selected_folder {
            ui.label(format!("当前目录: {}", folder.display()));
        }

        ui.add_space(8.0);

        let btn_text = match self.app_mode {
            AppMode::Fresh => {
                if self.selected_folder.is_none() {
                    "请先选择文件夹"
                } else {
                    "开始总览"
                }
            }
            AppMode::Overview => "进入分拣",
            AppMode::Sorting => "退出分拣",
        };

        if self.app_mode == AppMode::Fresh && self.selected_folder.is_some() {
            if ui
                .add_enabled(self.ffmpeg_path.is_some(), egui::Button::new(btn_text))
                .clicked()
            {
                self.enter_overview();
            }
        } else if self.app_mode == AppMode::Overview {
            if ui
                .add_enabled(self.ffmpeg_path.is_some(), egui::Button::new(btn_text))
                .clicked()
            {
                self.enter_sorting();
            }
        } else if self.app_mode == AppMode::Sorting {
            if ui.button(btn_text).clicked() {
                self.exit_sorting();
            }
        } else {
            ui.add_enabled(false, egui::Button::new(btn_text));
        }

        ui.add_space(12.0);

        ui.horizontal(|ui| {
            ui.label("截图间隔(秒):");
            ui.add(
                egui::DragValue::new(&mut self.config.screenshot_interval)
                    .range(1.0..=300.0)
                    .speed(1.0),
            );
        });

        ui.add_space(8.0);
        ui.checkbox(&mut self.config.shift_lock, "覆盖文件名模式");

        if let Some(ref prog) = self.folder_progress {
            ui.add_space(8.0);
            ui.label(format!("识别码: {}", prog.identifier));
        }

        if self.app_mode == AppMode::Sorting {
            if let Some(video) = self.videos.get(self.current_video_index) {
                ui.add_space(8.0);
                ui.separator();
                ui.label(format!("视频: {}", video.filename));
                let dur = video.duration_secs.unwrap_or(0.0);
                if dur > 0.0 {
                    ui.label(format!("时长: {:.0}s", dur));
                }
                let time_range = format!(
                    "截图范围: {:.0}s - {:.0}s",
                    self.screenshot_start_sec,
                    self.screenshot_start_sec + self.screenshot_interval * 10.0
                );
                ui.label(time_range);
            }
        }
    }

    fn pick_folder(&mut self) {
        if let Some(path) = rfd::FileDialog::new().pick_folder() {
            self.selected_folder = Some(path);
        }
    }

    fn enter_overview(&mut self) {
        if let Some(ref folder) = self.selected_folder {
            self.videos = scanner::scan_videos(folder);
            self.overview_thumbnails.clear();
            self.thumbnail_queue.clear();
            self.thumbnail_loaded.clear();
            self.overview_search.clear();

            let id = config::generate_identifier();
            self.folder_progress = Some(progress::init_progress(&self.videos, &id));

            self.app_mode = AppMode::Overview;
        }
    }

    fn enter_sorting(&mut self) {
        if self.videos.is_empty() {
            return;
        }

        let start_idx = self
            .folder_progress
            .as_ref()
            .map(|p| p.last_processed)
            .unwrap_or(0);
        self.current_video_index = start_idx.min(self.videos.len().saturating_sub(1));
        self.screenshot_interval = self.config.screenshot_interval;
        self.screenshot_textures.clear();

        self.load_current_screenshots();

        self.current_labels.clear();
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.is_star_phase = false;
        self.is_starred = false;
        self.show_star_hint = false;
        self.playing_screenshot = None;

        self.app_mode = AppMode::Sorting;
    }

    fn exit_sorting(&mut self) {
        self.current_labels.clear();
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.is_star_phase = false;
        self.is_starred = false;
        self.app_mode = AppMode::Overview;
    }

    // ========== Welcome / Overview ==========

    fn render_welcome(&self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(100.0);
            ui.heading(RichText::new("视频标签分拣工具").size(32.0));
            ui.add_space(20.0);
            ui.label("请先在左侧边栏选择一个包含视频的文件夹");
            ui.add_space(10.0);
            ui.label("然后点击「开始总览」进入总览模式");
        });
    }

    fn render_overview(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context) {
        ui.horizontal(|ui| {
            ui.label("搜索:");
            ui.text_edit_singleline(&mut self.overview_search);
            if ui.button("x").clicked() {
                self.overview_search.clear();
            }
            ui.separator();
            ui.label("排序:");
            egui::ComboBox::from_id_salt("sort_mode")
                .selected_text(match self.overview_sort {
                    SortMode::Name => "文件名",
                    SortMode::Date => "日期",
                    SortMode::Size => "大小",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.overview_sort, SortMode::Name, "文件名");
                    ui.selectable_value(&mut self.overview_sort, SortMode::Date, "日期");
                    ui.selectable_value(&mut self.overview_sort, SortMode::Size, "大小");
                });
        });

        ui.separator();

        let search_lower = self.overview_search.to_lowercase();
        let filtered: Vec<usize> = self
            .videos
            .iter()
            .enumerate()
            .filter(|(_, v)| {
                search_lower.is_empty()
                    || v.filename.to_lowercase().contains(&search_lower)
            })
            .map(|(i, _)| i)
            .collect();

        let thumb_size = 160.0;
        let spacing = 8.0;
        let cols = (ui.available_width() / (thumb_size + spacing)).max(1.0) as usize;
        let total_rows = (filtered.len() + cols - 1) / cols;
        let row_height = thumb_size + 40.0 + spacing;

        egui::ScrollArea::vertical()
            .id_salt("overview_scroll")
            .show(ui, |ui| {
                let visible_rect = ui.clip_rect();
                let first_row = (visible_rect.top() / row_height).max(0.0) as usize;
                let last_row =
                    ((visible_rect.bottom() / row_height).ceil() as usize + 1).min(total_rows);

                ui.add_space(first_row as f32 * row_height);

                for row in first_row..last_row {
                    ui.horizontal(|ui| {
                        for col in 0..cols {
                            let idx = row * cols + col;
                            if idx >= filtered.len() {
                                break;
                            }
                            let video_idx = filtered[idx];
                            self.render_thumbnail_card(ui, video_idx, thumb_size);
                        }
                    });
                    ui.add_space(spacing);
                }

                let remaining = total_rows.saturating_sub(last_row);
                ui.add_space(remaining as f32 * row_height);
            });
    }

    fn render_thumbnail_card(
        &mut self,
        ui: &mut egui::Ui,
        video_idx: usize,
        thumb_size: f32,
    ) {
        let video = &self.videos[video_idx];
        let thumb_rect = egui::Frame::NONE
            .fill(Color32::from_gray(40))
            .stroke(egui::Stroke::new(1.0, Color32::from_gray(80)))
            .show(ui, |ui| {
                let (rect, response) = ui.allocate_exact_size(
                    Vec2::new(thumb_size, thumb_size),
                    egui::Sense::click(),
                );

                if let Some(texture) = self.overview_thumbnails.get(&video_idx) {
                    ui.put(
                        rect,
                        egui::Image::new(texture).fit_to_exact_size(rect.size()),
                    );
                } else {
                    ui.put(
                        rect,
                        egui::Label::new(
                            RichText::new("📹")
                                .size(40.0)
                                .color(Color32::from_gray(120)),
                        )
                        .selectable(false),
                    );
                    if !self.thumbnail_loaded.contains(&video_idx) {
                        self.thumbnail_queue.push_back(video_idx);
                        self.thumbnail_loaded.insert(video_idx);
                    }
                }

                response
            });

        let resp = thumb_rect.inner;

        let label = if video.filename.len() > 18 {
            format!("{}...", &video.filename[..15.min(video.filename.len())])
        } else {
            video.filename.clone()
        };
        ui.label(RichText::new(label).size(11.0));

        if resp.double_clicked() {
            self.independent_edit = Some(video_idx);
            self.current_video_index = video_idx;
            self.screenshot_textures.clear();
            self.load_current_screenshots();
            self.current_labels.clear();
            self.undo_stack.clear();
            self.redo_stack.clear();
            self.is_star_phase = false;
            self.is_starred = false;
            self.app_mode = AppMode::Sorting;
        }
    }

    fn process_thumbnail_queue(&mut self, ctx: &egui::Context) {
        if self.thumbnail_queue.is_empty() {
            return;
        }

        let video_idx = self.thumbnail_queue.pop_front().unwrap();
        if let Some(video) = self.videos.get(video_idx) {
            let thumb_path = config::cache_dir()
                .join("thumbs")
                .join(format!("thumb_{}.png", video_idx));
            let _ = std::fs::create_dir_all(thumb_path.parent().unwrap());

            if ffmpeg::extract_thumbnail(&video.path, &thumb_path).is_ok() {
                if let Ok(img_data) = std::fs::read(&thumb_path) {
                    if let Ok(img) = image::load_from_memory(&img_data) {
                        let rgba = img.to_rgba8();
                        let size = [rgba.width() as _, rgba.height() as _];
                        let pixels = rgba.into_raw();
                        let color_img =
                            egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
                        let texture = ctx.load_texture(
                            format!("thumb_{}", video_idx),
                            egui::ImageData::Color(color_img.into()),
                            egui::TextureOptions::LINEAR,
                        );
                        self.overview_thumbnails.insert(video_idx, texture);
                    }
                }
            }
        }
    }

    // ========== Sorting Mode ==========

    fn render_sorting(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let available = ui.available_size();
        let list_width = 160.0;
        let main_width = available.x - list_width - 8.0;

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.set_min_width(main_width);
                self.render_screenshot_area(ui, ctx);
                ui.add_space(8.0);
                self.render_label_preview_bar(ui);
                ui.add_space(8.0);
                self.render_tag_grid(ui);
            });

            ui.separator();

            egui::ScrollArea::vertical()
                .id_salt("video_list_scroll")
                .show(ui, |ui| {
                    ui.set_width(list_width);
                    self.render_video_list(ui);
                });
        });
    }

    fn load_current_screenshots(&mut self) {
        if self.videos.is_empty() {
            return;
        }

        let duration = self.videos[self.current_video_index].ensure_duration();
        self.screenshot_start_sec = 0.0;

        let paths = self.screenshot_cache.get_or_extract_screenshots(
            &self.videos[self.current_video_index].path,
            self.screenshot_start_sec,
            self.screenshot_interval,
            10,
            duration,
        );

        self.screenshot_paths = paths;
        self.screenshot_textures.clear();
    }

    fn advance_screenshots(&mut self, backward: bool) {
        if self.videos.is_empty() {
            return;
        }

        let duration = self.videos[self.current_video_index].ensure_duration();
        let step = self.screenshot_interval * 10.0;

        if backward {
            self.screenshot_start_sec = (self.screenshot_start_sec - step).max(0.0);
        } else {
            let max_start = (duration - self.screenshot_interval * 10.0).max(0.0);
            self.screenshot_start_sec = (self.screenshot_start_sec + step).min(max_start);
        }

        let paths = self.screenshot_cache.get_or_extract_screenshots(
            &self.videos[self.current_video_index].path,
            self.screenshot_start_sec,
            self.screenshot_interval,
            10,
            duration,
        );

        self.screenshot_paths = paths;
        self.screenshot_textures.clear();
    }

    fn render_screenshot_area(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.screenshot_paths.is_empty() {
            ui.label("加载截图中...");
            return;
        }

        let cols = 5;
        let rows = 2;
        let img_size = (ui.available_width() / cols as f32 - 12.0)
            .max(80.0)
            .min(220.0);

        // Load textures for screenshots
        for (idx, path) in self.screenshot_paths.iter().enumerate() {
            let tex_id = format!("scr_{}_{}", self.current_video_index, idx);
            if !self.screenshot_textures.contains_key(&tex_id) {
                if let Ok(data) = std::fs::read(path) {
                    if let Ok(img) = image::load_from_memory(&data) {
                        let rgba = img.to_rgba8();
                        let size = [rgba.width() as _, rgba.height() as _];
                        let pixels = rgba.into_raw();
                        let color_img =
                            egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
                        let texture = ctx.load_texture(
                            tex_id.clone(),
                            egui::ImageData::Color(color_img.into()),
                            egui::TextureOptions::LINEAR,
                        );
                        self.screenshot_textures.insert(tex_id, texture);
                    }
                }
            }
        }

        ui.vertical(|ui| {
            for row in 0..rows {
                ui.horizontal(|ui| {
                    for col in 0..cols {
                        let idx = row * cols + col;
                        if idx >= self.screenshot_paths.len() {
                            ui.add_space(img_size + 8.0);
                            continue;
                        }

                        let cell_size = Vec2::new(img_size, img_size * 9.0 / 16.0);
                        let (rect, response) =
                            ui.allocate_exact_size(cell_size, egui::Sense::click());

                        let is_playing = self.playing_screenshot == Some(idx);
                        let border_color = if is_playing {
                            Color32::YELLOW
                        } else if response.hovered() {
                            Color32::WHITE
                        } else {
                            Color32::from_gray(80)
                        };

                        ui.painter().rect_stroke(
                            rect,
                            0.0,
                            egui::Stroke::new(2.0, border_color),
                            StrokeKind::Middle,
                        );

                        let tex_id =
                            format!("scr_{}_{}", self.current_video_index, idx);
                        if let Some(tex) = self.screenshot_textures.get(&tex_id) {
                            ui.put(
                                rect,
                                egui::Image::new(tex).fit_to_exact_size(rect.size()),
                            );
                        }

                        let time_sec =
                            self.screenshot_start_sec + idx as f64 * self.screenshot_interval;
                        ui.painter().text(
                            rect.left_bottom() + egui::vec2(2.0, -2.0),
                            egui::Align2::LEFT_BOTTOM,
                            format!("{:.0}s", time_sec),
                            egui::FontId::proportional(10.0),
                            Color32::WHITE,
                        );

                        if response.clicked() {
                            let seek = self.screenshot_start_sec
                                + idx as f64 * self.screenshot_interval;
                            self.audio_player
                                .play_clip(&self.videos[self.current_video_index].path, seek);
                            self.playing_screenshot = Some(idx);
                        }
                    }
                });
                ui.add_space(4.0);
            }
        });

        if self.playing_screenshot.is_some() && !self.audio_player.is_playing() {
            self.playing_screenshot = None;
        }
    }

    fn render_label_preview_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("标签预览:");

            let labels = self.current_labels.clone();
            let mut remove_idx: Option<usize> = None;

            for (i, label) in labels.iter().enumerate() {
                let frame = egui::Frame::NONE
                    .fill(Color32::from_rgb(60, 100, 180))
                    .stroke(egui::Stroke::new(1.0, Color32::WHITE))
                    .inner_margin(4.0);

                frame.show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(label.clone());
                        if ui.small_button("x").clicked() {
                            remove_idx = Some(i);
                        }
                    });
                });
            }

            if let Some(idx) = remove_idx {
                self.undo_stack.push(self.current_labels.clone());
                self.redo_stack.clear();
                self.current_labels.remove(idx);
            }
        });
    }

    fn render_tag_grid(&mut self, ui: &mut egui::Ui) {
        let tag_names = self.tag_library.sorted_names();
        let cols = 9;
        let rows = 3;

        ui.vertical(|ui| {
            ui.separator();

            for row in 0..rows {
                ui.horizontal(|ui| {
                    for col in 0..cols {
                        let idx = row * cols + col;
                        let is_selected = self.tag_row == row && self.tag_col == col;

                        if idx < tag_names.len() {
                            let tag = &tag_names[idx];
                            let fill = if is_selected {
                                Color32::from_rgb(100, 140, 220)
                            } else {
                                Color32::from_gray(50)
                            };

                            let btn = egui::Button::new(
                                RichText::new(format!("{} {}", col + 1, tag)).size(12.0),
                            )
                            .fill(fill)
                            .min_size(Vec2::new(60.0, 24.0));

                            let resp = ui.add(btn);

                            if resp.clicked() {
                                self.add_label(tag.clone());
                            }
                            if resp.secondary_clicked() {
                                self.tag_library.remove_tag(tag);
                                self.tag_library.save();
                            }
                        } else if idx == tag_names.len() && !self.editing_new_tag {
                            let btn = egui::Button::new(RichText::new("+").size(14.0))
                                .fill(Color32::from_gray(40))
                                .min_size(Vec2::new(30.0, 24.0));

                            if ui.add(btn).clicked() {
                                self.editing_new_tag = true;
                                self.new_tag_text.clear();
                            }
                        } else if self.editing_new_tag && idx == tag_names.len() {
                            let resp = ui.add(
                                egui::TextEdit::singleline(&mut self.new_tag_text)
                                    .desired_width(80.0),
                            );

                            if resp.lost_focus() {
                                self.finish_new_tag();
                            }
                        } else {
                            ui.add_sized(Vec2::new(30.0, 24.0), egui::Label::new(""));
                        }
                    }
                });
            }
        });
    }

    fn render_video_list(&mut self, ui: &mut egui::Ui) {
        ui.heading("视频列表");
        ui.separator();

        let item_height = 24.0;
        let total = self.videos.len();
        let visible_rect = ui.clip_rect();
        let first = (visible_rect.top() / item_height).max(0.0) as usize;
        let last = ((visible_rect.bottom() / item_height).ceil() as usize + 1).min(total);

        ui.add_space(first as f32 * item_height);

        let identifier = self
            .folder_progress
            .as_ref()
            .map(|p| p.identifier.clone())
            .unwrap_or_default();

        for i in first..last {
            let video = &self.videos[i];
            let is_current = i == self.current_video_index;
            let processed = video.filename.contains(&format!("[{}]", identifier));

            let text = RichText::new(format!(
                "{} {}",
                if is_current { "▶" } else { " " },
                video.filename
            ))
            .size(12.0);

            let fill = if is_current {
                Color32::from_rgb(60, 100, 180)
            } else if processed {
                Color32::from_rgb(40, 80, 40)
            } else {
                Color32::TRANSPARENT
            };

            let resp = ui.add_sized(
                Vec2::new(ui.available_width(), item_height),
                egui::Button::new(text).fill(fill),
            );

            if resp.clicked() {
                self.current_video_index = i;
                self.screenshot_textures.clear();
                self.load_current_screenshots();
                self.current_labels.clear();
                self.undo_stack.clear();
                self.redo_stack.clear();
                self.is_star_phase = false;
                self.is_starred = false;
            }
        }

        let remaining = total.saturating_sub(last);
        ui.add_space(remaining as f32 * item_height);
    }

    // ========== Label operations ==========

    fn add_label(&mut self, label: String) {
        self.undo_stack.push(self.current_labels.clone());
        self.redo_stack.clear();
        self.current_labels.push(label);
    }

    fn undo_label(&mut self) {
        if let Some(prev) = self.undo_stack.pop() {
            self.redo_stack.push(self.current_labels.clone());
            self.current_labels = prev;
        }
    }

    fn redo_label(&mut self) {
        if let Some(next) = self.redo_stack.pop() {
            self.undo_stack.push(self.current_labels.clone());
            self.current_labels = next;
        }
    }

    fn finish_new_tag(&mut self) {
        if !self.new_tag_text.is_empty() {
            self.tag_library.add_tag(&self.new_tag_text);
            self.tag_library.save();
            self.new_tag_text.clear();
        }
        self.editing_new_tag = false;
    }

    fn confirm_labels_and_enter_star(&mut self) {
        if self.is_star_phase {
            self.finalize_current_video();
            return;
        }

        self.is_star_phase = true;
        self.is_starred = false;
        self.show_star_hint = true;
    }

    fn finalize_current_video(&mut self) {
        if self.videos.is_empty() {
            return;
        }

        let video = &self.videos[self.current_video_index];
        let extension = video.extension.clone();
        let original_basename = video.filename.clone();

        if let Some(ref prog) = self.folder_progress {
            let overwrite = self.config.shift_lock;
            let new_name = config::format_video_name(
                &prog.identifier,
                self.current_video_index,
                prog.digit_count,
                &self.current_labels,
                self.is_starred,
                &original_basename,
                &extension,
                overwrite,
            );

            let parent = video.path.parent().unwrap_or(std::path::Path::new("."));
            let new_path = parent.join(&new_name);

            if !self.current_labels.is_empty() || self.is_starred {
                let mut final_path = new_path.clone();
                scanner::resolve_name_conflict(&mut final_path);
                let _ = std::fs::rename(&video.path, &final_path);
            }

            if !self.current_labels.is_empty() {
                self.tag_library.record_usage(&self.current_labels);
                self.tag_library.save();
            }
        }

        if let Some(ref mut prog) = self.folder_progress {
            prog.last_processed = self.current_video_index + 1;
            progress::save_progress(self.selected_folder.as_ref().unwrap(), prog);
        }

        let next_idx = self.current_video_index + 1;
        if next_idx >= self.videos.len() {
            self.app_mode = AppMode::Overview;
            self.show_completion = true;
            self.independent_edit = None;
            return;
        }

        if self.independent_edit.is_some() {
            self.app_mode = AppMode::Overview;
            self.independent_edit = None;
            return;
        }

        self.current_video_index = next_idx;
        self.screenshot_textures.clear();
        self.load_current_screenshots();
        self.current_labels.clear();
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.is_star_phase = false;
        self.is_starred = false;
    }

    // ========== Dialogs ==========

    fn render_ffmpeg_dialog(&mut self, ctx: &egui::Context) {
        if !self.ffmpeg_dialog_open {
            return;
        }

        egui::Window::new("FFmpeg 设置")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                if self.ffmpeg_path.is_some() {
                    ui.label(format!(
                        "已找到: {}",
                        self.ffmpeg_path.as_ref().unwrap().display()
                    ));
                    if ui.button("重新扫描").clicked() {
                        self.ffmpeg_path = ffmpeg::find_ffmpeg();
                    }
                } else {
                    ui.label("未找到 ffmpeg，请安装或手动指定路径:");
                    ui.horizontal(|ui| {
                        ui.label("路径:");
                        ui.text_edit_singleline(&mut self.ffmpeg_custom_path);
                    });
                    if ui.button("浏览").clicked() {
                        if let Some(path) = rfd::FileDialog::new().pick_file() {
                            self.ffmpeg_custom_path = path.to_string_lossy().to_string();
                        }
                    }
                    if ui.button("确认").clicked() {
                        let p = std::path::PathBuf::from(&self.ffmpeg_custom_path);
                        if p.exists() {
                            self.ffmpeg_path = Some(p);
                            self.ffmpeg_error = false;
                        }
                    }
                }

                ui.separator();
                if ui.button("关闭").clicked() {
                    self.ffmpeg_dialog_open = false;
                }
            });
    }

    fn render_completion_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_completion {
            return;
        }

        egui::Window::new("完成")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label("全部分拣完成！");
                if ui.button("确定").clicked() {
                    self.show_completion = false;
                }
            });
    }

    fn render_star_hint_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_star_hint {
            return;
        }

        egui::Window::new("打分")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .auto_sized()
            .show(ctx, |ui| {
                ui.label(if self.is_starred {
                    "★ 已打星 | 任意键切换 | Enter 确认"
                } else {
                    "☆ 未打星 | 任意键打星 | Enter 跳过"
                });
                if ui.button("关闭提示").clicked() {
                    self.show_star_hint = false;
                }
            });
    }

    fn handle_keyboard_input(&mut self, ctx: &egui::Context) {
        let input = ctx.input(|i| i.clone());

        if self.editing_new_tag {
            if input.key_pressed(egui::Key::Enter) {
                self.finish_new_tag();
            }
            return;
        }

        if self.is_star_phase {
            if input.key_pressed(egui::Key::Enter) {
                self.finalize_current_video();
                return;
            }

            let any_key = input.events.iter().any(|e| {
                matches!(e, egui::Event::Key { pressed: true, key, .. }
                    if *key != egui::Key::Enter)
            });
            if any_key {
                self.is_starred = !self.is_starred;
            }
            return;
        }

        // Normal label mode
        if input.key_pressed(egui::Key::R) && !input.modifiers.shift {
            self.advance_screenshots(false);
        }
        if input.key_pressed(egui::Key::R) && input.modifiers.shift {
            self.advance_screenshots(true);
        }

        if input.key_pressed(egui::Key::Z) && !input.modifiers.ctrl {
            self.undo_label();
        }
        if input.key_pressed(egui::Key::Y) && !input.modifiers.ctrl {
            self.redo_label();
        }

        if input.key_pressed(egui::Key::Enter) {
            let shift_held = input.modifiers.shift;
            if shift_held {
                let saved = self.config.shift_lock;
                self.config.shift_lock = true;
                self.confirm_labels_and_enter_star();
                self.config.shift_lock = saved;
            } else {
                self.confirm_labels_and_enter_star();
            }
            return;
        }

        if input.key_pressed(egui::Key::ArrowUp) {
            self.tag_row = self.tag_row.saturating_sub(1);
        }
        if input.key_pressed(egui::Key::ArrowDown) {
            self.tag_row = (self.tag_row + 1).min(2);
        }
        if input.key_pressed(egui::Key::ArrowLeft) {
            self.tag_col = self.tag_col.saturating_sub(1);
        }
        if input.key_pressed(egui::Key::ArrowRight) {
            self.tag_col = (self.tag_col + 1).min(8);
        }

        let tag_names = self.tag_library.sorted_names();
        let num_keys = [
            egui::Key::Num1,
            egui::Key::Num2,
            egui::Key::Num3,
            egui::Key::Num4,
            egui::Key::Num5,
            egui::Key::Num6,
            egui::Key::Num7,
            egui::Key::Num8,
            egui::Key::Num9,
        ];

        for (n, key) in num_keys.iter().enumerate() {
            if input.key_pressed(*key) {
                let tag_idx = self.tag_row * 9 + n;
                if tag_idx < tag_names.len() {
                    self.add_label(tag_names[tag_idx].clone());
                }
            }
        }
    }
}
