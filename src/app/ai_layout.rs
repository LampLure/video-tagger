use super::*;

fn ai_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(Color32::from_rgb(16, 25, 38))
        .stroke(egui::Stroke::new(1.0, Color32::from_rgb(45, 85, 130)))
        .corner_radius(egui::CornerRadius::same(4))
        .inner_margin(egui::Margin::same(10))
}

impl VideoTaggerApp {
    pub(super) fn render_ai_sorting(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let available = ui.available_size();
        let list_width = (available.x * 0.24).clamp(280.0, 390.0).min((available.x - 760.0).max(260.0));
        let workspace_width = (available.x - list_width - 16.0).max(420.0);
        let height = available.y.max(420.0);
        ui.horizontal(|ui| {
            ui.allocate_ui_with_layout(Vec2::new(workspace_width, height), egui::Layout::top_down(egui::Align::Min), |ui| {
                ai_frame().inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if let Some(video) = self.videos.get(self.current_video_index) {
                            ui.label(RichText::new(format!("{}/{}", self.current_video_index + 1, self.videos.len())).strong());
                            ui.label(RichText::new(&video.filename).strong());
                        }
                    });
                });
                ui.add_space(8.0);
                let output_h = 230.0;
                let shot_h = (ui.available_height() - output_h - 14.0).clamp(260.0, 620.0);
                self.render_ai_screenshots_simple(ui, ctx, shot_h);
                ui.add_space(8.0);
                ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), output_h), egui::Layout::top_down(egui::Align::Min), |ui| self.render_ai_output_area(ui));
            });
            ui.separator();
            ui.allocate_ui_with_layout(Vec2::new(list_width, height), egui::Layout::top_down(egui::Align::Min), |ui| self.render_ai_video_queue_simple(ui));
        });
    }

    fn render_ai_screenshots_simple(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, desired_height: f32) {
        if self.screenshot_paths.is_empty() {
            ai_frame().show(ui, |ui| {
                ui.set_min_height(desired_height.max(220.0));
                ui.centered_and_justified(|ui| ui.label("加载视频截图中..."));
            });
            return;
        }
        for (idx, path) in self.screenshot_paths.iter().enumerate() {
            let id = format!("ai_scr_{}_{}", self.current_video_index, idx);
            if !self.screenshot_textures.contains_key(&id) {
                if let Ok(data) = std::fs::read(path) {
                    if let Ok(img) = image::load_from_memory(&data) {
                        let rgba = img.to_rgba8();
                        let size = [rgba.width() as _, rgba.height() as _];
                        let pixels = rgba.into_raw();
                        let color_img = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
                        let texture = ctx.load_texture(id.clone(), egui::ImageData::Color(color_img.into()), egui::TextureOptions::LINEAR);
                        self.screenshot_textures.insert(id, texture);
                    }
                }
            }
        }
        let cols = 5;
        let gap = 6.0;
        let cell_w = (((ui.available_width() - gap * 4.0) / cols as f32).max(120.0)).clamp(120.0, 380.0);
        let cell_h = (((desired_height - 52.0) / 2.0).max(90.0)).min(cell_w * 0.72).clamp(90.0, 260.0);
        ai_frame().show(ui, |ui| {
            ui.set_min_height((cell_h * 2.0 + gap + 24.0).max(220.0));
            for row in 0..2 {
                ui.horizontal(|ui| {
                    for col in 0..cols {
                        let idx = row * cols + col;
                        let (rect, response) = ui.allocate_exact_size(Vec2::new(cell_w, cell_h), egui::Sense::click());
                        if idx < self.screenshot_paths.len() {
                            if response.clicked() { self.selected_screenshot_index = idx; }
                            let selected = self.selected_screenshot_index == idx;
                            ui.painter().rect_filled(rect, egui::CornerRadius::same(3), Color32::from_gray(18));
                            let id = format!("ai_scr_{}_{}", self.current_video_index, idx);
                            if let Some(tex) = self.screenshot_textures.get(&id) { ui.put(rect, egui::Image::new(tex).fit_to_exact_size(rect.size())); }
                            ui.painter().rect_stroke(rect, egui::CornerRadius::same(3), egui::Stroke::new(if selected { 3.0 } else { 1.0 }, if selected { Color32::from_rgb(110, 170, 255) } else { Color32::from_gray(60) }), StrokeKind::Middle);
                            let time_sec = self.screenshot_start_sec + idx as f64 * self.screenshot_interval.max(0.1);
                            ui.painter().text(rect.left_bottom() + egui::vec2(6.0, -6.0), egui::Align2::LEFT_BOTTOM, format!("{:.1}s", time_sec), egui::FontId::proportional(11.0), Color32::WHITE);
                        }
                        if col < cols - 1 { ui.add_space(gap); }
                    }
                });
                if row == 0 { ui.add_space(gap); }
            }
        });
    }

    fn render_ai_video_queue_simple(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("视频队列").strong().size(15.0));
        ui.separator();
        egui::ScrollArea::vertical().id_salt("ai_video_queue").show(ui, |ui| {
            for i in 0..self.videos.len() {
                let name = self.videos[i].filename.clone();
                let current = i == self.current_video_index;
                let fill = if current { Color32::from_rgb(38, 62, 105) } else { Color32::from_rgb(24, 34, 24) };
                ai_frame().fill(fill).show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new(format!("{}第 {} 个", if current { "> " } else { "" }, i + 1)).small().strong());
                        ui.add_sized([ui.available_width().max(1.0), 22.0], egui::Label::new(RichText::new(name).small()).truncate());
                    });
                });
                ui.add_space(8.0);
            }
        });
    }
}
