use super::*;

impl VideoTaggerApp {
    pub(super) fn refresh_ai_scripts(&mut self) {
        self.ai_scripts = ai::list_model_scripts();
        if self.ai_selected_script >= self.ai_scripts.len() {
            self.ai_selected_script = 0;
        }
    }

    pub(super) fn ai_text_settings_path(&self) -> PathBuf {
        config::app_data_dir().join("ai_text_settings.json")
    }

    pub(super) fn ensure_ai_text_settings_file(&mut self) -> Result<PathBuf, String> {
        let path = self.ai_text_settings_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        if !path.exists() {
            let text = if self.config.ai_text_settings.trim().is_empty() {
                ai::default_ai_text_settings()
            } else {
                self.config.ai_text_settings.clone()
            };
            std::fs::write(&path, text).map_err(|e| e.to_string())?;
        }
        Ok(path)
    }

    pub(super) fn load_ai_text_settings_from_file(&mut self) -> Result<(), String> {
        let path = self.ensure_ai_text_settings_file()?;
        let text = std::fs::read_to_string(&path).map_err(|e| format!("读取 AI 文本设置失败: {}", e))?;
        ai::validate_ai_text_settings(&text)?;
        self.config.ai_text_settings = text;
        self.config.save();
        Ok(())
    }

    pub(super) fn open_ai_text_settings_file(&mut self) -> Result<(), String> {
        let path = self.ensure_ai_text_settings_file()?;
        #[cfg(target_os = "windows")]
        let mut cmd = {
            let mut c = std::process::Command::new("cmd");
            c.arg("/C").arg("start").arg("").arg(&path);
            c
        };
        #[cfg(target_os = "macos")]
        let mut cmd = {
            let mut c = std::process::Command::new("open");
            c.arg(&path);
            c
        };
        #[cfg(all(unix, not(target_os = "macos")))]
        let mut cmd = {
            let mut c = std::process::Command::new("xdg-open");
            c.arg(&path);
            c
        };
        cmd.spawn().map_err(|e| format!("打开 AI 文本设置文件失败: {}", e))?;
        Ok(())
    }

    pub(super) fn reset_ai_text_settings_file(&mut self) -> Result<(), String> {
        let path = self.ai_text_settings_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let text = ai::default_ai_text_settings();
        std::fs::write(&path, &text).map_err(|e| e.to_string())?;
        self.config.ai_text_settings = text;
        self.config.save();
        Ok(())
    }

    pub(super) fn refresh_ai_props(&mut self) {
        match ai::probe_llama_props(Duration::from_secs(3)) {
            Ok(props) => {
                let vision = props.vision;
                let audio = props.audio;
                self.ai_service_props = Some(props);
                if self.ai_service_state == AiServiceState::Disconnected {
                    self.ai_service_state = if self.ai_model_process.is_some() { AiServiceState::ConnectedOwned } else { AiServiceState::ConnectedExternal };
                }
                self.ai_notice = Some(format!("AI 能力已刷新：视觉{}，音频{}。", if vision { "可用" } else { "不可用" }, if audio { "可用" } else { "不可用" }));
            }
            Err(err) => {
                self.ai_notice = Some(format!("刷新 AI 能力失败：{}", err));
            }
        }
    }

    pub(super) fn ai_runtime_config(&self) -> AiRuntimeConfig {
        AiRuntimeConfig {
            image_max_pixels: self.config.ai_image_max_pixels,
            jpeg_quality: self.config.ai_jpeg_quality,
            audio_sample_rate: self.config.ai_audio_sample_rate,
            audio_clip_seconds: self.config.ai_audio_clip_seconds,
            audio_clips_per_batch: self.config.ai_audio_clips_per_batch,
            max_extra_sample_batches: self.config.ai_max_extra_sample_batches,
            stream_idle_timeout_seconds: self.config.ai_stream_idle_timeout_seconds,
        }
    }

