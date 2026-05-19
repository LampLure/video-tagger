use super::*;

impl VideoTaggerApp {
    // App.rs calls this after the normal top bar. The title button is rendered by ui.rs itself.
    // AI settings are placed in the existing left control column, and AI output is placed in the
    // former lower tag area when sorting mode is active.
    pub(super) fn render_ai_mode_toolbar(&mut self, ctx: &egui::Context) {
        if !self.ai_mode {
            return;
        }
        self.render_ai_sidebar_overlay(ctx);
        if self.app_mode == AppMode::Sorting {
            self.render_ai_output_overlay(ctx);
        }
    }

    fn render_ai_sidebar_overlay(&mut self, ctx: &egui::Context) {
        let rect = ctx.screen_rect();
        let top = 430.0_f32.min((rect.bottom() - 260.0).max(150.0));
        let max_h = (rect.bottom() - top - 28.0).max(220.0);
        egui::Area::new("ai_sidebar_overlay".into())
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(8.0, top))
            .show(ctx, |ui| {
                ui.set_width(188.0);
                egui::Frame::none()
                    .fill(Color32::from_rgb(18, 30, 46))
                    .stroke(egui::Stroke::new(1.0, Color32::from_rgb(55, 105, 160)))
                    .corner_radius(egui::CornerRadius::same(4))
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        ui.set_max_height(max_h);
                        ui.label(RichText::new("AI 设置").strong().size(14.0).color(Color32::from_rgb(190, 225, 255)));
                        ui.separator();
                        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                            self.render_ai_service_controls(ui);
                            ui.separator();
                            self.render_ai_runtime_settings(ui);
                            ui.separator();
                            self.render_ai_text_settings(ui);
                        });
                    });
            });
    }

    fn render_ai_output_overlay(&mut self, ctx: &egui::Context) {
        let rect = ctx.screen_rect();
        let width = (rect.width() - 640.0).clamp(520.0, 980.0);
        let x = 215.0;
        let y = (rect.bottom() - 270.0).max(420.0);
        egui::Area::new("ai_output_overlay".into())
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(x, y))
            .show(ctx, |ui| {
                ui.set_width(width);
                self.render_ai_output_area(ui);
            });
    }

    pub(super) fn render_ai_sidebar_settings(&mut self, ui: &mut egui::Ui) {
        ui.add_space(10.0);
        egui::Frame::none()
            .fill(Color32::from_rgb(18, 30, 46))
            .stroke(egui::Stroke::new(1.0, Color32::from_rgb(38, 72, 110)))
            .corner_radius(egui::CornerRadius::same(4))
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.label(RichText::new("AI 设置").strong().size(15.0).color(Color32::from_rgb(190, 225, 255)));
                ui.separator();
                self.render_ai_service_controls(ui);
                ui.separator();
                self.render_ai_runtime_settings(ui);
                ui.separator();
                self.render_ai_text_settings(ui);
            });
    }

    // Disabled: no floating AI window.
    pub(super) fn render_ai_control_window(&mut self, _ctx: &egui::Context) {}

    fn render_ai_service_controls(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("模型服务")
            .default_open(true)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let script_names: Vec<String> = self.ai_scripts.iter()
                        .map(|path| path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| path.display().to_string()))
                        .collect();
                    let selected = script_names.get(self.ai_selected_script).cloned().unwrap_or_else(|| "未找到脚本".to_string());
                    egui::ComboBox::from_id_salt("ai_model_script_integrated")
                        .selected_text(selected)
                        .width(118.0)
                        .show_ui(ui, |ui| {
                            for (i, name) in script_names.iter().enumerate() {
                                ui.selectable_value(&mut self.ai_selected_script, i, name);
                            }
                        });
                    if ui.small_button("脚本").clicked() { self.refresh_ai_scripts(); }
                });
                if self.ai_scripts.is_empty() {
                    ui.label(RichText::new("未找到脚本，请放入 models 文件夹。" ).small().color(Color32::YELLOW));
                }
                let state = match self.ai_service_state {
                    AiServiceState::Disconnected => "未连接",
                    AiServiceState::Starting => "启动中",
                    AiServiceState::ConnectedOwned => "已连接(程序启动)",
                    AiServiceState::ConnectedExternal => "已连接(外部服务)",
                };
                ui.label(RichText::new(format!("状态：{}", state)).small());
                if let Some(props) = &self.ai_service_props {
                    ui.label(RichText::new(format!("视觉:{} 音频:{}", if props.vision { "可用" } else { "不可用" }, if props.audio { "可用" } else { "不可用" })).small());
                } else {
                    ui.label(RichText::new("视觉:未知 音频:未知").small().color(Color32::from_gray(150)));
                }
                ui.horizontal_wrapped(|ui| {
                    let can_start = self.ai_service_state == AiServiceState::Disconnected && !self.ai_scripts.is_empty();
                    if ui.add_enabled(can_start, egui::Button::new("启动")).clicked() { self.start_ai_model_service(); }
                    let can_stop = self.ai_service_state == AiServiceState::ConnectedOwned;
                    if ui.add_enabled(can_stop, egui::Button::new("停止")).clicked() { self.stop_ai_model_service(); }
                    if ui.button("刷新能力").clicked() { self.refresh_ai_props(); }
                });
            });
    }

    fn render_ai_runtime_settings(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("AI 分析参数")
            .default_open(true)
            .show(ui, |ui| {
                if ui.checkbox(&mut self.config.ai_auto_accept, "自动接受").changed() { self.config.save(); }
                ui.horizontal(|ui| { ui.label("追加"); if ui.add(egui::DragValue::new(&mut self.config.ai_max_extra_sample_batches).range(0..=5)).changed() { self.config.save(); } });
                ui.horizontal(|ui| { ui.label("像素"); if ui.add(egui::DragValue::new(&mut self.config.ai_image_max_pixels).range(8000..=500000).speed(1000)).changed() { self.config.save(); } });
                ui.horizontal(|ui| { ui.label("JPG"); if ui.add(egui::DragValue::new(&mut self.config.ai_jpeg_quality).range(20..=95)).changed() { self.config.save(); } });
                ui.horizontal(|ui| { ui.label("音频秒"); if ui.add(egui::DragValue::new(&mut self.config.ai_audio_clip_seconds).range(1.0..=10.0).speed(0.5)).changed() { self.config.save(); } });
                ui.horizontal(|ui| { ui.label("音频段"); if ui.add(egui::DragValue::new(&mut self.config.ai_audio_clips_per_batch).range(0..=10)).changed() { self.config.save(); } });
                ui.horizontal(|ui| { ui.label("超时"); if ui.add(egui::DragValue::new(&mut self.config.ai_stream_idle_timeout_seconds).range(5..=600)).changed() { self.config.save(); } ui.label("秒"); });
            });
    }

    fn render_ai_text_settings(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("AI 文本设置")
            .default_open(true)
            .show(ui, |ui| {
                ui.label(RichText::new("配置文件：ai_text_settings.json").small().color(Color32::from_gray(170)));
                ui.horizontal_wrapped(|ui| {
                    if ui.button("打开 JSON 文件").clicked() {
                        match self.open_ai_text_settings_file() {
                            Ok(()) => self.ai_notice = Some("已打开 AI 文本设置文件。修改保存后，点击“重新加载”。".to_string()),
                            Err(err) => self.ai_notice = Some(err),
                        }
                    }
                    if ui.button("重新加载").clicked() {
                        self.ai_notice = Some(match self.load_ai_text_settings_from_file() {
                            Ok(()) => "AI 文本设置已重新加载，并通过校验。".to_string(),
                            Err(err) => err,
                        });
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    if ui.small_button("校验当前文件").clicked() {
                        self.ai_notice = Some(match self.load_ai_text_settings_from_file() {
                            Ok(()) => "AI 文本设置校验通过。".to_string(),
                            Err(err) => err,
                        });
                    }
                    if ui.small_button("恢复默认文件").clicked() {
                        self.ai_notice = Some(match self.reset_ai_text_settings_file() {
                            Ok(()) => "AI 文本设置文件已恢复默认模板。".to_string(),
                            Err(err) => err,
                        });
                    }
                });
            });
    }

    pub(super) fn render_ai_output_area(&mut self, ui: &mut egui::Ui) {
        egui::Frame::none()
            .fill(Color32::from_rgb(16, 25, 38))
            .stroke(egui::Stroke::new(1.0, Color32::from_rgb(45, 85, 130)))
            .corner_radius(egui::CornerRadius::same(4))
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("AI 输出").strong().size(15.0).color(Color32::from_rgb(200, 230, 255)));
                    ui.separator();
                    let status = match self.ai_batch_state {
                        AiBatchState::Idle => "等待启动",
                        AiBatchState::Running => "正在分析",
                        AiBatchState::AwaitingConfirmation => "等待确认：Space 接受 / Delete 重生",
                    };
                    ui.label(RichText::new(status).small().color(Color32::from_gray(170)));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.ai_batch_state == AiBatchState::Running || self.ai_batch_state == AiBatchState::AwaitingConfirmation {
                            if ui.button("取消 AI 分析").clicked() { self.request_cancel_ai(); }
                        } else if ui.add_enabled(self.app_mode == AppMode::Sorting, egui::Button::new("启动 AI 分析")).clicked() {
                            self.start_ai_batch();
                        }
                    });
                });
                ui.separator();
                if let Some(result) = &self.ai_pending_result {
                    ui.label(RichText::new(format!("候选标签：{}", if result.labels.is_empty() { "无".to_string() } else { result.labels.join("、") })).color(Color32::from_rgb(190, 230, 190)));
                    ui.label(RichText::new(format!("候选评分：{}", result.score)).color(Color32::from_rgb(190, 230, 190)));
                    ui.label(RichText::new("Space 接受，Delete 重新生成。" ).small().color(Color32::YELLOW));
                    ui.separator();
                }
                egui::ScrollArea::vertical().stick_to_bottom(true).max_height(180.0).auto_shrink([false, false]).show(ui, |ui| {
                    if self.ai_log.is_empty() {
                        ui.label(RichText::new("AI 实时分析日志会显示在这里。" ).color(Color32::from_gray(140)));
                    } else {
                        for line in &self.ai_log {
                            ui.label(RichText::new(line).color(Color32::from_gray(225)));
                        }
                    }
                });
            });
    }

    pub(super) fn render_ai_notice(&mut self, ctx: &egui::Context) {
        let Some(text) = self.ai_notice.clone() else { return; };
        egui::Window::new("AI 提示")
            .collapsible(false)
            .resizable(true)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(text);
                if ui.button("确定").clicked() { self.ai_notice = None; }
            });
    }

    pub(super) fn render_ai_cancel_dialog(&mut self, ctx: &egui::Context) {
        if !self.ai_confirm_cancel { return; }
        egui::Window::new("取消 AI 分析")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label("取消 AI 分析会强制关闭模型服务，并丢弃当前视频结果。是否继续？");
                ui.horizontal(|ui| {
                    if ui.button("继续取消").clicked() { self.ai_confirm_cancel = false; self.cancel_ai_now(); }
                    if ui.button("返回").clicked() { self.ai_confirm_cancel = false; }
                });
            });
    }
}
