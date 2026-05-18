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
use crate::tags::{TagLibrary, MAX_TAG_CATEGORIES, MAX_TAGS_PER_CATEGORY, STAR_CATEGORY_NAME};

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

    current_video_index: usize,
    screenshot_interval: f64,
    screenshot_start_sec: f64,
    screenshot_paths: Vec<PathBuf>,
    screenshot_textures: HashMap<String, egui::TextureHandle>,
    selected_screenshot_index: usize,

    current_labels: Vec<String>,
    undo_stack: Vec<Vec<String>>,
    redo_stack: Vec<Vec<String>>,
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
}

impl Default for VideoTaggerApp {
    fn default() -> Self {
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

            current_video_index: 0,
            screenshot_interval: 10.0,
            screenshot_start_sec: 0.0,
            screenshot_paths: Vec::new(),
            screenshot_textures: HashMap::new(),
            selected_screenshot_index: 0,

            current_labels: Vec::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
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
                self.ffmpeg_dialog_open = true;
                self.ffmpeg_error = true;
            }
        }

        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.heading("Video Tagger");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("FFmpeg").clicked() { self.ffmpeg_dialog_open = true; }
                });
            });
        });

        egui::SidePanel::left("sidebar")
            .min_width(210.0)
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
                let done = self.current_video_index;
                let frac = done as f32 / total as f32;
                ui.add(egui::ProgressBar::new(frac).desired_width(ui.available_width()).text(format!("{}/{}", done, total)));
            });
        }

        self.render_ffmpeg_dialog(ctx);
        self.render_completion_dialog(ctx);
        self.process_thumbnail_queue(ctx);

        if self.app_mode == AppMode::Sorting {
            self.handle_keyboard_input(ctx);
        }
    }
}

impl VideoTaggerApp {
    fn tag_category_count(&self) -> usize { self.tag_library.category_count() }
    fn star_category_index(&self) -> usize { self.tag_category_count() }
    fn total_flow_categories(&self) -> usize { self.tag_category_count() + 1 }
    fn in_star_category(&self) -> bool { self.active_category_index >= self.star_category_index() }

    fn visible_tags_for_active_category(&self) -> Vec<String> {
        if self.in_star_category() {
            vec!["不星标".to_string(), "★ 星标".to_string()]
        } else {
            self.tag_library.category_names_for_display(self.active_category_index, self.config.tag_position_lock)
        }
    }

    fn reset_video_edit_state(&mut self) {
        self.current_labels.clear();
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.is_starred = false;
        self.pending_overwrite_once = false;
        self.active_category_index = 0;
        self.selected_tag_index = 0;
        self.selected_screenshot_index = 0;
        self.playing_screenshot = None;
        self.audio_player.stop();
    }

