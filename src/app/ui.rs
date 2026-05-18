use super::*;

fn panel_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(Color32::from_gray(24))
        .stroke(egui::Stroke::new(1.0, Color32::from_gray(35)))
        .corner_radius(egui::CornerRadius::same(4))
        .inner_margin(egui::Margin::same(10))
}

fn card_frame(fill: Color32, stroke: Color32) -> egui::Frame {
    egui::Frame::none()
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, stroke))
        .corner_radius(egui::CornerRadius::same(4))
        .inner_margin(egui::Margin::same(6))
}

fn status_badge(ui: &mut egui::Ui, text: &str, fill: Color32, color: Color32) {
    egui::Frame::none()
        .fill(fill)
        .inner_margin(egui::Margin::symmetric(6, 2))
        .corner_radius(egui::CornerRadius::same(3))
        .show(ui, |ui| {
            ui.label(RichText::new(text).small().strong().color(color));
        });
}

impl VideoTaggerApp {
    pub(super) fn render_top_bar(&mut self, ui: &mut egui::Ui) {
        egui::Frame::none()
            .fill(Color32::from_gray(20))
            .inner_margin(egui::Margin::symmetric(16, 10))
            .stroke(egui::Stroke::new(1.0, Color32::from_gray(30)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Video Tagger").strong().size(16.0).color(Color32::WHITE));
                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(8.0);
                    let mode_text = match self.app_mode {
                        AppMode::Fresh => "选择文件夹",
                        AppMode::Overview => "总览模式",
                        AppMode::Sorting => "分拣模式",
                    };
                    ui.label(RichText::new(mode_text).color(Color32::from_gray(180)).size(14.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let ffmpeg_btn = egui::Button::new(RichText::new("FFmpeg 设置").size(12.0))
                            .fill(Color32::from_gray(40))
                            .stroke(egui::Stroke::new(1.0, Color32::from_gray(60)))
                            .corner_radius(egui::CornerRadius::same(3));
                        if ui.add(ffmpeg_btn).clicked() {
                            self.ffmpeg_dialog_open = true;
                        }
                        if let Some(ref path) = self.ffmpeg_path {
                            let path_str = path.file_name()
                                .map(|n| n.to_string_lossy().into_owned())
                                .unwrap_or_else(|| path.display().to_string());
                            ui.label(RichText::new(format!("FFmpeg: {}", path_str)).small().color(Color32::from_gray(120)));
                            ui.add_space(8.0);
                        }
                    });
                });
            });
    }

    pub(super) fn render_sidebar(&mut self, ui: &mut egui::Ui) {
        egui::Frame::none()
            .fill(Color32::from_gray(24))
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.add_space(4.0);
                    ui.label(RichText::new("控制面板").strong().size(15.0).color(Color32::WHITE));
                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(10.0);
                    let w = ui.available_width().max(1.0);

                    let folder_btn = egui::Button::new(RichText::new("选择文件夹").strong())
                        .fill(Color32::from_gray(38))
                        .stroke(egui::Stroke::new(1.0, Color32::from_gray(55)))
                        .corner_radius(egui::CornerRadius::same(4));
                    if ui.add_sized([w, 32.0], folder_btn).clicked() {
                        self.pick_folder();
                    }

                    if let Some(ref folder) = self.selected_folder {
                        ui.add_space(8.0);
                        egui::Frame::none()
                            .fill(Color32::from_gray(28))
                            .inner_margin(egui::Margin::same(8))
                            .corner_radius(egui::CornerRadius::same(4))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width().max(1.0));
                                ui.label(RichText::new("当前工作目录").small().color(Color32::from_gray(140)));
                                ui.add_space(2.0);
                                ui.label(RichText::new(folder.display().to_string()).small().color(Color32::from_gray(200)));
                            });
                    }

                    ui.add_space(12.0);
                    let btn_text = match self.app_mode {
                        AppMode::Fresh => if self.selected_folder.is_some() { "开始总览" } else { "请先选择文件夹" },
                        AppMode::Overview => "进入分拣",
                        AppMode::Sorting => "退出分拣",
                    };
                    let enabled = self.ffmpeg_path.is_some() && !(self.app_mode == AppMode::Fresh && self.selected_folder.is_none());
                    let main_btn_fill = if enabled {
                        match self.app_mode {
                            AppMode::Sorting => Color32::from_rgb(150, 50, 50),
                            _ => Color32::from_rgb(45, 95, 185),
                        }
                    } else {
                        Color32::from_gray(40)
                    };
                    let main_btn_text_color = if enabled { Color32::WHITE } else { Color32::from_gray(100) };
                    let main_btn = egui::Button::new(RichText::new(btn_text).strong().color(main_btn_text_color))
                        .fill(main_btn_fill)
                        .corner_radius(egui::CornerRadius::same(4));
                    if ui.add_enabled(enabled, main_btn.min_size(Vec2::new(w, 36.0))).clicked() {
                        match self.app_mode {
                            AppMode::Fresh => self.enter_overview(),
                            AppMode::Overview => self.enter_sorting(),
                            AppMode::Sorting => self.exit_sorting(),
                        }
                    }

                    ui.add_space(16.0);
                    panel_frame().show(ui, |ui| {
                        ui.set_width(ui.available_width().max(1.0));
                        ui.label(RichText::new("截图设置").strong().color(Color32::from_gray(220)));
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("间隔").color(Color32::from_gray(160)));
                            ui.add(egui::DragValue::new(&mut self.config.screenshot_interval).range(1.0..=300.0).speed(1.0));
                            ui.label(RichText::new("秒").color(Color32::from_gray(160)));
                        });
                        ui.add_space(6.0);
                        ui.checkbox(&mut self.config.shift_lock, "自动覆盖文件名");
                        ui.add_space(2.0);
                        ui.label(RichText::new("提示: Shift+Enter 临时覆盖一次").small().color(Color32::from_gray(130)));
                    });

                    if let Some(ref prog) = self.folder_progress {
                        ui.add_space(10.0);
                        panel_frame().show(ui, |ui| {
                            ui.set_width(ui.available_width().max(1.0));
                            ui.label(RichText::new("当前目录信息").strong().color(Color32::from_gray(220)));
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("识别码:").small().color(Color32::from_gray(150)));
                                ui.monospace(RichText::new(&prog.identifier).color(Color32::LIGHT_BLUE).strong());
                            });
                            ui.add_space(4.0);
                            ui.label(RichText::new(format!("视频总数: {}", self.videos.len())).small().color(Color32::from_gray(180)));
                        });
                    }

                    if self.app_mode == AppMode::Sorting {
                        ui.add_space(10.0);
                        egui::Frame::none()
                            .fill(Color32::from_gray(32))
                            .stroke(egui::Stroke::new(1.0, Color32::from_rgb(50, 75, 110)))
                            .inner_margin(egui::Margin::same(10))
                            .corner_radius(egui::CornerRadius::same(4))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width().max(1.0));
                                if let Some(video) = self.videos.get(self.current_video_index) {
                                    ui.label(RichText::new("当前选择文件").strong().color(Color32::from_rgb(140, 180, 240)));
                                    ui.add_space(4.0);
                                    ui.label(RichText::new(&video.filename).small().color(Color32::WHITE));
                                    ui.add_space(4.0);
                                    let dur = video.duration_secs.unwrap_or(0.0);
                                    if dur > 0.0 {
                                        ui.label(RichText::new(format!("总时长: {:.1}s", dur)).small().color(Color32::from_gray(160)));
                                    }
                                    let end = self.screenshot_start_sec + self.current_effective_interval() * 9.0;
                                    ui.label(RichText::new(format!("区间: {:.1}s - {:.1}s", self.screenshot_start_sec, end)).small().color(Color32::from_gray(160)));
                                    ui.add_space(4.0);
                                    ui.separator();
                                    ui.add_space(4.0);
                                    ui.label(RichText::new("快捷键提示:").small().strong().color(Color32::from_gray(140)));
                                    ui.label(RichText::new("R : 下一组截图").small().color(Color32::from_gray(160)));
                                    ui.label(RichText::new("Shift+R : 上一组截图").small().color(Color32::from_gray(160)));
                                }
                            });
                    }
                });
            });
    }

    pub(super) fn render_welcome(&self, ui: &mut egui::Ui) {
        ui.centered_and_justified(|ui| {
            egui::Frame::none()
                .fill(Color32::from_gray(20))
                .stroke(egui::Stroke::new(1.0, Color32::from_gray(30)))
                .inner_margin(egui::Margin::same(40))
                .corner_radius(egui::CornerRadius::same(6))
                .show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("视频标签分拣工具").size(26.0).strong().color(Color32::WHITE));
                        ui.add_space(16.0);
                        ui.label(RichText::new("请先在左侧控制面板中点击「选择文件夹」指定视频目录").color(Color32::from_gray(160)).size(14.0));
                        ui.add_space(8.0);
                        ui.label(RichText::new("加载成功后点击「开始总览」，即可开始分拣与标签标记工作。").color(Color32::from_gray(130)).size(13.0));
                    });
                });
        });
    }

    pub(super) fn render_overview(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("媒体总览").strong().size(18.0).color(Color32::WHITE));
            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);
            ui.label(RichText::new(format!("共 {} 个媒体文件", self.videos.len())).color(Color32::from_gray(140)));
        });
        ui.add_space(10.0);

        panel_frame().inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("搜索:").color(Color32::from_gray(160)));
                let search_edit = egui::TextEdit::singleline(&mut self.overview_search)
                    .hint_text("输入关键字按文件名过滤...")
                    .margin(egui::Margin::symmetric(6, 4));
                ui.add_sized([260.0, 26.0], search_edit);
                if !self.overview_search.is_empty() {
                    let clear_btn = egui::Button::new(RichText::new("清空").size(11.0))
                        .fill(Color32::from_gray(45))
                        .corner_radius(egui::CornerRadius::same(3));
                    if ui.add(clear_btn).clicked() { self.overview_search.clear(); }
                }
                ui.add_space(16.0);
                ui.separator();
                ui.add_space(16.0);
                ui.label(RichText::new("排序方式:").color(Color32::from_gray(160)));
                egui::ComboBox::from_id_salt("sort_mode")
                    .selected_text(match self.overview_sort {
                        SortMode::Name => "文件名 A-Z",
                        SortMode::Date => "修改时间 新-旧",
                        SortMode::Size => "文件大小 大-小",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.overview_sort, SortMode::Name, "文件名 A-Z");
                        ui.selectable_value(&mut self.overview_sort, SortMode::Date, "修改时间 新-旧");
                        ui.selectable_value(&mut self.overview_sort, SortMode::Size, "文件大小 大-小");
                    });
            });
        });
        ui.add_space(12.0);

        let filtered = self.sorted_filtered_indices();
        let spacing = 12.0;
        let scrollbar_reserve = 24.0;
        let available = (ui.available_width() - scrollbar_reserve).max(240.0);
        let target_card_w = 220.0;
        let cols = ((available + spacing) / (target_card_w + spacing)).floor().max(1.0) as usize;
        let card_w = ((available - spacing * (cols.saturating_sub(1) as f32)) / cols as f32).floor().clamp(180.0, 260.0);
        let thumb_w = (card_w - 16.0).max(160.0);
        let thumb_h = thumb_w * 9.0 / 16.0;
        let card_h = thumb_h + 60.0;
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
                }
            });
    }

    fn render_thumbnail_card(&mut self, ui: &mut egui::Ui, video_idx: usize, thumb_size: Vec2, card_size: Vec2) {
        let filename = self.videos[video_idx].filename.clone();
        let processed = self.is_processed(video_idx);
        let mut open_edit = false;
        let fill = if processed { Color32::from_rgb(26, 36, 26) } else { Color32::from_gray(26) };
        let stroke_color = if processed { Color32::from_rgb(40, 65, 40) } else { Color32::from_gray(38) };
        let inner_w = (card_size.x - 14.0).max(1.0);
        let inner_h = (card_size.y - 14.0).max(1.0);

        let inner = card_frame(fill, stroke_color).show(ui, |ui| {
            ui.allocate_ui_with_layout(
                Vec2::new(inner_w, inner_h),
                egui::Layout::top_down(egui::Align::Center),
                |ui| {
                    let (rect, response) = ui.allocate_exact_size(thumb_size, egui::Sense::click());
                    self.paint_thumbnail(ui, rect, video_idx, "加载中...");
                    if response.double_clicked() { open_edit = true; }
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.add_space(4.0);
                        if processed {
                            status_badge(ui, "已分拣", Color32::from_rgb(32, 60, 32), Color32::from_rgb(140, 220, 140));
                        } else {
                            status_badge(ui, "未分拣", Color32::from_gray(38), Color32::from_gray(160));
                        }
                    });
                    ui.add_space(4.0);
                    ui.add_sized([thumb_size.x.max(1.0), 32.0], egui::Label::new(RichText::new(filename).small().color(Color32::from_gray(220))).truncate());
                },
            );
        });
        let response = inner.response.interact(egui::Sense::click());
        if response.double_clicked() { open_edit = true; }
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
            ui.put(rect, egui::Image::new(texture).fit_to_exact_size(rect.size()));
        } else if let Some(reason) = self.thumbnail_errors.get(&video_idx) {
            ui.painter().rect_filled(rect, egui::CornerRadius::same(3), Color32::from_rgb(55, 25, 25));
            ui.painter().text(rect.center_top() + egui::vec2(0.0, 10.0), egui::Align2::CENTER_TOP, "错误", egui::FontId::proportional(12.0), Color32::from_rgb(240, 120, 120));
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, reason.chars().take(30).collect::<String>(), egui::FontId::proportional(10.0), Color32::from_rgb(220, 160, 160));
        } else {
            ui.painter().rect_filled(rect, egui::CornerRadius::same(3), Color32::from_gray(38));
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, loading_text, egui::FontId::proportional(11.0), Color32::from_gray(130));
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
        let list_width = 240.0_f32.min((available.x * 0.20).max(200.0));
        let left_width = (available.x - list_width - 16.0).max(1.0);
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.set_width(left_width);
                self.render_sorting_header(ui);
                ui.add_space(8.0);
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
        panel_frame().inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
            ui.horizontal(|ui| {
                if let Some(video) = self.videos.get(self.current_video_index) {
                    status_badge(ui, &format!("{}/{}", self.current_video_index + 1, self.videos.len()), Color32::from_rgb(40, 60, 95), Color32::WHITE);
                    ui.add_space(6.0);
                    ui.label(RichText::new(&video.filename).strong().color(Color32::from_gray(220)));
                    if self.independent_edit.is_some() {
                        ui.add_space(6.0);
                        status_badge(ui, "独立编辑", Color32::from_rgb(70, 60, 20), Color32::from_rgb(240, 200, 100));
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new("流程: Enter 确认标签 -> 任意键切换打星 -> Enter 保存").small().color(Color32::from_gray(140)));
                });
            });
        });
    }

    fn render_screenshot_area(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.screenshot_loading && self.screenshot_paths.is_empty() {
            panel_frame().show(ui, |ui| {
                ui.set_min_height(220.0);
                ui.centered_and_justified(|ui| ui.label(RichText::new("正在从视频抽帧中...").size(15.0).color(Color32::from_gray(140))));
            });
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
            return;
        }
        if let Some(ref err) = self.screenshot_error {
            egui::Frame::none()
                .fill(Color32::from_rgb(45, 25, 25))
                .stroke(egui::Stroke::new(1.0, Color32::from_rgb(90, 40, 40)))
                .corner_radius(egui::CornerRadius::same(4))
                .show(ui, |ui| {
                    ui.set_min_height(220.0);
                    ui.centered_and_justified(|ui| ui.label(RichText::new(err).color(Color32::from_rgb(240, 130, 130)).size(14.0)));
                });
            return;
        }
        if self.screenshot_paths.is_empty() {
            ui.label(RichText::new("加载视频截图中...").color(Color32::from_gray(140)));
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
        let gap = 6.0;
        let cell_w = ((ui.available_width() - gap * 4.0) / cols as f32).clamp(120.0, 340.0);
        let cell_h = cell_w * 9.0 / 16.0;
        let shown_interval = self.current_effective_interval();

        panel_frame().show(ui, |ui| {
            ui.vertical(|ui| {
                for row in 0..2 {
                    ui.horizontal(|ui| {
                        for col in 0..cols {
                            let idx = row * cols + col;
                            if idx >= self.screenshot_paths.len() {
                                ui.allocate_exact_size(Vec2::new(cell_w, cell_h), egui::Sense::hover());
                                ui.add_space(gap);
                                continue;
                            }
                            let (rect, response) = ui.allocate_exact_size(Vec2::new(cell_w, cell_h), egui::Sense::click());
                            let is_playing = self.playing_screenshot == Some(idx);
                            let border_color = if is_playing {
                                Color32::from_rgb(100, 160, 240)
                            } else if response.hovered() {
                                Color32::from_gray(160)
                            } else {
                                Color32::from_gray(50)
                            };
                            ui.painter().rect_filled(rect, egui::CornerRadius::same(3), Color32::from_gray(18));
                            let tex_id = format!("scr_{}_{}_{}", self.current_video_index, (self.screenshot_start_sec * 10.0) as u64, idx);
                            if let Some(tex) = self.screenshot_textures.get(&tex_id) {
                                ui.put(rect, egui::Image::new(tex).fit_to_exact_size(rect.size()));
                            }
                            ui.painter().rect_stroke(rect, egui::CornerRadius::same(3), egui::Stroke::new(2.0, border_color), StrokeKind::Middle);
                            let time_sec = self.screenshot_start_sec + idx as f64 * shown_interval;
                            ui.painter().text(rect.left_bottom() + egui::vec2(6.0, -6.0), egui::Align2::LEFT_BOTTOM, format!("{:.1}s", time_sec), egui::FontId::proportional(11.0), Color32::WHITE);
                            if response.clicked() {
                                self.audio_player.play_clip(&self.videos[self.current_video_index].path, time_sec);
                                self.playing_screenshot = Some(idx);
                            }
                            ui.add_space(gap);
                        }
                    });
                    if row == 0 { ui.add_space(gap); }
                }
                if self.screenshot_loading {
                    ui.add_space(4.0);
                    ui.label(RichText::new("正在预载下一组截图区间数据...").small().color(Color32::from_rgb(100, 150, 200)));
                    ctx.request_repaint_after(std::time::Duration::from_millis(100));
                }
            });
        });
    }

    fn render_label_preview_bar(&mut self, ui: &mut egui::Ui) {
        panel_frame().inner_margin(egui::Margin::symmetric(12, 10)).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("当前视频已选标签:").strong().color(Color32::from_gray(200)));
                ui.add_space(4.0);
                let mut remove_active: Option<usize> = None;
                for (i, label) in self.current_labels.iter().enumerate() {
                    egui::Frame::none()
                        .fill(Color32::from_rgb(32, 55, 100))
                        .stroke(egui::Stroke::new(1.0, Color32::from_rgb(50, 85, 150)))
                        .inner_margin(egui::Margin::symmetric(8, 4))
                        .corner_radius(egui::CornerRadius::same(3))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(label).color(Color32::WHITE).size(12.0));
                                ui.add_space(2.0);
                                let close_btn = egui::Button::new(RichText::new("x").size(10.0).strong()).fill(Color32::TRANSPARENT).frame(false);
                                if ui.add(close_btn).clicked() { remove_active = Some(i); }
                            });
                        });
                    ui.add_space(6.0);
                }
                let mut remove_undone: Option<usize> = None;
                for (i, label) in self.undone_labels.iter().enumerate().rev() {
                    egui::Frame::none()
                        .fill(Color32::from_gray(32))
                        .stroke(egui::Stroke::new(1.0, Color32::from_gray(45)))
                        .inner_margin(egui::Margin::symmetric(8, 4))
                        .corner_radius(egui::CornerRadius::same(3))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(label).strikethrough().color(Color32::from_gray(130)).size(12.0));
                                let close_btn = egui::Button::new(RichText::new("x").size(10.0)).fill(Color32::TRANSPARENT).frame(false);
                                if ui.add(close_btn).clicked() { remove_undone = Some(i); }
                            });
                        });
                    ui.add_space(6.0);
                }
                if self.current_labels.is_empty() && self.undone_labels.is_empty() {
                    ui.label(RichText::new("暂未添加任何标签（直接按 Enter 将进入无星确认流程）").small().color(Color32::from_gray(120)));
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
        panel_frame().inner_margin(egui::Margin::same(12)).show(ui, |ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("快捷标签面板").strong().color(Color32::WHITE));
                    ui.add_space(4.0);
                    ui.label(RichText::new("方向键移动焦点，数字键 1-9 快速添加；右键删除标签").small().color(Color32::from_gray(130)));
                });
                ui.add_space(8.0);
                let total_gap = 6.0 * (cols as f32 - 1.0);
                let btn_w = ((ui.available_width() - total_gap) / cols as f32).max(1.0);
                for row in 0..rows {
                    ui.horizontal(|ui| {
                        for col in 0..cols {
                            let idx = row * cols + col;
                            let is_selected = self.tag_row == row && self.tag_col == col;
                            if idx < tag_names.len() {
                                let tag = &tag_names[idx];
                                let already_added = self.current_labels.iter().any(|existing| existing == tag);
                                let fill = if is_selected {
                                    Color32::from_rgb(45, 95, 185)
                                } else if already_added {
                                    Color32::from_rgb(30, 55, 35)
                                } else {
                                    Color32::from_gray(36)
                                };
                                let stroke = if is_selected {
                                    egui::Stroke::new(1.5, Color32::from_rgb(120, 180, 250))
                                } else if already_added {
                                    egui::Stroke::new(1.0, Color32::from_rgb(50, 90, 60))
                                } else {
                                    egui::Stroke::new(1.0, Color32::from_gray(48))
                                };
                                let text_color = if is_selected || already_added { Color32::WHITE } else { Color32::from_gray(200) };
                                let label_text = if already_added { format!("{}: {}*", col + 1, tag) } else { format!("{}: {}", col + 1, tag) };
                                let btn = egui::Button::new(RichText::new(label_text).size(12.0).color(text_color))
                                    .fill(fill)
                                    .stroke(stroke)
                                    .corner_radius(egui::CornerRadius::same(3));
                                let resp = ui.add_sized([btn_w, 28.0], btn);
                                if resp.clicked() { self.add_label(tag.clone()); }
                                if resp.secondary_clicked() { self.tag_library.remove_tag(tag); self.tag_library.save(); }
                            } else if idx == tag_names.len() && !self.editing_new_tag {
                                let add_btn = egui::Button::new(RichText::new("+ 新标签").size(12.0).color(Color32::from_gray(180)))
                                    .fill(Color32::from_gray(32))
                                    .stroke(egui::Stroke::new(1.0, Color32::from_gray(45)))
                                    .corner_radius(egui::CornerRadius::same(3));
                                if ui.add_sized([btn_w, 28.0], add_btn).clicked() {
                                    self.editing_new_tag = true;
                                    self.new_tag_text.clear();
                                }
                            } else if self.editing_new_tag && idx == tag_names.len() {
                                let edit_input = egui::TextEdit::singleline(&mut self.new_tag_text)
                                    .hint_text("按Enter确认")
                                    .margin(egui::Margin::symmetric(4, 2));
                                ui.add_sized([btn_w, 28.0], edit_input);
                            } else {
                                ui.allocate_exact_size(Vec2::new(btn_w, 28.0), egui::Sense::hover());
                            }
                            if col < cols - 1 { ui.add_space(6.0); }
                        }
                    });
                    if row < rows - 1 { ui.add_space(6.0); }
                }
            });
        });
    }

    fn render_video_list(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("视频队列").strong().size(15.0).color(Color32::WHITE));
        ui.add_space(2.0);
        ui.label(RichText::new("点击单项可回看修改").small().color(Color32::from_gray(130)));
        ui.add_space(6.0);
        ui.separator();
        ui.add_space(6.0);
        let mut clicked: Option<usize> = None;
        let row_h = 142.0;
        let rows = self.videos.len();
        egui::ScrollArea::vertical()
            .id_salt("video_list_scroll")
            .auto_shrink([false, false])
            .show_rows(ui, row_h, rows, |ui, row_range| {
                for i in row_range {
                    let is_current = i == self.current_video_index;
                    let processed = self.is_processed(i);
                    let name = self.videos[i].filename.clone();
                    let w = ui.available_width().max(1.0);
                    let fill = if is_current {
                        Color32::from_rgb(38, 62, 105)
                    } else if processed {
                        Color32::from_rgb(24, 34, 24)
                    } else {
                        Color32::from_gray(26)
                    };
                    let stroke = if is_current {
                        Color32::from_rgb(70, 110, 180)
                    } else if processed {
                        Color32::from_rgb(35, 55, 35)
                    } else {
                        Color32::from_gray(35)
                    };
                    let inner = card_frame(fill, stroke).show(ui, |ui| {
                        ui.allocate_ui_with_layout(
                            Vec2::new((w - 14.0).max(1.0), 125.0),
                            egui::Layout::top_down(egui::Align::Center),
                            |ui| {
                                let (rect, _image_resp) = ui.allocate_exact_size(Vec2::new(132.0, 74.0), egui::Sense::hover());
                                self.paint_thumbnail(ui, rect, i, "...");
                                ui.add_space(4.0);
                                let status_text = if is_current {
                                    format!("> 第 {} 个 (当前)", i + 1)
                                } else if processed {
                                    format!("第 {} 个 (已完成)", i + 1)
                                } else {
                                    format!("第 {} 个", i + 1)
                                };
                                let status_color = if is_current {
                                    Color32::from_rgb(140, 190, 255)
                                } else if processed {
                                    Color32::from_rgb(140, 200, 140)
                                } else {
                                    Color32::from_gray(160)
                                };
                                ui.label(RichText::new(status_text).small().strong().color(status_color));
                                ui.add_space(2.0);
                                ui.add_sized([(w - 20.0).max(1.0), 18.0], egui::Label::new(RichText::new(name).small().color(Color32::from_gray(210))).truncate());
                            },
                        );
                    });
                    let response = inner.response.interact(egui::Sense::click());
                    if response.clicked() { clicked = Some(i); }
                    ui.add_space(8.0);
                }
            });
        if let Some(i) = clicked { self.begin_edit_video(i, false); }
    }

    pub(super) fn render_ffmpeg_dialog(&mut self, ctx: &egui::Context) {
        if !self.ffmpeg_dialog_open { return; }
        egui::Window::new("FFmpeg 环境配置")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    if let Some(ref path) = self.ffmpeg_path {
                        ui.label(RichText::new(format!("系统检测成功: {}", path.display())).color(Color32::LIGHT_GREEN));
                        ui.add_space(6.0);
                        if ui.button("重新扫描系统路径").clicked() { self.ffmpeg_path = ffmpeg::find_ffmpeg(); }
                    } else {
                        ui.label(RichText::new("未检测到 ffmpeg，请安装或手动指定路径:").color(Color32::from_rgb(230, 140, 140)));
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.label("自定义路径:");
                            ui.text_edit_singleline(&mut self.ffmpeg_custom_path);
                        });
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            if ui.button("本地浏览...").clicked() {
                                if let Some(path) = rfd::FileDialog::new().pick_file() { self.ffmpeg_custom_path = path.to_string_lossy().to_string(); }
                            }
                            if ui.button("验证配置并关联").clicked() {
                                let p = std::path::PathBuf::from(&self.ffmpeg_custom_path);
                                if p.exists() { ffmpeg::set_ffmpeg_path(p.clone()); self.ffmpeg_path = Some(p); self.ffmpeg_error = false; self.ffmpeg_dialog_open = false; }
                            }
                        });
                    }
                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(4.0);
                    if ui.button("关闭").clicked() { self.ffmpeg_dialog_open = false; }
                });
            });
    }

    pub(super) fn render_completion_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_completion { return; }
        egui::Window::new("分拣结果状态").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.label(RichText::new("当前工作目录下的所有视频已成功分拣完毕。").strong().color(Color32::LIGHT_GREEN));
            ui.add_space(8.0);
            if ui.button("确定并返回").clicked() { self.show_completion = false; }
        });
    }

    pub(super) fn render_star_hint_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_star_hint { return; }
        egui::Window::new("打星确认阶段")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, [0.0, 80.0])
            .auto_sized()
            .show(ctx, |ui| {
                let hint_str = if self.is_starred {
                    "当前状态: [已打星推荐] | 任意键切换星标状态 | Enter 最终保存并进入下个视频"
                } else {
                    "当前状态: [未打星] | 任意键赋予推荐星标 | Enter 最终保存并进入下个视频"
                };
                ui.label(RichText::new(hint_str).strong().color(if self.is_starred { Color32::from_rgb(240, 200, 80) } else { Color32::from_gray(180) }));
                ui.add_space(6.0);
                if ui.button("关闭浮窗提示").clicked() { self.show_star_hint = false; }
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