    pub(super) fn start_ai_model_service(&mut self) {
        if self.ai_service_state == AiServiceState::Starting || self.ai_service_state == AiServiceState::ConnectedOwned {
            return;
        }
        if let Ok(props) = ai::probe_llama_props(Duration::from_secs(2)) {
            self.ai_service_props = Some(props);
            self.ai_service_state = AiServiceState::ConnectedExternal;
            self.ai_notice = Some("检测到 7080 已有服务运行。请确认它是目标 llama.cpp 服务；程序不会关闭此外部服务。".to_string());
            return;
        }
        self.refresh_ai_scripts();
        let Some(script) = self.ai_scripts.get(self.ai_selected_script).cloned() else {
            self.ai_notice = Some("未找到模型启动脚本，请将 .bat/.cmd 或 .sh 放入 models 文件夹。".to_string());
            return;
        };
        self.ai_service_state = AiServiceState::Starting;
        match ai::start_model_script(&script) {
            Ok(child) => {
                self.ai_model_process = Some(child);
                let start = std::time::Instant::now();
                let mut last_props: Option<AiServiceProps> = None;
                loop {
                    match ai::probe_llama_props(Duration::from_secs(1)) {
                        Ok(props) => {
                            if props.vision || props.audio || start.elapsed() >= Duration::from_secs(10) {
                                self.ai_service_props = Some(props);
                                self.ai_service_state = AiServiceState::ConnectedOwned;
                                self.ai_notice = Some("AI 服务已连接。".to_string());
                                return;
                            }
                            last_props = Some(props);
                            std::thread::sleep(Duration::from_millis(500));
                        }
                        Err(_) if start.elapsed() < Duration::from_secs(10) => {
                            std::thread::sleep(Duration::from_millis(250));
                        }
                        Err(_) => {
                            if let Some(props) = last_props {
                                self.ai_service_props = Some(props);
                                self.ai_service_state = AiServiceState::ConnectedOwned;
                                self.ai_notice = Some("AI 服务已连接，但模型能力可能仍在加载。请稍后点击刷新能力。".to_string());
                                return;
                            }
                            self.ai_service_state = AiServiceState::Disconnected;
                            self.ai_notice = Some("AI 服务未连接成功，可能模型仍在加载、启动脚本错误，或端口不是 7080。".to_string());
                            return;
                        }
                    }
                }
            }
            Err(err) => {
                self.ai_service_state = AiServiceState::Disconnected;
                self.ai_notice = Some(err);
            }
        }
    }

    pub(super) fn stop_ai_model_service(&mut self) {
        if let Some(mut child) = self.ai_model_process.take() {
            ai::stop_process_tree(&mut child);
        }
        self.ai_service_state = AiServiceState::Disconnected;
        self.ai_service_props = None;
        self.ai_batch_state = AiBatchState::Idle;
        self.ai_pending_result = None;
        self.ai_notice = Some("AI 模型服务已停止。".to_string());
    }

    pub(super) fn start_ai_batch(&mut self) {
        if self.app_mode != AppMode::Sorting || self.videos.is_empty() {
            self.ai_notice = Some("请先进入分拣模式。".to_string());
            return;
        }
        if self.ai_batch_state == AiBatchState::Running {
            return;
        }
        if !matches!(self.ai_service_state, AiServiceState::ConnectedOwned | AiServiceState::ConnectedExternal) {
            self.ai_notice = Some("请先启动并连接 AI 模型服务。".to_string());
            return;
        }
        if let Err(err) = self.load_ai_text_settings_from_file() {
            self.ai_notice = Some(err);
            return;
        }
        let Some(props) = self.ai_service_props.clone() else {
            self.ai_notice = Some("AI 服务状态未知，请重新检测或启动模型服务。".to_string());
            return;
        };
        if !props.vision {
            self.ai_notice = Some("当前模型不支持图片输入，无法进行视频 AI 打标签。请稍后点击刷新能力；如果仍不可用，请检查 /props。".to_string());
            return;
        }
        if let Err(err) = ai::validate_ai_text_settings(&self.config.ai_text_settings) {
            self.ai_notice = Some(err);
            return;
        }
        self.ai_failures.clear();
        self.ai_success_count = 0;
        self.ai_pending_result = None;
        self.start_current_ai_video();
    }

    pub(super) fn start_current_ai_video(&mut self) {
        if self.videos.is_empty() || self.current_video_index >= self.videos.len() {
            return;
        }
        let Some(props) = self.ai_service_props.clone() else {
            self.ai_notice = Some("AI 服务未连接。".to_string());
            return;
        };
        self.ai_work_id = self.ai_work_id.wrapping_add(1);
        self.ai_log.clear();
        self.ai_pending_result = None;
        self.ai_batch_state = AiBatchState::Running;
        self.screenshot_textures.clear();
        self.ai_log.push(format!("AI：准备分析第 {} 个视频。", self.current_video_index + 1));
        if !props.audio && self.config.ai_audio_clips_per_batch > 0 {
            self.ai_log.push("AI：当前模型不支持音频，本视频仅基于画面分析。".to_string());
        }
        let job = AiVideoJob {
            video: self.videos[self.current_video_index].clone(),
            work_id: self.ai_work_id,
            text_settings_json: self.config.ai_text_settings.clone(),
            runtime: self.ai_runtime_config(),
            allow_audio: props.audio && self.config.ai_audio_clips_per_batch > 0,
        };
        ai::spawn_video_analysis(job, self.ai_tx.clone());
    }

    pub(super) fn request_cancel_ai(&mut self) {
        if self.ai_batch_state == AiBatchState::Running || self.ai_batch_state == AiBatchState::AwaitingConfirmation {
            self.ai_confirm_cancel = true;
        }
    }

    pub(super) fn cancel_ai_now(&mut self) {
        if let Some(mut child) = self.ai_model_process.take() {
            ai::stop_process_tree(&mut child);
        }
        self.ai_batch_state = AiBatchState::Idle;
        self.ai_service_state = AiServiceState::Disconnected;
        self.ai_service_props = None;
        self.ai_pending_result = None;
        self.ai_log.push("AI：已取消分析，并强制关闭模型服务。".to_string());
    }