    fn render_sidebar(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.heading("控制面板");
            ui.separator();
        });
        ui.add_space(8.0);

        if ui.button("选择文件夹").clicked() { self.pick_folder(); }
        if let Some(ref folder) = self.selected_folder { ui.label(format!("当前目录: {}", folder.display())); }
        ui.add_space(8.0);

        let btn_text = match self.app_mode {
            AppMode::Fresh => if self.selected_folder.is_none() { "请先选择文件夹" } else { "开始总览" },
            AppMode::Overview => "进入分拣",
            AppMode::Sorting => "退出分拣",
        };
        let enabled = self.ffmpeg_path.is_some() && !(self.app_mode == AppMode::Fresh && self.selected_folder.is_none());
        if ui.add_enabled(enabled, egui::Button::new(btn_text)).clicked() {
            match self.app_mode {
                AppMode::Fresh => self.enter_overview(),
                AppMode::Overview => self.enter_sorting(),
                AppMode::Sorting => self.exit_sorting(),
            }
        }

        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.label("截图间隔(秒):");
            if ui.add(egui::DragValue::new(&mut self.config.screenshot_interval).range(1.0..=300.0).speed(1.0)).changed() {
                self.config.save();
            }
        });
        if ui.checkbox(&mut self.config.shift_lock, "覆盖文件名模式").changed() { self.config.save(); }
        if ui.checkbox(&mut self.config.tag_position_lock, "锁定标签位置").changed() { self.config.save(); }

        ui.add_space(12.0);
        ui.separator();
        ui.label(RichText::new("标签类别").strong());
        ui.label(RichText::new(format!("最多 {} 个类别，每类最多 {} 个标签；星标固定在最后", MAX_TAG_CATEGORIES, MAX_TAGS_PER_CATEGORY)).small());

        let mut remove_category: Option<usize> = None;
        for (idx, category) in self.tag_library.categories().iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(format!("{}.", idx + 1));
                ui.label(&category.name);
                if ui.small_button("删除").clicked() { remove_category = Some(idx); }
            });
        }
        if let Some(idx) = remove_category {
            self.tag_library.remove_category(idx);
            self.tag_library.save();
            if self.active_category_index > self.tag_category_count() { self.active_category_index = self.star_category_index(); }
            self.selected_tag_index = 0;
        }

        if self.editing_new_category {
            ui.horizontal(|ui| {
                let resp = ui.add(egui::TextEdit::singleline(&mut self.new_category_text).hint_text("类别名").desired_width(90.0));
                if ui.button("确定").clicked() || (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                    if self.tag_library.add_category(&self.new_category_text) {
                        self.tag_library.save();
                    }
                    self.new_category_text.clear();
                    self.editing_new_category = false;
                }
                if ui.button("取消").clicked() { self.editing_new_category = false; self.new_category_text.clear(); }
            });
        } else if self.tag_category_count() < MAX_TAG_CATEGORIES {
            if ui.button("添加标签类别").clicked() {
                self.editing_new_category = true;
                self.new_category_text.clear();
            }
        }

        if let Some(ref prog) = self.folder_progress {
            ui.add_space(8.0);
            ui.separator();
            ui.label(format!("识别码: {}", prog.identifier));
        }

        if self.app_mode == AppMode::Sorting {
            if let Some(video) = self.videos.get(self.current_video_index) {
                ui.add_space(8.0);
                ui.separator();
                ui.label(format!("视频: {}", video.filename));
                if let Some(dur) = video.duration_secs { ui.label(format!("时长: {:.0}s", dur)); }
                ui.label(format!("截图范围: {:.0}s - {:.0}s", self.screenshot_start_sec, self.screenshot_start_sec + self.screenshot_interval * 10.0));
                ui.separator();
                ui.label("快捷键:");
                ui.label("Q/E 前后截图组");
                ui.label("WASD 选图，X 播放音频");
                ui.label("←/→ 或 1-9 选标签");
                ui.label("Space 确认当前标签");
                ui.label("Delete 按顺序删除标签");
            }
        }
    }

    fn pick_folder(&mut self) {
        if let Some(path) = rfd::FileDialog::new().pick_folder() { self.selected_folder = Some(path); }
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
        if self.videos.is_empty() { return; }
        let start_idx = self.folder_progress.as_ref().map(|p| p.last_processed).unwrap_or(0);
        self.current_video_index = start_idx.min(self.videos.len().saturating_sub(1));
        self.screenshot_interval = self.config.screenshot_interval;
        self.screenshot_textures.clear();
        self.load_current_screenshots();
        self.reset_video_edit_state();
        self.app_mode = AppMode::Sorting;
    }

    fn exit_sorting(&mut self) {
        self.reset_video_edit_state();
        self.app_mode = AppMode::Overview;
    }

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
            if ui.button("x").clicked() { self.overview_search.clear(); }
            ui.separator();
            ui.label("排序:");
            egui::ComboBox::from_id_salt("sort_mode").selected_text(match self.overview_sort {
                SortMode::Name => "文件名",
                SortMode::Date => "日期",
                SortMode::Size => "大小",
            }).show_ui(ui, |ui| {
                ui.selectable_value(&mut self.overview_sort, SortMode::Name, "文件名");
                ui.selectable_value(&mut self.overview_sort, SortMode::Date, "日期");
                ui.selectable_value(&mut self.overview_sort, SortMode::Size, "大小");
            });
        });
        ui.separator();

        let search_lower = self.overview_search.to_lowercase();
        let filtered: Vec<usize> = self.videos.iter().enumerate()
            .filter(|(_, v)| search_lower.is_empty() || v.filename.to_lowercase().contains(&search_lower))
            .map(|(i, _)| i).collect();

        let thumb_size = 160.0;
        let spacing = 8.0;
        let cols = (ui.available_width() / (thumb_size + spacing)).max(1.0) as usize;
        let total_rows = (filtered.len() + cols - 1) / cols;
        let row_height = thumb_size + 40.0 + spacing;

        egui::ScrollArea::vertical().id_salt("overview_scroll").show(ui, |ui| {
            for row in 0..total_rows {
                ui.horizontal(|ui| {
                    for col in 0..cols {
                        let idx = row * cols + col;
                        if idx >= filtered.len() { break; }
                        self.render_thumbnail_card(ui, filtered[idx], thumb_size);
                    }
                });
                ui.add_space(spacing);
            }
            let _ = row_height;
        });
    }

    fn render_thumbnail_card(&mut self, ui: &mut egui::Ui, video_idx: usize, thumb_size: f32) {
        let video = &self.videos[video_idx];
        let thumb_rect = egui::Frame::NONE.fill(Color32::from_gray(40)).stroke(egui::Stroke::new(1.0, Color32::from_gray(80))).show(ui, |ui| {
            let (rect, response) = ui.allocate_exact_size(Vec2::new(thumb_size, thumb_size), egui::Sense::click());
            if let Some(texture) = self.overview_thumbnails.get(&video_idx) {
                ui.put(rect, egui::Image::new(texture).fit_to_exact_size(rect.size()));
            } else {
                ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "视频", egui::FontId::proportional(20.0), Color32::from_gray(120));
                if !self.thumbnail_loaded.contains(&video_idx) {
                    self.thumbnail_queue.push_back(video_idx);
                    self.thumbnail_loaded.insert(video_idx);
                }
            }
            response
        });
        let resp = thumb_rect.inner;
        let label = if video.filename.chars().count() > 18 { format!("{}...", video.filename.chars().take(15).collect::<String>()) } else { video.filename.clone() };
        ui.label(RichText::new(label).size(11.0));
        if resp.double_clicked() {
            self.independent_edit = Some(video_idx);
            self.current_video_index = video_idx;
            self.screenshot_textures.clear();
            self.load_current_screenshots();
            self.reset_video_edit_state();
            self.app_mode = AppMode::Sorting;
        }
    }

    fn process_thumbnail_queue(&mut self, ctx: &egui::Context) {
        if self.thumbnail_queue.is_empty() { return; }
        let video_idx = self.thumbnail_queue.pop_front().unwrap();
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
                    }
                }
            }
        }
    }

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
            egui::ScrollArea::vertical().id_salt("video_list_scroll").show(ui, |ui| {
                ui.set_width(list_width);
                self.render_video_list(ui);
            });
        });
    }

    fn load_current_screenshots(&mut self) {
        if self.videos.is_empty() { return; }
        let duration = self.videos[self.current_video_index].ensure_duration();
        self.screenshot_start_sec = 0.0;
        let paths = self.screenshot_cache.get_or_extract_screenshots(&self.videos[self.current_video_index].path, self.screenshot_start_sec, self.screenshot_interval, 10, duration);
        self.screenshot_paths = paths;
        self.screenshot_textures.clear();
        self.selected_screenshot_index = 0;
    }

    fn advance_screenshots(&mut self, backward: bool) {
        if self.videos.is_empty() { return; }
        let duration = self.videos[self.current_video_index].ensure_duration();
        let step = self.screenshot_interval * 10.0;
        if backward {
            self.screenshot_start_sec = (self.screenshot_start_sec - step).max(0.0);
        } else {
            let max_start = (duration - self.screenshot_interval * 10.0).max(0.0);
            self.screenshot_start_sec = (self.screenshot_start_sec + step).min(max_start);
        }
        let paths = self.screenshot_cache.get_or_extract_screenshots(&self.videos[self.current_video_index].path, self.screenshot_start_sec, self.screenshot_interval, 10, duration);
        self.screenshot_paths = paths;
        self.screenshot_textures.clear();
        self.selected_screenshot_index = self.selected_screenshot_index.min(self.screenshot_paths.len().saturating_sub(1));
    }

    fn play_selected_screenshot_audio(&mut self) {
        if self.videos.is_empty() || self.screenshot_paths.is_empty() { return; }
        let idx = self.selected_screenshot_index.min(self.screenshot_paths.len().saturating_sub(1));
        let seek = self.screenshot_start_sec + idx as f64 * self.screenshot_interval;
        self.audio_player.play_clip(&self.videos[self.current_video_index].path, seek);
        self.playing_screenshot = Some(idx);
    }

    fn move_selected_screenshot(&mut self, dx: isize, dy: isize) {
        if self.screenshot_paths.is_empty() { return; }
        let cols = 5isize;
        let len = self.screenshot_paths.len() as isize;
        let cur = self.selected_screenshot_index as isize;
        let row = cur / cols;
        let col = cur % cols;
        let next = ((row + dy).clamp(0, 1) * cols + (col + dx).clamp(0, cols - 1)).clamp(0, len - 1);
        self.selected_screenshot_index = next as usize;
    }

    fn render_screenshot_area(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.screenshot_paths.is_empty() { ui.label("加载截图中..."); return; }
        let cols = 5;
        let rows = 2;
        let img_size = (ui.available_width() / cols as f32 - 12.0).max(80.0).min(220.0);

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

        ui.vertical(|ui| {
            for row in 0..rows {
                ui.horizontal(|ui| {
                    for col in 0..cols {
                        let idx = row * cols + col;
                        if idx >= self.screenshot_paths.len() { ui.add_space(img_size + 8.0); continue; }
                        let cell_size = Vec2::new(img_size, img_size * 9.0 / 16.0);
                        let (rect, response) = ui.allocate_exact_size(cell_size, egui::Sense::click());
                        if response.clicked() { self.selected_screenshot_index = idx; }
                        let is_selected = self.selected_screenshot_index == idx;
                        let is_playing = self.playing_screenshot == Some(idx);
                        let border_color = if is_playing { Color32::YELLOW } else if is_selected { Color32::LIGHT_BLUE } else if response.hovered() { Color32::WHITE } else { Color32::from_gray(80) };
                        ui.painter().rect_stroke(rect, 0.0, egui::Stroke::new(2.0, border_color), StrokeKind::Middle);
                        let tex_id = format!("scr_{}_{}_{}", self.current_video_index, (self.screenshot_start_sec * 10.0) as u64, idx);
                        if let Some(tex) = self.screenshot_textures.get(&tex_id) {
                            ui.put(rect, egui::Image::new(tex).fit_to_exact_size(rect.size()));
                        }
                        let time_sec = self.screenshot_start_sec + idx as f64 * self.screenshot_interval;
                        ui.painter().text(rect.left_bottom() + egui::vec2(2.0, -2.0), egui::Align2::LEFT_BOTTOM, format!("{:.0}s", time_sec), egui::FontId::proportional(10.0), Color32::WHITE);
                    }
                });
                ui.add_space(4.0);
            }
        });
    }

    fn render_label_preview_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label("标签预览:");
            for label in &self.current_labels {
                egui::Frame::NONE.fill(Color32::from_rgb(60, 100, 180)).stroke(egui::Stroke::new(1.0, Color32::WHITE)).inner_margin(4.0).show(ui, |ui| { ui.label(label); });
            }
            if self.is_starred { ui.label(RichText::new("★ 星标").color(Color32::YELLOW)); }
        });
    }

    fn render_tag_grid(&mut self, ui: &mut egui::Ui) {
        let tags = self.visible_tags_for_active_category();
        self.selected_tag_index = self.selected_tag_index.min(tags.len().saturating_sub(1));
        ui.vertical(|ui| {
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                for idx in 0..self.total_flow_categories() {
                    let selected = idx == self.active_category_index;
                    let name = if idx == self.star_category_index() {
                        STAR_CATEGORY_NAME.to_string()
                    } else {
                        self.tag_library.categories().get(idx).map(|c| c.name.clone()).unwrap_or_else(|| format!("类别{}", idx + 1))
                    };
                    let text = if selected { format!("> {}", name) } else { name };
                    ui.label(RichText::new(text).color(if selected { Color32::LIGHT_BLUE } else { Color32::from_gray(160) }));
                    ui.add_space(8.0);
                }
            });
            ui.label(RichText::new("←/→ 选择标签，1-9 快速选择并确认，Space 确认，Delete 删除上一个已选标签").small().color(Color32::from_gray(140)));
            ui.add_space(6.0);

            ui.horizontal_wrapped(|ui| {
                for idx in 0..MAX_TAGS_PER_CATEGORY {
                    if idx < tags.len() {
                        let tag = &tags[idx];
                        let is_selected = idx == self.selected_tag_index;
                        let fill = if is_selected { Color32::from_rgb(100, 140, 220) } else { Color32::from_gray(50) };
                        let btn = egui::Button::new(RichText::new(format!("{} {}", idx + 1, tag)).size(12.0)).fill(fill).min_size(Vec2::new(82.0, 26.0));
                        let resp = ui.add(btn);
                        if resp.clicked() { self.selected_tag_index = idx; }
                        if resp.double_clicked() { self.confirm_selected_tag(); }
                        if resp.secondary_clicked() && !self.in_star_category() {
                            self.tag_library.remove_tag_from_category(self.active_category_index, tag);
                            self.tag_library.save();
                        }
                    } else if !self.in_star_category() && idx == tags.len() && !self.editing_new_tag {
                        if ui.add(egui::Button::new("+ 新标签").min_size(Vec2::new(82.0, 26.0))).clicked() {
                            self.editing_new_tag = true;
                            self.new_tag_text.clear();
                        }
                    } else if !self.in_star_category() && self.editing_new_tag && idx == tags.len() {
                        let resp = ui.add(egui::TextEdit::singleline(&mut self.new_tag_text).hint_text("标签名").desired_width(90.0));
                        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) { self.finish_new_tag(); }
                    } else {
                        ui.add_sized(Vec2::new(82.0, 26.0), egui::Label::new(""));
                    }
                }
            });
        });
    }

    fn render_video_list(&mut self, ui: &mut egui::Ui) {
        ui.heading("视频列表");
        ui.separator();
        let identifier = self.folder_progress.as_ref().map(|p| p.identifier.clone()).unwrap_or_default();
        let mut clicked: Option<usize> = None;
        for (i, video) in self.videos.iter().enumerate() {
            let is_current = i == self.current_video_index;
            let processed = video.filename.contains(&format!("[{}]", identifier));
            let text = RichText::new(format!("{} {}", if is_current { ">" } else { " " }, video.filename)).size(12.0);
            let fill = if is_current { Color32::from_rgb(60, 100, 180) } else if processed { Color32::from_rgb(40, 80, 40) } else { Color32::TRANSPARENT };
            if ui.add_sized(Vec2::new(ui.available_width(), 24.0), egui::Button::new(text).fill(fill)).clicked() { clicked = Some(i); }
        }
        if let Some(i) = clicked {
            self.current_video_index = i;
            self.screenshot_textures.clear();
            self.load_current_screenshots();
            self.reset_video_edit_state();
        }
    }

    fn add_label(&mut self, label: String) {
        if label.trim().is_empty() { return; }
        self.undo_stack.push(self.current_labels.clone());
        self.redo_stack.clear();
        self.current_labels.push(label);
    }

    fn undo_label(&mut self) {
        if let Some(prev) = self.undo_stack.pop() {
            self.redo_stack.push(self.current_labels.clone());
            self.current_labels = prev;
        } else {
            self.current_labels.pop();
        }
    }

    fn redo_label(&mut self) {
        if let Some(next) = self.redo_stack.pop() {
            self.undo_stack.push(self.current_labels.clone());
            self.current_labels = next;
        }
    }

    fn finish_new_tag(&mut self) {
        if !self.new_tag_text.trim().is_empty() && !self.in_star_category() {
            if self.tag_library.add_tag_to_category(self.active_category_index, &self.new_tag_text) {
                self.tag_library.save();
            }
            self.new_tag_text.clear();
        }
        self.editing_new_tag = false;
    }

    fn confirm_selected_tag(&mut self) {
        let tags = self.visible_tags_for_active_category();
        if tags.is_empty() && !self.in_star_category() {
            self.advance_category_or_finalize();
            return;
        }
        if self.in_star_category() {
            self.is_starred = self.selected_tag_index == 1;
            self.finalize_current_video();
            return;
        }
        if let Some(label) = tags.get(self.selected_tag_index).cloned() {
            self.add_label(label);
        }
        self.advance_category_or_finalize();
    }

    fn advance_category_or_finalize(&mut self) {
        if self.active_category_index + 1 < self.total_flow_categories() {
            self.active_category_index += 1;
            self.selected_tag_index = 0;
        } else {
            self.finalize_current_video();
        }
    }

    fn finalize_current_video(&mut self) {
        if self.videos.is_empty() { return; }
        let video = self.videos[self.current_video_index].clone();
        if let Some(ref prog) = self.folder_progress {
            let overwrite = self.config.shift_lock || self.pending_overwrite_once;
            let new_name = config::format_video_name(&prog.identifier, self.current_video_index, prog.digit_count, &self.current_labels, self.is_starred, &video.filename, &video.extension, overwrite);
            let parent = video.path.parent().unwrap_or(std::path::Path::new("."));
            let mut final_path = parent.join(&new_name);
            if !self.current_labels.is_empty() || self.is_starred {
                scanner::resolve_name_conflict(&mut final_path);
                if std::fs::rename(&video.path, &final_path).is_ok() {
                    if let Some(updated) = self.videos.get_mut(self.current_video_index) {
                        updated.path = final_path.clone();
                        updated.filename = final_path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or(new_name);
                        updated.extension = final_path.extension().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| video.extension.clone());
                    }
                }
            }
            if !self.current_labels.is_empty() {
                self.tag_library.record_usage(&self.current_labels);
                self.tag_library.save();
            }
        }
        if let Some(ref mut prog) = self.folder_progress {
            prog.last_processed = self.current_video_index + 1;
            if let Some(ref folder) = self.selected_folder { progress::save_progress(folder, prog); }
        }

        if self.independent_edit.is_some() {
            self.app_mode = AppMode::Overview;
            self.independent_edit = None;
            self.reset_video_edit_state();
            return;
        }
        let next_idx = self.current_video_index + 1;
        if next_idx >= self.videos.len() {
            self.app_mode = AppMode::Overview;
            self.show_completion = true;
            self.reset_video_edit_state();
            return;
        }
        self.current_video_index = next_idx;
        self.screenshot_textures.clear();
        self.load_current_screenshots();
        self.reset_video_edit_state();
    }

    fn render_ffmpeg_dialog(&mut self, ctx: &egui::Context) {
        if !self.ffmpeg_dialog_open { return; }
        egui::Window::new("FFmpeg 设置").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            if let Some(ref path) = self.ffmpeg_path {
                ui.label(format!("已找到: {}", path.display()));
                if ui.button("重新扫描").clicked() { self.ffmpeg_path = ffmpeg::find_ffmpeg(); }
            } else {
                ui.label("未找到 ffmpeg，请安装或手动指定路径:");
                ui.horizontal(|ui| { ui.label("路径:"); ui.text_edit_singleline(&mut self.ffmpeg_custom_path); });
                if ui.button("浏览").clicked() { if let Some(path) = rfd::FileDialog::new().pick_file() { self.ffmpeg_custom_path = path.to_string_lossy().to_string(); } }
                if ui.button("确认").clicked() {
                    let p = std::path::PathBuf::from(&self.ffmpeg_custom_path);
                    if p.exists() { self.ffmpeg_path = Some(p); self.ffmpeg_error = false; }
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

    fn handle_keyboard_input(&mut self, ctx: &egui::Context) {
        let input = ctx.input(|i| i.clone());
        if self.editing_new_tag {
            if input.key_pressed(egui::Key::Enter) { self.finish_new_tag(); }
            return;
        }
        if self.editing_new_category { return; }

        if input.key_pressed(egui::Key::Q) { self.advance_screenshots(true); }
        if input.key_pressed(egui::Key::E) { self.advance_screenshots(false); }
        if input.key_pressed(egui::Key::W) { self.move_selected_screenshot(0, -1); }
        if input.key_pressed(egui::Key::S) { self.move_selected_screenshot(0, 1); }
        if input.key_pressed(egui::Key::A) { self.move_selected_screenshot(-1, 0); }
        if input.key_pressed(egui::Key::D) { self.move_selected_screenshot(1, 0); }
        if input.key_pressed(egui::Key::X) { self.play_selected_screenshot_audio(); }

        if input.key_pressed(egui::Key::Delete) { self.undo_label(); }
        if input.key_pressed(egui::Key::Z) && input.modifiers.ctrl { self.undo_label(); }
        if input.key_pressed(egui::Key::Y) && input.modifiers.ctrl { self.redo_label(); }
        if input.key_pressed(egui::Key::Enter) && input.modifiers.shift { self.pending_overwrite_once = true; }

        if input.key_pressed(egui::Key::ArrowLeft) { self.selected_tag_index = self.selected_tag_index.saturating_sub(1); }
        if input.key_pressed(egui::Key::ArrowRight) {
            let len = self.visible_tags_for_active_category().len().max(1);
            self.selected_tag_index = (self.selected_tag_index + 1).min(len - 1);
        }
        if input.key_pressed(egui::Key::Space) { self.confirm_selected_tag(); return; }

        let num_keys = [egui::Key::Num1, egui::Key::Num2, egui::Key::Num3, egui::Key::Num4, egui::Key::Num5, egui::Key::Num6, egui::Key::Num7, egui::Key::Num8, egui::Key::Num9];
        for (n, key) in num_keys.iter().enumerate() {
            if input.key_pressed(*key) {
                let tags = self.visible_tags_for_active_category();
                if n < tags.len() {
                    self.selected_tag_index = n;
                    self.confirm_selected_tag();
                }
            }
        }
    }
}
