use super::*;

impl VideoTaggerApp {
    pub(super) fn render_top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Video Tagger");
            ui.separator();
            ui.label(match self.app_mode {
                AppMode::Fresh => "选择文件夹",
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
    }

    pub(super) fn render_sidebar(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| ui.heading("控制面板"));
        ui.separator();

        if ui.add_sized([ui.available_width(), 30.0], egui::Button::new("选择文件夹")).clicked() {
            self.pick_folder();
        }

        if let Some(ref folder) = self.selected_folder {
            ui.add_space(6.0);
            ui.label(RichText::new("当前目录").strong());
            ui.label(RichText::new(folder.display().to_string()).small());
        }

        ui.add_space(8.0);
        let btn_text = match self.app_mode {
            AppMode::Fresh => if self.selected_folder.is_some() { "开始总览" } else { "请先选择文件夹" },
            AppMode::Overview => "进入分拣",
            AppMode::Sorting => "退出分拣",
        };
        let enabled = self.ffmpeg_path.is_some() && !(self.app_mode == AppMode::Fresh && self.selected_folder.is_none());
        if ui.add_enabled(enabled, egui::Button::new(btn_text).min_size(Vec2::new(ui.available_width(), 30.0))).clicked() {
            match self.app_mode {
                AppMode::Fresh => self.enter_overview(),
                AppMode::Overview => self.enter_sorting(),
                AppMode::Sorting => self.exit_sorting(),
            }
        }

        ui.add_space(10.0);
        ui.group(|ui| {
            ui.label(RichText::new("截图设置").strong());
            ui.horizontal(|ui| {
                ui.label("间隔");
                ui.add(egui::DragValue::new(&mut self.config.screenshot_interval).range(1.0..=300.0).speed(1.0));
                ui.label("秒");
            });
            ui.checkbox(&mut self.config.shift_lock, "覆盖文件名");
            ui.label(RichText::new("Shift+Enter 临时覆盖一次").small().color(Color32::from_gray(150)));
        });

        if let Some(ref prog) = self.folder_progress {
            ui.add_space(8.0);
            ui.group(|ui| {
                ui.label(RichText::new("识别码").strong());
                ui.monospace(&prog.identifier);
                ui.label(format!("视频: {}", self.videos.len()));
            });
        }

        if self.app_mode == AppMode::Sorting {
            ui.add_space(8.0);
            ui.group(|ui| {
                if let Some(video) = self.videos.get(self.current_video_index) {
                    ui.label(RichText::new("当前视频").strong());
                    ui.label(RichText::new(&video.filename).small());
                    let dur = video.duration_secs.unwrap_or(0.0);
                    if dur > 0.0 {
                        ui.label(format!("时长: {:.1}s", dur));
                    }
                    let end = self.screenshot_start_sec + self.current_effective_interval() * 9.0;
                    ui.label(format!("截图: {:.1}s - {:.1}s", self.screenshot_start_sec, end));
                    ui.label(RichText::new("R 后移 / Shift+R 回退").small());
                }
            });
        }
    }

