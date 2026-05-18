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
        let top_fill = if self.ai_mode { Color32::from_rgb(16, 34, 58) } else { Color32::from_gray(20) };
        let top_stroke = if self.ai_mode { Color32::from_rgb(45, 90, 140) } else { Color32::from_gray(30) };
        egui::Frame::none()
            .fill(top_fill)
            .inner_margin(egui::Margin::symmetric(16, 10))
            .stroke(egui::Stroke::new(1.0, top_stroke))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let title = if self.ai_mode { "AI Video Tagger" } else { "Video Tagger" };
                    let title_color = if self.ai_mode { Color32::from_rgb(205, 235, 255) } else { Color32::WHITE };
                    let title_fill = if self.ai_mode { Color32::from_rgb(35, 95, 165) } else { Color32::from_gray(20) };
                    let title_response = ui.add(
                        egui::Button::new(RichText::new(title).strong().size(16.0).color(title_color))
                            .fill(title_fill)
                            .stroke(egui::Stroke::new(1.0, if self.ai_mode { Color32::from_rgb(80, 150, 220) } else { Color32::from_gray(35) }))
                            .min_size(Vec2::new(124.0, 28.0)),
                    );
                    if title_response.clicked() {
                        if self.ai_batch_state == AiBatchState::Running {
                            self.ai_notice = Some("AI 正在分析中，请先取消或等待完成后再切换模式。".to_string());
                        } else {
                            self.ai_mode = !self.ai_mode;
                            if self.ai_mode { self.refresh_ai_scripts(); }
                        }
                    }
                    title_response.on_hover_text("点击切换普通模式 / AI 模式");
                    ui.separator();
                    ui.label(RichText::new(match self.app_mode {
                        AppMode::Fresh => "选择文件夹",
                        AppMode::Overview => "总览模式",
                        AppMode::Sorting => "分拣模式",
                    }).color(if self.ai_mode { Color32::from_rgb(190, 220, 245) } else { Color32::from_gray(180) }).size(14.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("FFmpeg 设置").clicked() { self.ffmpeg_dialog_open = true; }
                        if let Some(ref path) = self.ffmpeg_path {
                            let path_str = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string());
                            ui.label(RichText::new(format!("FFmpeg: {}", path_str)).small().color(Color32::from_gray(120)));
                        }
                    });
                });
            });
    }

    pub(super) fn render_sidebar(&mut self, ui: &mut egui::Ui) {
        egui::Frame::none().fill(if self.ai_mode { Color32::from_rgb(22, 32, 46) } else { Color32::from_gray(24) }).inner_margin(egui::Margin::same(12)).show(ui, |ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("控制面板").strong().size(15.0).color(Color32::WHITE));
                ui.separator();
                let w = ui.available_width().max(1.0);
                if ui.add_sized([w, 32.0], egui::Button::new("选择文件夹")).clicked() { self.pick_folder(); }
                if let Some(ref folder) = self.selected_folder {
                    ui.add_space(6.0);
                    panel_frame().show(ui, |ui| {
                        ui.label(RichText::new("当前目录").small().color(Color32::from_gray(140)));
                        ui.label(RichText::new(folder.display().to_string()).small());
                    });
                }

                ui.add_space(8.0);
                let btn_text = match self.app_mode {
                    AppMode::Fresh => if self.selected_folder.is_some() { "开始总览" } else { "请先选择文件夹" },
                    AppMode::Overview => "进入分拣",
                    AppMode::Sorting => "退出分拣",
                };
                let enabled = self.ffmpeg_path.is_some() && !(self.app_mode == AppMode::Fresh && self.selected_folder.is_none());
                if ui.add_enabled(enabled, egui::Button::new(btn_text).min_size(Vec2::new(w, 34.0))).clicked() {
                    match self.app_mode {
                        AppMode::Fresh => self.enter_overview(),
                        AppMode::Overview => self.enter_sorting(),
                        AppMode::Sorting => self.exit_sorting(),
                    }
                }

                ui.add_space(10.0);
                panel_frame().show(ui, |ui| {
                    ui.label(RichText::new("截图设置").strong());
                    ui.horizontal(|ui| {
                        ui.label("间隔");
                        if ui.add(egui::DragValue::new(&mut self.config.screenshot_interval).range(1.0..=300.0).speed(1.0)).changed() { self.config.save(); }
                        ui.label("秒");
                    });
                    if ui.checkbox(&mut self.config.shift_lock, "覆盖文件名").changed() { self.config.save(); }
                });

                ui.add_space(10.0);
                panel_frame().show(ui, |ui| {
                    ui.label(RichText::new("标签设置").strong());
                    if ui.checkbox(&mut self.config.tag_position_lock, "锁定标签位置").changed() { self.config.save(); }
                    ui.label(RichText::new(if self.config.tag_position_lock { "按创建顺序显示" } else { "按频率排序" }).small().color(Color32::from_gray(130)));
                });

                if let Some(ref prog) = self.folder_progress {
                    ui.add_space(10.0);
                    panel_frame().show(ui, |ui| {
                        ui.label(RichText::new("目录信息").strong());
                        ui.label(format!("识别码: {}", prog.identifier));
                        ui.label(format!("视频数: {}", self.videos.len()));
                    });
                }

                if self.app_mode == AppMode::Sorting {
                    ui.add_space(10.0);
                    panel_frame().show(ui, |ui| {
                        if let Some(video) = self.videos.get(self.current_video_index) {
                            ui.label(RichText::new("当前视频").strong());
                            ui.label(RichText::new(&video.filename).small());
                            ui.separator();
                            ui.label(RichText::new("Q/E 翻页，WASD 选图，X 播放").small());
                            ui.label(RichText::new("上下切类别，左右切标签，Space 选择，Delete 撤销").small());
                        }
                    });
                }
            });
        });
    }

    pub(super) fn render_welcome(&self, ui: &mut egui::Ui) {
        ui.centered_and_justified(|ui| {
            panel_frame().inner_margin(egui::Margin::same(40)).show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("视频标签分拣工具").size(26.0).strong());
                    ui.add_space(12.0);
                    ui.label("请先选择视频文件夹，然后开始总览。");
                });
            });
        });
    }

    pub(super) fn render_overview(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("媒体总览").strong().size(18.0));
            ui.separator();
            ui.label(format!("{} 个视频", self.videos.len()));
        });
        ui.add_space(8.0);
        panel_frame().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("搜索");
                ui.add_sized([260.0, 24.0], egui::TextEdit::singleline(&mut self.overview_search).hint_text("按文件名过滤"));
                if ui.button("清空").clicked() { self.overview_search.clear(); }
                ui.separator();
                egui::ComboBox::from_id_salt("sort_mode").selected_text(match self.overview_sort {
                    SortMode::Name => "文件名",
                    SortMode::Date => "修改时间",
                    SortMode::Size => "大小",
                }).show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.overview_sort, SortMode::Name, "文件名");
                    ui.selectable_value(&mut self.overview_sort, SortMode::Date, "修改时间");
                    ui.selectable_value(&mut self.overview_sort, SortMode::Size, "大小");
                });
            });
        });
        ui.add_space(10.0);

        let filtered = self.sorted_filtered_indices();
        let spacing = 12.0;
        let available = (ui.available_width() - 24.0).max(240.0);
        let target_card_w = 220.0;
        let cols = ((available + spacing) / (target_card_w + spacing)).floor().max(1.0) as usize;
        let card_w = ((available - spacing * (cols.saturating_sub(1) as f32)) / cols as f32).floor().clamp(180.0, 260.0);
        let thumb_w = (card_w - 16.0).max(160.0);
        let thumb_h = thumb_w * 9.0 / 16.0;
        let card_h = thumb_h + 60.0;
        let row_h = card_h + spacing;
        let rows = (filtered.len() + cols - 1) / cols;
        egui::ScrollArea::vertical().id_salt("overview_scroll").auto_shrink([false, false]).show_rows(ui, row_h, rows, |ui, row_range| {
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
                    for idx in start..end { self.render_thumbnail_card(ui, filtered[idx], Vec2::new(thumb_w, thumb_h), Vec2::new(card_w, card_h)); }
                    for _ in (end - start)..cols { ui.allocate_exact_size(Vec2::new(card_w, card_h), egui::Sense::hover()); }
                });
            }
        });
    }

    fn render_thumbnail_card(&mut self, ui: &mut egui::Ui, video_idx: usize, thumb_size: Vec2, card_size: Vec2) {
        let filename = self.videos[video_idx].filename.clone();
        let processed = self.is_processed(video_idx);
        let mut open_edit = false;
        let fill = if processed { Color32::from_rgb(26, 36, 26) } else { Color32::from_gray(26) };
        let stroke = if processed { Color32::from_rgb(40, 65, 40) } else { Color32::from_gray(38) };
        let inner = card_frame(fill, stroke).show(ui, |ui| {
            ui.allocate_ui_with_layout(Vec2::new((card_size.x - 14.0).max(1.0), (card_size.y - 14.0).max(1.0)), egui::Layout::top_down(egui::Align::Center), |ui| {
                let (rect, response) = ui.allocate_exact_size(thumb_size, egui::Sense::click());
                self.paint_thumbnail(ui, rect, video_idx, "加载中...");
                if response.double_clicked() { open_edit = true; }
                ui.add_space(6.0);
                if processed { status_badge(ui, "已分拣", Color32::from_rgb(32, 60, 32), Color32::from_rgb(140, 220, 140)); } else { status_badge(ui, "未分拣", Color32::from_gray(38), Color32::from_gray(160)); }
                ui.add_sized([thumb_size.x.max(1.0), 30.0], egui::Label::new(RichText::new(filename).small()).truncate());
            });
        });
        if inner.response.interact(egui::Sense::click()).double_clicked() { open_edit = true; }
        if open_edit { self.begin_edit_video(video_idx, true); }
    }

    fn queue_thumbnail_if_needed(&mut self, video_idx: usize) {
        if !self.overview_thumbnails.contains_key(&video_idx)
            && !self.thumbnail_loaded.contains(&video_idx)
            && !self.thumbnail_errors.contains_key(&video_idx)
            && !self.thumbnail_inflight.contains(&video_idx)
            && !self.thumbnail_queue.contains(&video_idx) {
            self.thumbnail_queue.push_back(video_idx);
        }
    }

    fn paint_thumbnail(&mut self, ui: &mut egui::Ui, rect: egui::Rect, video_idx: usize, loading_text: &str) {
        if let Some(texture) = self.overview_thumbnails.get(&video_idx) {
            ui.put(rect, egui::Image::new(texture).fit_to_exact_size(rect.size()));
        } else if let Some(reason) = self.thumbnail_errors.get(&video_idx) {
            ui.painter().rect_filled(rect, egui::CornerRadius::same(3), Color32::from_rgb(55, 25, 25));
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, reason.chars().take(28).collect::<String>(), egui::FontId::proportional(10.0), Color32::LIGHT_RED);
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
            if self.overview_thumbnails.contains_key(&video_idx) || self.thumbnail_loaded.contains(&video_idx) || self.thumbnail_errors.contains_key(&video_idx) || self.thumbnail_inflight.contains(&video_idx) { continue; }
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
        if !self.thumbnail_queue.is_empty() || !self.thumbnail_inflight.is_empty() { ctx.request_repaint_after(std::time::Duration::from_millis(100)); }
    }

    pub(super) fn render_sorting(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let available = ui.available_size();
        let list_width = (available.x * 0.26).clamp(280.0, 410.0).min((available.x - 760.0).max(260.0));
        let left_width = (available.x - list_width - 16.0).max(1.0);
        let left_height = available.y.max(1.0);
        ui.horizontal(|ui| {
            ui.allocate_ui_with_layout(Vec2::new(left_width, left_height), egui::Layout::top_down(egui::Align::Min), |ui| {
                self.render_sorting_header(ui);
                ui.add_space(8.0);
                let bottom_controls_h = 230.0;
                let screenshot_h = (ui.available_height() - bottom_controls_h).clamp(240.0, 640.0);
                self.render_screenshot_area(ui, ctx, screenshot_h);
                let spacer = (ui.available_height() - bottom_controls_h).max(0.0);
                ui.add_space(spacer);
                if self.ai_mode {
                    ui.add_space(8.0);
                    panel_frame().inner_margin(egui::Margin::symmetric(12, 10)).show(ui, |ui| {
                        ui.label(RichText::new("AI 模式下，标签栏由右侧 AI 输出栏接管。" ).small().color(Color32::from_rgb(180, 215, 245)));
                    });
                } else {
                    self.render_label_preview_bar(ui);
                    ui.add_space(8.0);
                    self.render_tag_grid(ui);
                }
                ui.add_space(16.0);
            });
            ui.separator();
            ui.vertical(|ui| { ui.set_width(list_width); self.render_video_list(ui); });
        });
    }

    fn render_sorting_header(&mut self, ui: &mut egui::Ui) {
        panel_frame().inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
            ui.horizontal(|ui| {
                if let Some(video) = self.videos.get(self.current_video_index) {
                    status_badge(ui, &format!("{}/{}", self.current_video_index + 1, self.videos.len()), Color32::from_rgb(40, 60, 95), Color32::WHITE);
                    ui.label(RichText::new(&video.filename).strong().color(Color32::from_gray(220)));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(if self.ai_mode { "AI 模式：右侧输出栏显示模型实时反馈；Space/Delete 仅用于 AI 确认" } else { "类别式打标：Space 选中并进入下一类，最后一类为星标" }).small().color(Color32::from_gray(140)));
                });
            });
        });
    }

    fn render_screenshot_area(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, desired_height: f32) {
        if self.screenshot_loading && self.screenshot_paths.is_empty() {
            panel_frame().show(ui, |ui| {
                ui.set_min_height(desired_height.max(220.0));
                ui.centered_and_justified(|ui| ui.label(RichText::new("正在从视频抽帧中...").size(15.0).color(Color32::from_gray(140))));
            });
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
            return;
        }
        if let Some(ref err) = self.screenshot_error {
            egui::Frame::none().fill(Color32::from_rgb(45, 25, 25)).show(ui, |ui| {
                ui.set_min_height(desired_height.max(220.0));
                ui.centered_and_justified(|ui| ui.label(RichText::new(err).color(Color32::LIGHT_RED).size(14.0)));
            });
            return;
        }
        if self.screenshot_paths.is_empty() { ui.label("加载视频截图中..."); return; }

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
        let cell_w = (((ui.available_width() - gap * 4.0) / cols as f32).max(120.0)).clamp(120.0, 380.0);
        let grid_height = (desired_height - 36.0).max(180.0);
        let cell_h = (((grid_height - gap) / 2.0).max(90.0)).min(cell_w * 0.72).clamp(90.0, 260.0);
        let shown_interval = self.current_effective_interval();

        panel_frame().show(ui, |ui| {
            ui.set_min_height((cell_h * 2.0 + gap + 24.0).max(220.0));
            for row in 0..2 {
                ui.horizontal(|ui| {
                    for col in 0..cols {
                        let idx = row * cols + col;
                        let (rect, response) = ui.allocate_exact_size(Vec2::new(cell_w, cell_h), egui::Sense::click());
                        if idx < self.screenshot_paths.len() {
                            if response.clicked() { self.selected_screenshot_index = idx; }
                            let selected = self.selected_screenshot_index == idx;
                            let playing = self.playing_screenshot == Some(idx);
                            let border = if playing { Color32::YELLOW } else if selected { Color32::from_rgb(110, 170, 255) } else if response.hovered() { Color32::from_gray(160) } else { Color32::from_gray(50) };
                            ui.painter().rect_filled(rect, egui::CornerRadius::same(3), Color32::from_gray(18));
                            let tex_id = format!("scr_{}_{}_{}", self.current_video_index, (self.screenshot_start_sec * 10.0) as u64, idx);
                            if let Some(tex) = self.screenshot_textures.get(&tex_id) { ui.put(rect, egui::Image::new(tex).fit_to_exact_size(rect.size())); }
                            ui.painter().rect_stroke(rect, egui::CornerRadius::same(3), egui::Stroke::new(if selected { 3.0 } else { 2.0 }, border), StrokeKind::Middle);
                            let time_sec = self.screenshot_start_sec + idx as f64 * shown_interval;
                            ui.painter().text(rect.left_bottom() + egui::vec2(6.0, -6.0), egui::Align2::LEFT_BOTTOM, format!("{:.1}s", time_sec), egui::FontId::proportional(11.0), Color32::WHITE);
                        }
                        if col < cols - 1 { ui.add_space(gap); }
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
    }

    fn render_label_preview_bar(&mut self, ui: &mut egui::Ui) {
        panel_frame().inner_margin(egui::Margin::symmetric(12, 10)).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("已选:").strong());
                for label in &self.current_labels { status_badge(ui, label, Color32::from_rgb(32, 55, 100), Color32::WHITE); }
                if self.is_starred { status_badge(ui, "星标", Color32::from_rgb(95, 70, 20), Color32::from_rgb(250, 220, 120)); }
                if self.current_labels.is_empty() && !self.is_starred { ui.label(RichText::new("无").small().color(Color32::from_gray(130))); }
            });
        });
    }

    fn render_tag_grid(&mut self, ui: &mut egui::Ui) {
        panel_frame().inner_margin(egui::Margin::same(12)).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("标签类别").strong().color(Color32::WHITE));
                for i in 0..self.tag_library.category_count() {
                    let name = self.tag_library.categories()[i].name.clone();
                    let active = self.active_category_index == i;
                    if ui.add(egui::Button::new(RichText::new(name).color(if active { Color32::WHITE } else { Color32::from_gray(180) })).fill(if active { Color32::from_rgb(45, 95, 185) } else { Color32::from_gray(36) })).clicked() {
                        self.select_category(i);
                    }
                }
                let star_active = self.is_star_category();
                if ui.add(egui::Button::new(STAR_CATEGORY_NAME).fill(if star_active { Color32::from_rgb(95, 70, 20) } else { Color32::from_gray(36) })).clicked() {
                    self.select_category(self.tag_library.category_count());
                }
                if self.tag_library.category_count() < MAX_TAG_CATEGORIES && !self.editing_new_category {
                    if ui.button("+ 添加标签类别").clicked() { self.editing_new_category = true; self.new_category_text.clear(); }
                }
                if self.editing_new_category {
                    ui.add_sized([120.0, 24.0], egui::TextEdit::singleline(&mut self.new_category_text).hint_text("类别名"));
                }
                if !self.is_star_category() && self.tag_library.category_count() > 0 {
                    if ui.button("删除当前类别").clicked() {
                        self.tag_library.remove_category(self.active_category_index);
                        self.tag_library.save();
                        self.select_category(self.active_category_index.min(self.tag_library.category_count()));
                    }
                }
            });
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                let tags = self.current_category_tags();
                for i in 0..tags.len() {
                    let tag = &tags[i];
                    let selected = self.selected_tag_index == i;
                    let already = if self.is_star_category() { (i == 1 && self.is_starred) || (i == 0 && !self.is_starred) } else { self.current_labels.get(self.active_category_index).map(|l| l == tag).unwrap_or(false) };
                    let fill = if selected { Color32::from_rgb(45, 95, 185) } else if already { Color32::from_rgb(30, 55, 35) } else { Color32::from_gray(36) };
                    let text = format!("{}: {}", i + 1, tag);
                    if ui.add(egui::Button::new(RichText::new(text).size(12.0)).fill(fill).min_size(Vec2::new(86.0, 28.0))).clicked() {
                        self.selected_tag_index = i;
                        self.select_current_tag_and_advance();
                    }
                }
                if !self.is_star_category() && tags.len() < MAX_TAGS_PER_CATEGORY && !self.editing_new_tag {
                    if ui.button("+ 新标签").clicked() { self.editing_new_tag = true; self.new_tag_text.clear(); }
                }
                if self.editing_new_tag {
                    ui.add_sized([120.0, 28.0], egui::TextEdit::singleline(&mut self.new_tag_text).hint_text("标签名"));
                }
                if !self.is_star_category() && self.selected_tag_index < tags.len() {
                    if ui.button("删除选中标签").clicked() {
                        let name = tags[self.selected_tag_index].clone();
                        self.tag_library.remove_tag_from_category(self.active_category_index, &name);
                        self.tag_library.save();
                        self.selected_tag_index = self.selected_tag_index.saturating_sub(1);
                    }
                }
            });
            ui.add_space(4.0);
            ui.label(RichText::new("上下键切换类别，左右键切换标签，Space 选择并进入下一类别，Delete 撤销最近选择。最后类别固定为星标。").small().color(Color32::from_gray(130)));
        });
    }

    fn render_video_list(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("视频队列").strong().size(15.0));
        ui.label(RichText::new("自动进入下一视频时队列会跟随；手动滚动不会被锁定").small().color(Color32::from_gray(130)));
        ui.separator();
        let mut clicked: Option<usize> = None;
        let row_h = 178.0;
        let rows = self.videos.len();
        let follow_index = self.video_list_follow_index.take();
        let mut scroll_area = egui::ScrollArea::vertical()
            .id_salt("video_list_scroll")
            .auto_shrink([false, false]);
        if let Some(index) = follow_index {
            scroll_area = scroll_area.vertical_scroll_offset(index as f32 * row_h);
        }
        scroll_area.show_rows(ui, row_h, rows, |ui, row_range| {
            for i in row_range {
                let is_current = i == self.current_video_index;
                let processed = self.is_processed(i);
                let name = self.videos[i].filename.clone();
                let w = ui.available_width().max(1.0);
                let fill = if is_current { Color32::from_rgb(38, 62, 105) } else if processed { Color32::from_rgb(24, 34, 24) } else { Color32::from_gray(26) };
                let stroke = if is_current { Color32::from_rgb(70, 110, 180) } else if processed { Color32::from_rgb(35, 55, 35) } else { Color32::from_gray(35) };
                let thumb_w = (w - 42.0).clamp(150.0, 240.0);
                let thumb_h = (thumb_w * 9.0 / 16.0).clamp(84.0, 135.0);
                let card_h = (thumb_h + 72.0).max(150.0);
                let inner = card_frame(fill, stroke).show(ui, |ui| {
                    ui.allocate_ui_with_layout(Vec2::new((w - 18.0).max(1.0), card_h), egui::Layout::top_down(egui::Align::Center), |ui| {
                        let (rect, _) = ui.allocate_exact_size(Vec2::new(thumb_w, thumb_h), egui::Sense::hover());
                        self.paint_thumbnail(ui, rect, i, "...");
                        ui.label(RichText::new(format!("{}第 {} 个", if is_current { "> " } else { "" }, i + 1)).small().strong());
                        ui.add_sized([(w - 28.0).max(1.0), 22.0], egui::Label::new(RichText::new(name).small()).truncate());
                    });
                });
                if inner.response.interact(egui::Sense::click()).clicked() { clicked = Some(i); }
                ui.add_space(8.0);
            }
        });
        if let Some(i) = clicked { self.switch_to_video(i, false); }
    }

    pub(super) fn render_ffmpeg_dialog(&mut self, ctx: &egui::Context) {
        if !self.ffmpeg_dialog_open { return; }
        egui::Window::new("FFmpeg 环境配置").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            if let Some(ref path) = self.ffmpeg_path {
                ui.label(format!("已找到: {}", path.display()));
                if ui.button("重新扫描").clicked() { self.ffmpeg_path = ffmpeg::find_ffmpeg(); }
            } else {
                ui.label("未找到 ffmpeg，请安装或手动指定路径:");
                ui.horizontal(|ui| { ui.label("路径:"); ui.text_edit_singleline(&mut self.ffmpeg_custom_path); });
                if ui.button("浏览").clicked() { if let Some(path) = rfd::FileDialog::new().pick_file() { self.ffmpeg_custom_path = path.to_string_lossy().to_string(); } }
                if ui.button("确认").clicked() {
                    let p = std::path::PathBuf::from(&self.ffmpeg_custom_path);
                    if p.exists() { ffmpeg::set_ffmpeg_path(p.clone()); self.ffmpeg_path = Some(p); self.ffmpeg_error = false; self.ffmpeg_dialog_open = false; }
                }
            }
            ui.separator();
            if ui.button("关闭").clicked() { self.ffmpeg_dialog_open = false; }
        });
    }

    pub(super) fn render_completion_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_completion { return; }
        egui::Window::new("完成").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.label("全部分拣完成。");
            if ui.button("确定").clicked() { self.show_completion = false; }
        });
    }
}
