use super::*;
use egui::{Color32, CornerRadius, Margin, Stroke, StrokeKind, Vec2};

fn panel_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(Color32::from_rgb(32, 34, 38))
        .corner_radius(CornerRadius::same(8))
        .stroke(Stroke::new(1.0, Color32::from_rgb(50, 52, 56)))
        .inner_margin(Margin::same(10))
}

fn card_frame(is_active: bool, is_processed: bool) -> egui::Frame {
    let fill = if is_active {
        Color32::from_rgb(45, 60, 90)
    } else if is_processed {
        Color32::from_rgb(35, 48, 40)
    } else {
        Color32::from_rgb(26, 28, 31)
    };
    let stroke = if is_active {
        Color32::from_rgb(90, 125, 215)
    } else {
        Color32::from_rgb(45, 48, 52)
    };
    egui::Frame::new()
        .fill(fill)
        .corner_radius(CornerRadius::same(6))
        .stroke(Stroke::new(1.0, stroke))
        .inner_margin(Margin::same(8))
}

fn badge(ui: &mut egui::Ui, text: &str, fg: Color32, bg: Color32) {
    egui::Frame::new()
        .fill(bg)
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::symmetric(8, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(10.0).strong().color(fg));
        });
}

impl VideoTaggerApp {
    pub(super) fn render_top_bar(&mut self, ui: &mut egui::Ui) {
        egui::Frame::new()
            .fill(Color32::from_rgb(24, 26, 29))
            .inner_margin(Margin::symmetric(16, 10))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Video Tagger").size(18.0).strong());
                    ui.add_space(16.0);
                    let mode_text = match self.app_mode {
                        AppMode::Fresh => "选择文件夹",
                        AppMode::Overview => "总览模式",
                        AppMode::Sorting => "分拣模式",
                    };
                    ui.label(RichText::new(mode_text).color(Color32::from_gray(180)));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(egui::Button::new("FFmpeg").corner_radius(CornerRadius::same(4))).clicked() {
                            self.ffmpeg_dialog_open = true;
                        }
                        if let Some(ref path) = self.ffmpeg_path {
                            ui.label(RichText::new(path.display().to_string()).small().color(Color32::from_gray(120)));
                        }
                    });
                });
            });
    }

    pub(super) fn render_sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.vertical_centered(|ui| ui.label(RichText::new("控制面板").strong().size(16.0)));
        ui.add_space(12.0);

        panel_frame().show(ui, |ui| {
            if ui.add_sized(
                [ui.available_width(), 36.0],
                egui::Button::new("重新选择文件夹").corner_radius(CornerRadius::same(6)),
            ).clicked() {
                self.pick_folder();
            }
            if let Some(ref folder) = self.selected_folder {
                ui.add_space(12.0);
                ui.label(RichText::new("当前工作目录").small().color(Color32::from_gray(150)));
                ui.label(RichText::new(folder.display().to_string()).size(13.0));
            }
            ui.add_space(16.0);
            let btn_text = match self.app_mode {
                AppMode::Fresh => if self.selected_folder.is_some() { "开始总览" } else { "请先选择文件夹" },
                AppMode::Overview => "进入分拣",
                AppMode::Sorting => "返回总览",
            };
            let enabled = self.ffmpeg_path.is_some() && !(self.app_mode == AppMode::Fresh && self.selected_folder.is_none());
            let primary_btn = egui::Button::new(RichText::new(btn_text).strong())
                .min_size(Vec2::new(ui.available_width(), 36.0))
                .fill(if enabled { Color32::from_rgb(70, 100, 180) } else { Color32::from_gray(50) })
                .corner_radius(CornerRadius::same(6));
            if ui.add_enabled(enabled, primary_btn).clicked() {
                match self.app_mode {
                    AppMode::Fresh => self.enter_overview(),
                    AppMode::Overview => self.enter_sorting(),
                    AppMode::Sorting => self.exit_sorting(),
                }
            }
        });

        ui.add_space(12.0);
        panel_frame().show(ui, |ui| {
            ui.label(RichText::new("截图设置").strong());
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("间隔").color(Color32::from_gray(170)));
                ui.add(egui::DragValue::new(&mut self.config.screenshot_interval).range(1.0..=300.0).speed(1.0).suffix(" s"));
            });
            ui.add_space(4.0);
            ui.checkbox(&mut self.config.shift_lock, "覆盖源文件名");
            ui.label(RichText::new("Shift+Enter 临时覆盖一次").small().color(Color32::from_gray(120)));
        });

        if let Some(ref prog) = self.folder_progress {
            ui.add_space(12.0);
            panel_frame().show(ui, |ui| {
                ui.label(RichText::new("文件夹状态").strong());
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("标识码:").color(Color32::from_gray(150)));
                    ui.label(RichText::new(&prog.identifier).monospace().color(Color32::LIGHT_BLUE));
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("总视频:").color(Color32::from_gray(150)));
                    ui.label(format!("{} 个", self.videos.len()));
                });
            });
        }

        if self.app_mode == AppMode::Sorting {
            ui.add_space(12.0);
            panel_frame().show(ui, |ui| {
                if let Some(video) = self.videos.get(self.current_video_index) {
                    ui.label(RichText::new("当前处理信息").strong());
                    ui.add_space(6.0);
                    ui.label(RichText::new(&video.filename).size(13.0).italics());
                    ui.add_space(6.0);
                    let dur = video.duration_secs.unwrap_or(0.0);
                    if dur > 0.0 {
                        ui.label(RichText::new(format!("时长: {:.1}s", dur)).small().color(Color32::from_gray(170)));
                    }
                    let end = self.screenshot_start_sec + self.current_effective_interval() * 9.0;
                    ui.label(RichText::new(format!("区间: {:.1}s - {:.1}s", self.screenshot_start_sec, end)).small().color(Color32::from_gray(170)));
                    ui.add_space(8.0);
                    ui.label(RichText::new("R 后移 / Shift+R 回退").small().color(Color32::from_gray(100)));
                }
            });
        }
    }

    pub(super) fn render_welcome(&self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.3);
            ui.heading(RichText::new("Video Tagger").size(32.0).strong());
            ui.add_space(12.0);
            ui.label(RichText::new("请在左侧面板选择包含视频的文件夹以开始").color(Color32::from_gray(150)).size(16.0));
        });
    }

    pub(super) fn render_overview(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.heading(RichText::new("资源总览").strong());
            ui.label(RichText::new(format!("(共 {} 个视频)", self.videos.len())).color(Color32::from_gray(150)));
        });
        ui.add_space(12.0);

        panel_frame().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("搜索").strong());
                ui.add_sized(
                    [300.0, 28.0],
                    egui::TextEdit::singleline(&mut self.overview_search)
                        .hint_text("输入文件名进行过滤...")
                        .margin(Margin::same(6)),
                );
                if ui.button("清空").clicked() { self.overview_search.clear(); }
                ui.add_space(16.0);
                ui.separator();
                ui.add_space(16.0);
                ui.label(RichText::new("排序").strong());
                egui::ComboBox::from_id_salt("sort_mode")
                    .selected_text(match self.overview_sort {
                        SortMode::Name => "文件名 (A-Z)",
                        SortMode::Date => "修改时间 (新-旧)",
                        SortMode::Size => "文件大小 (大-小)",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.overview_sort, SortMode::Name, "文件名 (A-Z)");
                        ui.selectable_value(&mut self.overview_sort, SortMode::Date, "修改时间 (新-旧)");
                        ui.selectable_value(&mut self.overview_sort, SortMode::Size, "文件大小 (大-小)");
                    });
            });
        });
        ui.add_space(16.0);

        let filtered = self.sorted_filtered_indices();
        let spacing = 16.0;
        let scrollbar_reserve = 24.0;
        let available = (ui.available_width() - scrollbar_reserve).max(240.0);
        let target_card_w = 240.0;
        let cols = ((available + spacing) / (target_card_w + spacing)).floor().max(1.0) as usize;
        let card_w = ((available - spacing * (cols.saturating_sub(1) as f32)) / cols as f32).floor().clamp(180.0, 280.0);
        let thumb_w = (card_w - 16.0).max(160.0);
        let thumb_h = thumb_w * 9.0 / 16.0;
        let card_h = thumb_h + 65.0;
        let row_h = card_h + spacing;
        let rows = (filtered.len() + cols - 1) / cols;

        egui::ScrollArea::vertical()
            .id_salt("overview_scroll")
            .auto_shrink([false, false])
            .show_rows(ui, row_h, rows, |ui, row_range| {
                let mut visible_indices = Vec::new();
                for row in row_range.clone() {
                    let start = row * cols;
                    let end = (start + cols).min(filtered.len());
                    visible_indices.extend_from_slice(&filtered[start..end]);
                }
                self.prioritize_overview_thumbnails(&visible_indices);

                for row in row_range {
                    let start = row * cols;
                    let end = (start + cols).min(filtered.len());
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = spacing;
                        for idx in start..end {
                            self.render_thumbnail_card(ui, filtered[idx], Vec2::new(thumb_w, thumb_h), Vec2::new(card_w, card_h));
                        }
                        for _ in (end - start)..cols {
                            ui.allocate_exact_size(Vec2::new(card_w, card_h), egui::Sense::hover());
                        }
                    });
                    ui.add_space(spacing);
                }
            });
    }

    fn render_thumbnail_card(&mut self, ui: &mut egui::Ui, video_idx: usize, thumb_size: Vec2, card_size: Vec2) {
        let filename = self.videos[video_idx].filename.clone();
        let processed = self.is_processed(video_idx);
        let mut open_edit = false;

        let inner = card_frame(false, processed).show(ui, |ui| {
            ui.allocate_ui_with_layout(
                card_size - Vec2::new(16.0, 16.0),
                egui::Layout::top_down(egui::Align::Center),
                |ui| {
                    let (rect, response) = ui.allocate_exact_size(thumb_size, egui::Sense::click());
                    self.paint_thumbnail(ui, rect, video_idx, "加载中...");
                    if response.double_clicked() { open_edit = true; }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if processed {
                            badge(ui, "已分拣", Color32::from_rgb(100, 220, 130), Color32::from_rgb(30, 80, 45));
                        } else {
                            badge(ui, "未分拣", Color32::from_gray(180), Color32::from_rgb(60, 60, 65));
                        }
                    });
                    ui.add_space(4.0);
                    ui.add_sized([thumb_size.x, 20.0], egui::Label::new(RichText::new(filename).size(13.0)).truncate());
                },
            );
        });

        let response = inner.response.interact(egui::Sense::click());
        if response.double_clicked() { open_edit = true; }
        if response.hovered() { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
        if open_edit { self.begin_edit_video(video_idx, true); }
    }

    fn queue_thumbnail_if_needed(&mut self, video_idx: usize) {
        if !self.overview_thumbnails.contains_key(&video_idx)
            && !self.thumbnail_loaded.contains(&video_idx)
            && !self.thumbnail_errors.contains_key(&video_idx)
            && !self.thumbnail_inflight.contains(&video_idx)
            && !self.thumbnail_queue.contains(&video_idx)
        {
            self.thumbnail_queue.push_back(video_idx);
        }
    }

    fn paint_thumbnail(&mut self, ui: &mut egui::Ui, rect: egui::Rect, video_idx: usize, loading_text: &str) {
        if let Some(texture) = self.overview_thumbnails.get(&video_idx) {
            ui.put(rect, egui::Image::new(texture).fit_to_exact_size(rect.size()).corner_radius(CornerRadius::same(4)));
        } else if let Some(reason) = self.thumbnail_errors.get(&video_idx) {
            ui.painter().rect_filled(rect, CornerRadius::same(4), Color32::from_rgb(50, 25, 25));
            ui.painter().rect_stroke(rect, CornerRadius::same(4), Stroke::new(1.0, Color32::from_rgb(100, 40, 40)), StrokeKind::Middle);
            ui.painter().text(rect.center_top() + egui::vec2(0.0, 20.0), egui::Align2::CENTER_TOP, "Error", egui::FontId::proportional(14.0), Color32::LIGHT_RED);
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, reason.chars().take(30).collect::<String>(), egui::FontId::proportional(10.0), Color32::from_rgb(230, 170, 170));
        } else {
            ui.painter().rect_filled(rect, CornerRadius::same(4), Color32::from_gray(35));
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, loading_text, egui::FontId::proportional(12.0), Color32::from_gray(120));
            self.queue_thumbnail_if_needed(video_idx);
        }
    }

    pub(super) fn poll_thumbnail_results(&mut self, ctx: &egui::Context) {
        let mut changed = false;
        if let Some(rx) = self.thumbnail_rx.take() {
            loop {
                match rx.try_recv() {
                    Ok(ThumbnailResult::Loaded { index, size, rgba }) => {
                        self.thumbnail_inflight.remove(&index);
                        self.thumbnail_loaded.insert(index);
                        let color_img = egui::ColorImage::from_rgba_unmultiplied(size, &rgba);
                        let texture = ctx.load_texture(format!("thumb_{}", index), egui::ImageData::Color(color_img.into()), egui::TextureOptions::LINEAR);
                        self.overview_thumbnails.insert(index, texture);
                        changed = true;
                    }
                    Ok(ThumbnailResult::Failed { index, reason }) => {
                        self.thumbnail_inflight.remove(&index);
                        self.thumbnail_errors.insert(index, reason);
                        changed = true;
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
                }
            }
            self.thumbnail_rx = Some(rx);
        }
        if changed { ctx.request_repaint(); }
    }

    pub(super) fn poll_screenshot_results(&mut self, ctx: &egui::Context) {
        let mut changed = false;
        if let Some(rx) = self.screenshot_rx.take() {
            loop {
                match rx.try_recv() {
                    Ok(ScreenshotResult::Loaded { request_id, key, paths }) if request_id == self.screenshot_request_id => {
                        self.screenshot_cached_ranges.insert(key, paths.clone());
                        self.screenshot_loading = false;
                        self.screenshot_error = None;
                        self.screenshot_paths = paths;
                        self.screenshot_textures.clear();
                        self.prefetch_adjacent_screenshot_ranges();
                        changed = true;
                    }
                    Ok(ScreenshotResult::Prefetched { key, paths }) => {
                        self.screenshot_prefetching.remove(&key);
                        self.screenshot_cached_ranges.insert(key, paths);
                        changed = true;
                    }
                    Ok(ScreenshotResult::Failed { request_id, reason }) if request_id == self.screenshot_request_id => {
                        self.screenshot_loading = false;
                        self.screenshot_error = Some(reason);
                        self.screenshot_paths.clear();
                        self.screenshot_textures.clear();
                        changed = true;
                    }
                    Ok(_) => {}
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
                }
            }
            self.screenshot_rx = Some(rx);
        }
        if changed { ctx.request_repaint(); }
    }

    pub(super) fn process_thumbnail_queue(&mut self, ctx: &egui::Context) {
        let max_new_jobs = 2usize.saturating_sub(self.thumbnail_inflight.len().min(2));
        for _ in 0..max_new_jobs {
            let Some(video_idx) = self.thumbnail_queue.pop_front() else { break; };
            if self.overview_thumbnails.contains_key(&video_idx)
                || self.thumbnail_loaded.contains(&video_idx)
                || self.thumbnail_errors.contains_key(&video_idx)
                || self.thumbnail_inflight.contains(&video_idx)
            {
                continue;
            }
            let Some(video) = self.videos.get(video_idx).cloned() else { continue; };
            self.thumbnail_inflight.insert(video_idx);
            let tx = self.thumbnail_tx.clone();
            let video_hash = ScreenshotCache::video_hash(&video.path);
            let thumb_path = config::cache_dir().join("thumbs").join(format!("thumb_{}.png", video_hash));
            std::thread::spawn(move || {
                let result = (|| -> Result<([usize; 2], Vec<u8>), String> {
                    if let Some(parent) = thumb_path.parent() { std::fs::create_dir_all(parent).map_err(|e| e.to_string())?; }
                    ffmpeg::extract_thumbnail(&video.path, &thumb_path)?;
                    let bytes = std::fs::read(&thumb_path).map_err(|e| e.to_string())?;
                    let img = image::load_from_memory(&bytes).map_err(|e| format!("图片解码失败: {}", e))?;
                    let rgba = img.to_rgba8();
                    let size = [rgba.width() as usize, rgba.height() as usize];
                    Ok((size, rgba.into_raw()))
                })();
                let _ = match result {
                    Ok((size, rgba)) => tx.send(ThumbnailResult::Loaded { index: video_idx, size, rgba }),
                    Err(reason) => tx.send(ThumbnailResult::Failed { index: video_idx, reason }),
                };
            });
        }
        if !self.thumbnail_queue.is_empty() || !self.thumbnail_inflight.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    pub(super) fn render_sorting(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let available = ui.available_size();
        let list_width = 280.0_f32.min((available.x * 0.22).max(220.0));
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.set_width((available.x - list_width - 16.0).max(640.0));
                ui.add_space(8.0);
                self.render_sorting_header(ui);
                ui.add_space(12.0);
                self.render_screenshot_area(ui, ctx);
                ui.add_space(12.0);
                self.render_label_preview_bar(ui);
                ui.add_space(12.0);
                self.render_tag_grid(ui);
            });
            ui.add_space(8.0);
            egui::Frame::new()
                .fill(Color32::from_rgb(22, 24, 26))
                .inner_margin(Margin::same(0))
                .show(ui, |ui| {
                    ui.set_width(list_width);
                    ui.set_height(ui.available_height());
                    self.render_video_list(ui);
                });
        });
    }

    fn render_sorting_header(&mut self, ui: &mut egui::Ui) {
        panel_frame().show(ui, |ui| {
            ui.horizontal(|ui| {
                if let Some(video) = self.videos.get(self.current_video_index) {
                    ui.label(RichText::new(format!("[{}/{}]", self.current_video_index + 1, self.videos.len())).strong().color(Color32::from_rgb(90, 160, 230)).size(16.0));
                    ui.add_space(8.0);
                    ui.label(RichText::new(&video.filename).size(16.0));
                    if self.independent_edit.is_some() {
                        ui.add_space(8.0);
                        badge(ui, "独立编辑", Color32::YELLOW, Color32::from_rgb(100, 80, 20));
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new("Enter 确认标签 -> 任意键切星 -> Enter 保存").small().color(Color32::from_gray(120)));
                });
            });
        });
    }

    fn render_screenshot_area(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.screenshot_loading && self.screenshot_paths.is_empty() {
            panel_frame().show(ui, |ui| {
                ui.set_min_height(240.0);
                ui.centered_and_justified(|ui| {
                    ui.label(RichText::new("截图生成中...").size(18.0).color(Color32::from_gray(140)));
                });
            });
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
            return;
        }

        if let Some(ref err) = self.screenshot_error {
            egui::Frame::new()
                .fill(Color32::from_rgb(50, 25, 25))
                .corner_radius(CornerRadius::same(8))
                .stroke(Stroke::new(1.0, Color32::from_rgb(100, 40, 40)))
                .show(ui, |ui| {
                    ui.set_min_height(240.0);
                    ui.centered_and_justified(|ui| {
                        ui.label(RichText::new(format!("Error: {}", err)).color(Color32::LIGHT_RED).size(16.0));
                    });
                });
            return;
        }

        if self.screenshot_paths.is_empty() {
            ui.label(RichText::new("等待截图...").color(Color32::from_gray(100)));
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
        let cell_w = ((ui.available_width() - gap * (cols as f32 - 1.0)) / cols as f32).clamp(150.0, 360.0);
        let cell_h = cell_w * 9.0 / 16.0;
        let shown_interval = self.current_effective_interval();

        ui.vertical(|ui| {
            for row in 0..2 {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for col in 0..cols {
                        let idx = row * cols + col;
                        let (rect, response) = ui.allocate_exact_size(Vec2::new(cell_w, cell_h), egui::Sense::click());
                        let is_playing = self.playing_screenshot == Some(idx);
                        let border_color = if is_playing {
                            Color32::from_rgb(100, 200, 100)
                        } else if response.hovered() {
                            Color32::from_rgb(150, 180, 255)
                        } else {
                            Color32::TRANSPARENT
                        };

                        ui.painter().rect_filled(rect, CornerRadius::same(6), Color32::from_gray(20));
                        let tex_id = format!("scr_{}_{}_{}", self.current_video_index, (self.screenshot_start_sec * 10.0) as u64, idx);
                        if let Some(tex) = self.screenshot_textures.get(&tex_id) {
                            ui.put(rect, egui::Image::new(tex).fit_to_exact_size(rect.size()).corner_radius(CornerRadius::same(6)));
                        }
                        if border_color != Color32::TRANSPARENT {
                            ui.painter().rect_stroke(rect, CornerRadius::same(6), Stroke::new(3.0, border_color), StrokeKind::Middle);
                        }

                        let time_sec = self.screenshot_start_sec + idx as f64 * shown_interval;
                        let text_rect = egui::Rect::from_min_size(rect.left_bottom() + egui::vec2(6.0, -22.0), egui::vec2(45.0, 16.0));
                        ui.painter().rect_filled(text_rect, CornerRadius::same(4), Color32::from_black_alpha(180));
                        ui.painter().text(text_rect.center(), egui::Align2::CENTER_CENTER, format!("{:.1}s", time_sec), egui::FontId::proportional(11.0), Color32::WHITE);

                        if response.clicked() {
                            self.audio_player.play_clip(&self.videos[self.current_video_index].path, time_sec);
                            self.playing_screenshot = Some(idx);
                        }
                        if response.hovered() { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
                    }
                });
                if row == 0 { ui.add_space(gap); }
            }
            if self.screenshot_loading {
                ui.add_space(6.0);
                ui.label(RichText::new("正在预载下一组截图...").small().color(Color32::from_gray(150)));
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
        });
    }

    fn render_label_preview_bar(&mut self, ui: &mut egui::Ui) {
        panel_frame().show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("待写入标签").strong());
                ui.add_space(8.0);
                let mut remove_active: Option<usize> = None;
                for (i, label) in self.current_labels.iter().enumerate() {
                    let response = egui::Frame::new()
                        .fill(Color32::from_rgb(55, 95, 175))
                        .corner_radius(CornerRadius::same(12))
                        .inner_margin(Margin::symmetric(10, 4))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(label).color(Color32::WHITE));
                                if ui.add(egui::Button::new(RichText::new("x").size(10.0)).fill(Color32::TRANSPARENT).frame(false)).clicked() {
                                    remove_active = Some(i);
                                }
                            });
                        }).response;
                    if response.hovered() { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
                }

                let mut remove_undone: Option<usize> = None;
                for (i, label) in self.undone_labels.iter().enumerate().rev() {
                    egui::Frame::new()
                        .fill(Color32::from_rgb(40, 42, 45))
                        .stroke(Stroke::new(1.0, Color32::from_gray(80)))
                        .corner_radius(CornerRadius::same(12))
                        .inner_margin(Margin::symmetric(10, 4))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(label).strikethrough().color(Color32::from_gray(140)));
                                if ui.add(egui::Button::new(RichText::new("redo").size(10.0)).fill(Color32::TRANSPARENT).frame(false)).clicked() {
                                    remove_undone = Some(i);
                                }
                            });
                        });
                }
                if self.current_labels.is_empty() && self.undone_labels.is_empty() {
                    ui.label(RichText::new("暂无标签 (按 Enter 直接进入无星确认)").small().color(Color32::from_gray(120)));
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
        panel_frame().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("快捷标签板").strong());
                ui.add_space(8.0);
                ui.label(RichText::new("方向键移动，数字 1-9 快捷添加 | 右键删除标签").small().color(Color32::from_gray(120)));
            });
            ui.add_space(10.0);
            let gap = 8.0;
            let btn_w = ((ui.available_width() - gap * (cols as f32 - 1.0)) / cols as f32).max(72.0);
            for row in 0..rows {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for col in 0..cols {
                        let idx = row * cols + col;
                        let is_selected = self.tag_row == row && self.tag_col == col;
                        if idx < tag_names.len() {
                            let tag = &tag_names[idx];
                            let already_added = self.current_labels.iter().any(|existing| existing == tag);
                            let (bg_fill, text_color, stroke) = if is_selected {
                                (Color32::from_rgb(60, 90, 160), Color32::WHITE, Stroke::new(1.0, Color32::from_rgb(100, 150, 255)))
                            } else if already_added {
                                (Color32::from_rgb(35, 65, 45), Color32::from_rgb(180, 230, 190), Stroke::NONE)
                            } else {
                                (Color32::from_rgb(45, 48, 52), Color32::from_gray(200), Stroke::NONE)
                            };
                            let text = if already_added { format!("{}  yes  {}", col + 1, tag) } else { format!("{}  {}", col + 1, tag) };
                            let resp = ui.add(
                                egui::Button::new(RichText::new(text).size(13.0).color(text_color))
                                    .fill(bg_fill)
                                    .stroke(stroke)
                                    .corner_radius(CornerRadius::same(6))
                                    .min_size(Vec2::new(btn_w, 32.0)),
                            );
                            if resp.clicked() { self.add_label(tag.clone()); }
                            if resp.secondary_clicked() { self.tag_library.remove_tag(tag); self.tag_library.save(); }
                            if resp.hovered() { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
                        } else if idx == tag_names.len() && !self.editing_new_tag {
                            let resp = ui.add(
                                egui::Button::new(RichText::new("+ 新增").size(13.0).color(Color32::from_gray(150)))
                                    .fill(Color32::from_rgb(30, 32, 35))
                                    .stroke(Stroke::new(1.0, Color32::from_gray(60)))
                                    .corner_radius(CornerRadius::same(6))
                                    .min_size(Vec2::new(btn_w, 32.0)),
                            );
                            if resp.clicked() { self.editing_new_tag = true; self.new_tag_text.clear(); }
                        } else if self.editing_new_tag && idx == tag_names.len() {
                            ui.add_sized([btn_w, 32.0], egui::TextEdit::singleline(&mut self.new_tag_text).hint_text("按 Enter 保存").margin(Margin::same(6)));
                        } else {
                            ui.add_sized(Vec2::new(btn_w, 32.0), egui::Label::new(""));
                        }
                    }
                });
                if row < rows - 1 { ui.add_space(gap); }
            }
        });
    }

    fn render_video_list(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            ui.vertical(|ui| {
                ui.heading(RichText::new("视频队列").strong().size(16.0));
                ui.label(RichText::new("点击任意视频可回看修改").small().color(Color32::from_gray(120)));
            });
        });
        ui.add_space(8.0);
        ui.separator();
        let mut clicked: Option<usize> = None;
        egui::ScrollArea::vertical().id_salt("video_list_scroll").show(ui, |ui| {
            ui.add_space(8.0);
            for i in 0..self.videos.len() {
                let is_current = i == self.current_video_index;
                let processed = self.is_processed(i);
                let name = self.videos[i].filename.clone();
                let fill = if is_current {
                    Color32::from_rgb(45, 65, 110)
                } else if processed {
                    Color32::from_rgb(30, 45, 35)
                } else {
                    Color32::from_rgb(30, 32, 35)
                };
                let stroke = if is_current { Stroke::new(1.0, Color32::from_rgb(90, 140, 220)) } else { Stroke::NONE };
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    let inner = egui::Frame::new()
                        .fill(fill)
                        .stroke(stroke)
                        .corner_radius(CornerRadius::same(8))
                        .inner_margin(Margin::same(8))
                        .show(ui, |ui| {
                            ui.allocate_ui_with_layout(
                                Vec2::new(ui.available_width() - 16.0, 100.0),
                                egui::Layout::top_down(egui::Align::Center),
                                |ui| {
                                    let (rect, _) = ui.allocate_exact_size(Vec2::new(112.0, 63.0), egui::Sense::hover());
                                    self.paint_thumbnail(ui, rect, i, "...");
                                    ui.add_space(6.0);
                                    let prefix = if is_current { ">" } else if processed { "done" } else { "" };
                                    let color = if is_current { Color32::from_rgb(120, 180, 255) } else if processed { Color32::from_rgb(100, 200, 120) } else { Color32::from_gray(150) };
                                    ui.label(RichText::new(format!("{} {}", prefix, i + 1)).strong().color(color).size(12.0));
                                    ui.add_sized([ui.available_width(), 16.0], egui::Label::new(RichText::new(name).size(11.0).color(Color32::from_gray(200))).truncate());
                                },
                            );
                        });
                    let response = inner.response.interact(egui::Sense::click());
                    if response.clicked() { clicked = Some(i); }
                    if response.hovered() { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
                });
                ui.add_space(6.0);
            }
            ui.add_space(16.0);
        });
        if let Some(i) = clicked { self.begin_edit_video(i, false); }
    }

    pub(super) fn render_ffmpeg_dialog(&mut self, ctx: &egui::Context) {
        if !self.ffmpeg_dialog_open { return; }
        let mut close = false;
        egui::Window::new("FFmpeg 环境配置")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .frame(egui::Frame::window(&ctx.style()).fill(Color32::from_rgb(32, 34, 38)).corner_radius(CornerRadius::same(8)))
            .show(ctx, |ui| {
                ui.add_space(8.0);
                if let Some(ref path) = self.ffmpeg_path {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("已找到 FFmpeg:").color(Color32::LIGHT_GREEN));
                        ui.label(RichText::new(path.display().to_string()).monospace());
                    });
                    ui.add_space(8.0);
                    if ui.button("重新扫描环境变量").clicked() { self.ffmpeg_path = ffmpeg::find_ffmpeg(); }
                } else {
                    ui.label(RichText::new("未找到 ffmpeg，请安装或手动指定路径:").color(Color32::LIGHT_RED));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label("自定义路径:");
                        ui.add_sized([240.0, 24.0], egui::TextEdit::singleline(&mut self.ffmpeg_custom_path));
                    });
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("浏览...").clicked() {
                            if let Some(path) = rfd::FileDialog::new().pick_file() { self.ffmpeg_custom_path = path.to_string_lossy().to_string(); }
                        }
                        if ui.button("确认路径").clicked() {
                            let p = std::path::PathBuf::from(&self.ffmpeg_custom_path);
                            if p.exists() { ffmpeg::set_ffmpeg_path(p.clone()); self.ffmpeg_path = Some(p); self.ffmpeg_error = false; close = true; }
                        }
                    });
                }
                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);
                ui.vertical_centered(|ui| {
                    if ui.add_sized([100.0, 32.0], egui::Button::new("关闭")).clicked() { close = true; }
                });
            });
        if close { self.ffmpeg_dialog_open = false; }
    }

    pub(super) fn render_completion_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_completion { return; }
        let mut close = false;
        egui::Window::new("完成")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .frame(egui::Frame::window(&ctx.style()).fill(Color32::from_rgb(32, 34, 38)).corner_radius(CornerRadius::same(8)))
            .show(ctx, |ui| {
                ui.add_space(12.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("当前文件夹的视频已全部分拣完成！").size(16.0).strong().color(Color32::LIGHT_GREEN));
                    ui.add_space(20.0);
                    if ui.add_sized([120.0, 36.0], egui::Button::new("确定")).clicked() { close = true; }
                });
                ui.add_space(8.0);
            });
        if close { self.show_completion = false; }
    }

    pub(super) fn render_star_hint_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_star_hint { return; }
        let bg_color = if self.is_starred { Color32::from_rgb(180, 140, 30) } else { Color32::from_rgb(50, 60, 80) };
        egui::Window::new("打星确认阶段")
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, [0.0, 100.0])
            .frame(
                egui::Frame::window(&ctx.style())
                    .fill(bg_color)
                    .corner_radius(CornerRadius::same(8))
                    .stroke(Stroke::new(1.0, Color32::WHITE))
                    .inner_margin(Margin::same(16)),
            )
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    let text = if self.is_starred {
                        "已标记星标\n\n按任意键取消 | 按 Enter 保存并进行下一个"
                    } else {
                        "未标记星标\n\n按任意键打星 | 按 Enter 保存并进行下一个"
                    };
                    ui.label(RichText::new(text).size(16.0).color(Color32::WHITE).strong());
                });
            });
    }

    pub(super) fn handle_keyboard_input(&mut self, ctx: &egui::Context) {
        let input = ctx.input(|i| i.clone());
        if self.editing_new_tag {
            if input.key_pressed(egui::Key::Enter) { self.finish_new_tag(); }
            return;
        }
        if self.is_star_phase {
            if input.key_pressed(egui::Key::Enter) { self.finalize_current_video(); return; }
            let any_key = input.events.iter().any(|e| matches!(e, egui::Event::Key { pressed: true, key, .. } if *key != egui::Key::Enter));
            if any_key { self.is_starred = !self.is_starred; }
            return;
        }
        if input.key_pressed(egui::Key::R) { self.advance_screenshots(input.modifiers.shift); }
        if input.key_pressed(egui::Key::Z) && !input.modifiers.ctrl { self.undo_label(); }
        if input.key_pressed(egui::Key::Y) && !input.modifiers.ctrl { self.redo_label(); }
        if input.key_pressed(egui::Key::Enter) { self.confirm_labels_and_enter_star(input.modifiers.shift); return; }
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