    pub(super) fn render_welcome(&self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(100.0);
            ui.heading(RichText::new("视频标签分拣工具").size(32.0));
            ui.add_space(20.0);
            ui.label("请先在左侧边栏选择一个包含视频的文件夹");
            ui.label("选择后点击「开始总览」，再进入分拣。");
        });
    }

    pub(super) fn render_overview(&mut self, ui: &mut egui::Ui) {
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
        egui::Frame::group(ui.style())
            .fill(if processed { Color32::from_rgb(35, 48, 35) } else { Color32::from_gray(28) })
            .show(ui, |ui| {
                ui.set_width(thumb_size.x);
                let (rect, response) = ui.allocate_exact_size(thumb_size, egui::Sense::click());
                self.paint_thumbnail(ui, rect, video_idx, "加载中");
                if response.double_clicked() { open_edit = true; }
                ui.add_space(4.0);
                ui.label(RichText::new(if processed { "已分拣" } else { "未分拣" }).small().color(if processed { Color32::LIGHT_GREEN } else { Color32::from_gray(150) }));
                ui.label(RichText::new(filename).small());
            });
        if open_edit { self.begin_edit_video(video_idx, true); }
    }

    fn queue_thumbnail_if_needed(&mut self, video_idx: usize) {
        if !self.overview_thumbnails.contains_key(&video_idx)
            && !self.thumbnail_loaded.contains(&video_idx)
            && !self.thumbnail_errors.contains(&video_idx)
        {
            self.thumbnail_queue.push_back(video_idx);
            self.thumbnail_loaded.insert(video_idx);
        }
    }

    fn paint_thumbnail(&mut self, ui: &mut egui::Ui, rect: egui::Rect, video_idx: usize, loading_text: &str) {
        if let Some(texture) = self.overview_thumbnails.get(&video_idx) {
            ui.put(rect, egui::Image::new(texture).fit_to_exact_size(rect.size()));
        } else if self.thumbnail_errors.contains(&video_idx) {
            ui.painter().rect_filled(rect, 3.0, Color32::from_rgb(70, 30, 30));
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "Error", egui::FontId::proportional(14.0), Color32::LIGHT_RED);
        } else {
            ui.painter().rect_filled(rect, 3.0, Color32::from_gray(45));
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, loading_text, egui::FontId::proportional(12.0), Color32::from_gray(150));
            self.queue_thumbnail_if_needed(video_idx);
        }
    }

    pub(super) fn process_thumbnail_queue(&mut self, ctx: &egui::Context) {
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

    pub(super) fn render_sorting(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let available = ui.available_size();
        let list_width = 255.0_f32.min((available.x * 0.18).max(210.0));
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.set_width((available.x - list_width - 10.0).max(640.0));
                self.render_sorting_header(ui);
                ui.add_space(6.0);
                self.render_screenshot_area(ui, ctx);
                ui.add_space(6.0);
                self.render_label_preview_bar(ui);
                ui.add_space(6.0);
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
                    if self.independent_edit.is_some() { ui.label(RichText::new("独立编辑").color(Color32::YELLOW)); }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new("Enter 确认标签 → 任意键切星 → Enter 保存").small());
                });
            });
        });
    }

    fn render_screenshot_area(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if let Some(ref err) = self.screenshot_error {
            egui::Frame::group(ui.style()).fill(Color32::from_rgb(60, 25, 25)).show(ui, |ui| {
                ui.set_min_height(220.0);
                ui.centered_and_justified(|ui| ui.label(RichText::new(err).color(Color32::LIGHT_RED).size(22.0)));
            });
            return;
        }
        if self.screenshot_paths.is_empty() { ui.label("加载截图中..."); return; }

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
        let cell_w = ((ui.available_width() - gap * 4.0) / cols as f32).clamp(150.0, 340.0);
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
                        if let Some(tex) = self.screenshot_textures.get(&tex_id) { ui.put(rect, egui::Image::new(tex).fit_to_exact_size(rect.size())); }
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
    }

    fn render_label_preview_bar(&mut self, ui: &mut egui::Ui) {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("标签预览").strong());
                let mut remove_active: Option<usize> = None;
                for (i, label) in self.current_labels.iter().enumerate() {
                    egui::Frame::NONE.fill(Color32::from_rgb(55, 95, 175)).stroke(egui::Stroke::new(1.0, Color32::WHITE)).inner_margin(4.0).show(ui, |ui| {
                        ui.horizontal(|ui| { ui.label(label); if ui.small_button("x").clicked() { remove_active = Some(i); } });
                    });
                }
                let mut remove_undone: Option<usize> = None;
                for (i, label) in self.undone_labels.iter().enumerate().rev() {
                    egui::Frame::NONE.fill(Color32::from_gray(55)).stroke(egui::Stroke::new(1.0, Color32::from_gray(95))).inner_margin(4.0).show(ui, |ui| {
                        ui.horizontal(|ui| { ui.label(RichText::new(label).strikethrough().color(Color32::from_gray(170))); if ui.small_button("x").clicked() { remove_undone = Some(i); } });
                    });
                }
                if self.current_labels.is_empty() && self.undone_labels.is_empty() { ui.label(RichText::new("无标签，直接 Enter 将进入无星确认").small().color(Color32::from_gray(150))); }
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
                ui.label(RichText::new("方向键移动，数字 1-9 选择当前行；同一视频不重复添加同一标签").small().color(Color32::from_gray(150)));
            });
            ui.add_space(4.0);
            let btn_w = ((ui.available_width() - 6.0 * (cols as f32 - 1.0)) / cols as f32).max(72.0);
            for row in 0..rows {
                ui.horizontal(|ui| {
                    for col in 0..cols {
                        let idx = row * cols + col;
                        let is_selected = self.tag_row == row && self.tag_col == col;
                        if idx < tag_names.len() {
                            let tag = &tag_names[idx];
                            let already_added = self.current_labels.iter().any(|existing| existing == tag);
                            let fill = if is_selected { Color32::from_rgb(90, 125, 215) } else if already_added { Color32::from_rgb(45, 75, 55) } else { Color32::from_gray(45) };
                            let text = if already_added { format!("{} ✓{}", col + 1, tag) } else { format!("{} {}", col + 1, tag) };
                            let resp = ui.add(egui::Button::new(RichText::new(text).size(12.0)).fill(fill).min_size(Vec2::new(btn_w, 28.0)));
                            if resp.clicked() { self.add_label(tag.clone()); }
                            if resp.secondary_clicked() { self.tag_library.remove_tag(tag); self.tag_library.save(); }
                        } else if idx == tag_names.len() && !self.editing_new_tag {
                            if ui.add(egui::Button::new("+").min_size(Vec2::new(btn_w, 28.0))).clicked() { self.editing_new_tag = true; self.new_tag_text.clear(); }
                        } else if self.editing_new_tag && idx == tag_names.len() {
                            ui.add_sized([btn_w, 28.0], egui::TextEdit::singleline(&mut self.new_tag_text).hint_text("新标签"));
                        } else {
                            ui.add_sized(Vec2::new(btn_w, 28.0), egui::Label::new(""));
                        }
                    }
                });
                ui.add_space(4.0);
            }
        });
    }

    fn render_video_list(&mut self, ui: &mut egui::Ui) {
        ui.heading("视频列表");
        ui.label(RichText::new("点击回看并修改").small().color(Color32::from_gray(150)));
        ui.separator();
        let item_height = 66.0;
        let mut clicked: Option<usize> = None;
        egui::ScrollArea::vertical().id_salt("video_list_scroll").show_rows(ui, item_height, self.videos.len(), |ui, range| {
            for i in range {
                let is_current = i == self.current_video_index;
                let processed = self.is_processed(i);
                let name = self.videos[i].filename.clone();
                let fill = if is_current { Color32::from_rgb(60, 100, 180) } else if processed { Color32::from_rgb(35, 70, 35) } else { Color32::from_gray(32) };
                egui::Frame::NONE.fill(fill).inner_margin(4.0).show(ui, |ui| {
                    let response = ui.horizontal(|ui| {
                        let (rect, image_resp) = ui.allocate_exact_size(Vec2::new(62.0, 36.0), egui::Sense::click());
                        self.paint_thumbnail(ui, rect, i, "...");
                        ui.vertical(|ui| {
                            ui.label(RichText::new(format!("{}{}", if is_current { "▶ " } else { "" }, i + 1)).strong());
                            ui.label(RichText::new(name).small());
                        });
                        image_resp
                    }).response;
                    if response.clicked() { clicked = Some(i); }
                });
                ui.add_space(3.0);
            }
        });
        if let Some(i) = clicked { self.begin_edit_video(i, false); }
    }

    pub(super) fn render_ffmpeg_dialog(&mut self, ctx: &egui::Context) {
        if !self.ffmpeg_dialog_open { return; }
        egui::Window::new("FFmpeg 设置").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            if let Some(ref path) = self.ffmpeg_path {
                ui.label(format!("已找到: {}", path.display()));
                if ui.button("重新扫描").clicked() { self.ffmpeg_path = ffmpeg::find_ffmpeg(); }
            } else {
                ui.label("未找到 ffmpeg，请安装或手动指定路径:");
                ui.horizontal(|ui| { ui.label("路径:"); ui.text_edit_singleline(&mut self.ffmpeg_custom_path); });
                if ui.button("浏览").clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_file() { self.ffmpeg_custom_path = path.to_string_lossy().to_string(); }
                }
                if ui.button("确认").clicked() {
                    let p = PathBuf::from(&self.ffmpeg_custom_path);
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
            ui.label("全部分拣完成！");
            if ui.button("确定").clicked() { self.show_completion = false; }
        });
    }

    pub(super) fn render_star_hint_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_star_hint { return; }
        egui::Window::new("打星确认").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_TOP, [0.0, 80.0]).auto_sized().show(ctx, |ui| {
            ui.label(if self.is_starred { "★ 已打星 | 任意键切换 | Enter 保存" } else { "☆ 未打星 | 任意键打星 | Enter 保存" });
            if ui.button("隐藏提示").clicked() { self.show_star_hint = false; }
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