    pub(super) fn accept_ai_pending_result(&mut self) {
        let Some(result) = self.ai_pending_result.clone() else { return; };
        self.current_labels = result.labels.clone();
        self.current_labels.push(ai::point_label(result.score));
        self.ai_pending_result = None;
        self.ai_batch_state = AiBatchState::Idle;
        self.finalize_current_video();
        self.ai_success_count += 1;
        if self.app_mode == AppMode::Sorting && self.current_video_index < self.videos.len() {
            self.start_current_ai_video();
        } else {
            self.finish_ai_batch_summary();
        }
    }

    pub(super) fn retry_ai_current_video(&mut self) {
        self.ai_pending_result = None;
        self.ai_log.clear();
        self.start_current_ai_video();
    }

    fn handle_ai_done(&mut self, result: AiAnalysisResult) {
        if self.config.ai_auto_accept {
            self.current_labels = result.labels.clone();
            self.current_labels.push(ai::point_label(result.score));
            self.ai_batch_state = AiBatchState::Idle;
            self.finalize_current_video();
            self.ai_success_count += 1;
            if self.app_mode == AppMode::Sorting && self.current_video_index < self.videos.len() {
                self.start_current_ai_video();
            } else {
                self.finish_ai_batch_summary();
            }
        } else {
            self.ai_pending_result = Some(result);
            self.ai_batch_state = AiBatchState::AwaitingConfirmation;
            self.ai_log.push("AI：等待确认。Space 接受，Delete 重新生成。".to_string());
        }
    }

    fn handle_ai_failed(&mut self, reason: String) {
        let filename = self.videos.get(self.current_video_index).map(|v| v.filename.clone()).unwrap_or_default();
        ai::save_raw_log("ai_failures.log", &format!("{}: {}\n", filename, reason));
        if self.config.ai_auto_accept {
            self.ai_failures.push(AiFailureRecord { filename, reason: reason.clone() });
            self.ai_log.push(format!("AI：当前视频分析失败，已跳过。原因：{}", reason));
            self.ai_batch_state = AiBatchState::Idle;
            self.skip_current_video();
            if self.app_mode == AppMode::Sorting && self.current_video_index < self.videos.len() {
                self.start_current_ai_video();
            } else {
                self.finish_ai_batch_summary();
            }
        } else {
            self.ai_batch_state = AiBatchState::Idle;
            self.ai_log.push(format!("AI：当前视频分析失败，已停止。原因：{}", reason));
        }
    }

    fn finish_ai_batch_summary(&mut self) {
        self.ai_batch_state = AiBatchState::Idle;
        let mut text = format!("AI 分析完成。成功：{} 个，失败：{} 个。", self.ai_success_count, self.ai_failures.len());
        if !self.ai_failures.is_empty() {
            text.push_str("\n失败文件：");
            for (i, item) in self.ai_failures.iter().enumerate() {
                text.push_str(&format!("\n{}. {}：{}", i + 1, item.filename, item.reason));
            }
        }
        self.ai_notice = Some(text);
    }

    pub(super) fn poll_ai_events(&mut self, ctx: &egui::Context) {
        let mut changed = false;
        if let Some(rx) = self.ai_rx.take() {
            loop {
                match rx.try_recv() {
                    Ok(AiEvent::Log { work_id, text }) if work_id == self.ai_work_id => {
                        self.ai_log.push(text);
                        changed = true;
                    }
                    Ok(AiEvent::Preview { work_id, paths, times: _ }) if work_id == self.ai_work_id => {
                        self.screenshot_paths = paths;
                        self.screenshot_textures.clear();
                        self.screenshot_loading = false;
                        self.screenshot_error = None;
                        changed = true;
                    }
                    Ok(AiEvent::Done { work_id, result }) if work_id == self.ai_work_id => {
                        self.handle_ai_done(result);
                        changed = true;
                    }
                    Ok(AiEvent::Failed { work_id, reason }) if work_id == self.ai_work_id => {
                        self.handle_ai_failed(reason);
                        changed = true;
                    }
                    Ok(_) => {}
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
                }
            }
            self.ai_rx = Some(rx);
        }
        if self.ai_batch_state == AiBatchState::Running || changed {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    pub(super) fn handle_ai_keyboard_input(&mut self, ctx: &egui::Context) -> bool {
        if !self.ai_mode {
            return false;
        }
        if self.ai_batch_state == AiBatchState::Running {
            return true;
        }
        if self.ai_batch_state == AiBatchState::AwaitingConfirmation {
            let input = ctx.input(|i| i.clone());
            if input.key_pressed(egui::Key::Space) || input.key_pressed(egui::Key::Enter) {
                self.accept_ai_pending_result();
            } else if input.key_pressed(egui::Key::Delete) {
                self.retry_ai_current_video();
            }
            return true;
        }
        false
    }
}