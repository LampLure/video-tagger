use super::*;

fn compact_name(name: &str, max_chars: usize) -> String {
    let chars: Vec<char> = name.chars().collect();
    if chars.len() <= max_chars {
        return name.to_string();
    }
    if max_chars <= 8 {
        return chars.into_iter().take(max_chars).collect();
    }
    let head = (max_chars - 3) / 2;
    let tail = max_chars - 3 - head;
    let start: String = chars.iter().take(head).collect();
    let end: String = chars.iter().skip(chars.len().saturating_sub(tail)).collect();
    format!("{}...{}", start, end)
}

impl VideoTaggerApp {
    pub(super) fn render_ai_mode_toolbar(&mut self, _ctx: &egui::Context) {}

    fn force_stop_ai_service_and_port(&mut self) {
        if let Some(mut child) = self.ai_model_process.take() {
            ai::stop_process_tree(&mut child);
        }
        #[cfg(target_os = "windows")]
        {
            let _ = std::process::Command::new("cmd")
                .args(["/C", "for /f \"tokens=5\" %a in ('netstat -ano ^| findstr :7080') do taskkill /PID %a /T /F"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
        #[cfg(all(unix, not(target_os = "windows")))]
        {
            let _ = std::process::Command::new("sh")
                .arg("-c")
                .arg("fuser -k 7080/tcp >/dev/null 2>&1 || true; pids=$(lsof -ti tcp:7080 2>/dev/null || true); if [ -n \"$pids\" ]; then kill -9 $pids >/dev/null 2>&1 || true; fi")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
        self.ai_service_state = AiServiceState::Disconnected;
        self.ai_service_props = None;
        self.ai_batch_state = AiBatchState::Idle;
        self.ai_pending_result = None;
        self.ai_log.push("程序：已强制停止 AI 服务，并尝试杀死占用 7080 端口的进程。".to_string());
        self.ai_notice = Some("已强制停止 AI 服务，并尝试杀死占用 7080 端口的进程。".to_string());
    }

    pub(super) fn render_ai_sidebar_settings(&mut self, ui: &mut egui::Ui) {
        ui.add_space(10.0);
        egui::Frame::none()
            .fill(Color32::from_rgb(18, 30, 46))
            .stroke(egui::Stroke::new(1.0, Color32::from_rgb(55, 105, 160)))
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

    pub(super) fn render_ai_control_window(&mut self, _ctx: &egui::Context) {}

    fn render_ai_service_controls(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("模型服务")
            .default_open(true)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let script_names: Vec<String> = self.ai_scripts.iter()
                        .map(|path| path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| path.display().to_string()))
                        .collect();
                    let selected_full = script_names.get(self.ai_selected_script).cloned().unwrap_or_else(|| "未找到脚本".to_string());
                    let selected = compact_name(&selected_full, 24);
                    egui::ComboBox::from_id_salt("ai_model_script_integrated")
                        .selected_text(selected)
                        .width(150.0)
                        .show_ui(ui, |ui| {
                            for (i, name) in script_names.iter().enumerate() {
                                ui.selectable_value(&mut self.ai_selected_script, i, compact_name(name, 36));
                            }
                        })
                        .response
                        .on_hover_text(selected_full);
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
                    if ui.button("停止").clicked() { self.force_stop_ai_service_and_port(); }
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
                ui.horizontal(|ui| { ui.label("超时"); if ui.add(egui::DragValue::new(&mut self.config.ai_stream_idle_timeout_seconds).range(60..=3600)).changed() { self.config.save(); } ui.label("秒"); });
                if self.config.ai_stream_idle_timeout_seconds < 60 {
                    self.config.ai_stream_idle_timeout_seconds = 600;
                    self.config.save();
                }
                ui.label(RichText::new("本地多模态模型首 token 可能很慢，建议超时 >= 600 秒。" ).small().color(Color32::from_gray(150)));
            });
    }

    fn render_ai_text_settings(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("AI 文本与提示词")
            .default_open(true)
            .show(ui, |ui| {
                ui.label(RichText::new("标签/评分文件：ai_text_settings.json").small().color(Color32::from_gray(170)));
                ui.horizontal_wrapped(|ui| {
                    if ui.button("打开标签 JSON").clicked() {
                        match self.open_ai_text_settings_file() {
                            Ok(()) => self.ai_notice = Some("已打开 AI 标签/评分设置文件。修改保存后，点击“重新加载标签”。".to_string()),
                            Err(err) => self.ai_notice = Some(err),
                        }
                    }
                    if ui.button("重新加载标签").clicked() {
                        self.ai_notice = Some(match self.load_ai_text_settings_from_file() {
                            Ok(()) => "AI 标签/评分设置已重新加载，并通过校验。".to_string(),
                            Err(err) => err,
                        });
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    if ui.small_button("校验标签文件").clicked() {
                        self.ai_notice = Some(match self.load_ai_text_settings_from_file() {
                            Ok(()) => "AI 标签/评分设置校验通过。".to_string(),
                            Err(err) => err,
                        });
                    }
                    if ui.small_button("恢复默认标签").clicked() {
                        self.ai_notice = Some(match self.reset_ai_text_settings_file() {
                            Ok(()) => "AI 标签/评分设置文件已恢复默认模板。".to_string(),
                            Err(err) => err,
                        });
                    }
                });
                ui.separator();
                ui.label(RichText::new("提示词文件：ai_prompt_template.txt").small().color(Color32::from_gray(170)));
                ui.horizontal_wrapped(|ui| {
                    if ui.button("打开提示词").clicked() {
                        match self.open_ai_prompt_template_file() {
                            Ok(()) => self.ai_notice = Some("已打开 AI 提示词模板。修改保存后，下次 AI 分析会自动使用新提示词。".to_string()),
                            Err(err) => self.ai_notice = Some(err),
                        }
                    }
                    if ui.small_button("恢复默认提示词").clicked() {
                        self.ai_notice = Some(match self.reset_ai_prompt_template_file() {
                            Ok(()) => "AI 提示词模板已恢复默认。".to_string(),
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
                    ui.separator();
                    ui.label(RichText::new(format!("保留最近 {} 条", self.ai_log_records.len().min(5))).small().color(Color32::from_gray(130)));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.ai_batch_state == AiBatchState::Running || self.ai_batch_state == AiBatchState::AwaitingConfirmation {
                            if ui.button("取消 AI 分析").clicked() { self.request_cancel_ai(); }
                        } else if ui.add_enabled(self.app_mode == AppMode::Sorting, egui::Button::new("启动 AI 分析")).clicked() {
                            if self.config.ai_stream_idle_timeout_seconds < 600 {
                                self.config.ai_stream_idle_timeout_seconds = 600;
                                self.config.save();
                                self.ai_log.push("程序：已将 AI 超时自动提升到 600 秒，避免本地模型首 token 过慢导致误判通信失败。".to_string());
                            }
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
                egui::ScrollArea::vertical()
                    .stick_to_bottom(true)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if self.ai_log_records.is_empty() {
                            ui.label(RichText::new("AI 实时分析日志会显示在这里。" ).color(Color32::from_gray(140)));
                            return;
                        }
                        for record in &self.ai_log_records {
                            let fill = if record.is_active { Color32::from_rgb(18, 34, 54) } else { Color32::from_rgb(18, 25, 36) };
                            let stroke = if record.is_active { Color32::from_rgb(75, 125, 190) } else { Color32::from_rgb(35, 62, 95) };
                            egui::Frame::none()
                                .fill(fill)
                                .stroke(egui::Stroke::new(1.0, stroke))
                                .corner_radius(egui::CornerRadius::same(4))
                                .inner_margin(egui::Margin::same(8))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new(&record.title).strong().color(Color32::from_rgb(205, 230, 255)));
                                        if record.is_active {
                                            ui.label(RichText::new("当前输出中").small().color(Color32::YELLOW));
                                        }
                                    });
                                    ui.separator();
                                    if record.lines.is_empty() {
                                        ui.label(RichText::new("等待日志..." ).color(Color32::from_gray(140)));
                                    } else {
                                        for line in &record.lines {
                                            ui.label(RichText::new(line).color(Color32::from_gray(225)));
                                        }
                                    }
                                });
                            ui.add_space(8.0);
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
                    if ui.button("继续取消").clicked() { self.ai_confirm_cancel = false; self.force_stop_ai_service_and_port(); }
                    if ui.button("返回").clicked() { self.ai_confirm_cancel = false; }
                });
            });
    }
}
