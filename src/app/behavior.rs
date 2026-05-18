use super::*;

impl VideoTaggerApp {
    pub(super) fn processed_count(&self) -> usize {
        let identifier = self.folder_progress.as_ref().map(|p| p.identifier.as_str()).unwrap_or("");
        self.videos
            .iter()
            .filter(|v| config::parse_video_name(&v.filename).map(|p| p.identifier == identifier).unwrap_or(false))
            .count()
    }

    pub(super) fn is_processed(&self, index: usize) -> bool {
        let identifier = self.folder_progress.as_ref().map(|p| p.identifier.as_str()).unwrap_or("");
        self.videos
            .get(index)
            .and_then(|v| config::parse_video_name(&v.filename))
            .map(|p| p.identifier == identifier)
            .unwrap_or(false)
    }

    pub(super) fn current_effective_interval(&self) -> f64 {
        let duration = self.videos.get(self.current_video_index).and_then(|v| v.duration_secs).unwrap_or(0.0);
        let remaining = (duration - self.screenshot_start_sec).max(0.0);
        if duration > 0.0 && (duration < self.screenshot_interval * 10.0 || remaining < self.screenshot_interval * 10.0) {
            (remaining / 10.0).max(0.1)
        } else {
            self.screenshot_interval.max(0.1)
        }
    }

    fn screenshot_range_key(&self, video_index: usize, start_sec: f64) -> String {
        let path_key = self
            .videos
            .get(video_index)
            .map(|v| ScreenshotCache::video_hash(&v.path))
            .unwrap_or_else(|| "missing".to_string());
        format!("{}:{}:{}", path_key, (start_sec * 10.0) as u64, (self.screenshot_interval * 1000.0) as u64)
    }

    fn extract_screenshot_range(video_path: PathBuf, duration: f64, start_sec: f64, interval: f64) -> Result<Vec<PathBuf>, String> {
        let hash = ScreenshotCache::video_hash(&video_path);
        let output_dir = config::cache_dir().join("screens").join(hash);
        let count = 10usize;
        let remaining = (duration - start_sec).max(0.0);
        let effective_interval = if duration > 0.0 && (duration < interval * count as f64 || remaining < interval * count as f64) {
            (remaining / count as f64).max(0.1)
        } else {
            interval.max(0.1)
        };
        let prefix = format!("r{}_i{}", (start_sec * 10.0) as u64, (effective_interval * 1000.0) as u64);
        ffmpeg::extract_screenshots(&video_path, start_sec, effective_interval, count, &output_dir, &prefix)
    }

    pub(super) fn category_count_with_star(&self) -> usize {
        self.tag_library.category_count() + 1
    }

    pub(super) fn is_star_category(&self) -> bool {
        self.active_category_index >= self.tag_library.category_count()
    }

    pub(super) fn current_category_tags(&self) -> Vec<String> {
        if self.is_star_category() {
            vec!["不打星".to_string(), "星标".to_string()]
        } else {
            self.tag_library.category_names_for_display(self.active_category_index, self.config.tag_position_lock)
        }
    }

    fn clamp_category_cursor(&mut self) {
        let max_category = self.category_count_with_star().saturating_sub(1);
        self.active_category_index = self.active_category_index.min(max_category);
        let tag_count = self.current_category_tags().len().max(1);
        self.selected_tag_index = self.selected_tag_index.min(tag_count.saturating_sub(1));
        self.selected_screenshot_index = self.selected_screenshot_index.min(9);
    }

    pub(super) fn select_category(&mut self, category_index: usize) {
        self.active_category_index = category_index.min(self.category_count_with_star().saturating_sub(1));
        if self.is_star_category() {
            self.selected_tag_index = if self.is_starred { 1 } else { 0 };
        } else {
            self.selected_tag_index = 0;
            if let Some(existing) = self.current_labels.get(self.active_category_index) {
                let tags = self.current_category_tags();
                if let Some(pos) = tags.iter().position(|tag| tag == existing) {
                    self.selected_tag_index = pos;
                }
            }
        }
        self.clamp_category_cursor();
    }

    fn set_current_category_label(&mut self, label: String) {
        if self.is_star_category() { return; }
        let idx = self.active_category_index;
        if self.current_labels.iter().enumerate().any(|(i, existing)| i != idx && existing == &label) {
            return;
        }
        if idx < self.current_labels.len() {
            self.current_labels[idx] = label;
        } else {
            self.current_labels.push(label);
        }
        self.undone_labels.clear();
    }

    pub(super) fn reset_edit_state(&mut self) {
        self.current_labels.clear();
        self.undone_labels.clear();
        self.is_starred = false;
        self.pending_overwrite_once = false;
        self.active_category_index = 0;
        self.selected_tag_index = 0;
        self.selected_screenshot_index = 0;
        self.playing_screenshot = None;
        self.screenshot_error = None;
        self.screenshot_loading = false;
        self.audio_player.stop();
    }

    pub(super) fn hydrate_labels_from_filename(&mut self) {
        self.current_labels.clear();
        self.undone_labels.clear();
        self.is_starred = false;
        if let Some(video) = self.videos.get(self.current_video_index) {
            if let Some(parsed) = config::parse_video_name(&video.filename) {
                for label in parsed.labels {
                    if !self.current_labels.iter().any(|existing| existing == &label) {
                        self.current_labels.push(label);
                    }
                }
                self.is_starred = parsed.starred;
            }
        }
        self.select_category(self.current_labels.len().min(self.tag_library.category_count()));
    }

    pub(super) fn begin_edit_video(&mut self, index: usize, independent: bool) {
        if index >= self.videos.len() { return; }
        self.current_video_index = index;
        self.screenshot_interval = self.config.screenshot_interval;
        self.screenshot_textures.clear();
        self.reset_edit_state();
        self.load_current_screenshots();
        self.hydrate_labels_from_filename();
        self.independent_edit = if independent { Some(index) } else { None };
        self.app_mode = AppMode::Sorting;
    }

    pub(super) fn clear_thumbnail_state(&mut self) {
        self.overview_thumbnails.clear();
        self.thumbnail_queue.clear();
        self.thumbnail_loaded.clear();
        self.thumbnail_errors.clear();
        self.thumbnail_inflight.clear();
    }

    pub(super) fn pick_folder(&mut self) {
        if let Some(path) = rfd::FileDialog::new().pick_folder() {
            self.selected_folder = Some(path);
            self.app_mode = AppMode::Fresh;
            self.videos.clear();
            self.folder_progress = None;
            self.clear_thumbnail_state();
        }
    }

    pub(super) fn enter_overview(&mut self) {
        let Some(folder) = self.selected_folder.clone() else { return; };
        self.videos = scanner::scan_videos(&folder);
        self.clear_thumbnail_state();
        self.overview_search.clear();
        self.folder_progress = Some(progress::init_progress_for_folder(&folder, &self.videos));
        if let Some(ref prog) = self.folder_progress { progress::save_progress(&folder, prog); }
        self.app_mode = AppMode::Overview;
    }

    pub(super) fn enter_sorting(&mut self) {
        if self.videos.is_empty() { return; }
        let start_idx = self.folder_progress.as_ref().map(|p| p.last_processed).unwrap_or(0);
        self.begin_edit_video(start_idx.min(self.videos.len().saturating_sub(1)), false);
    }

    pub(super) fn exit_sorting(&mut self) {
        self.reset_edit_state();
        self.independent_edit = None;
        self.app_mode = AppMode::Overview;
    }

    pub(super) fn sorted_filtered_indices(&self) -> Vec<usize> {
        let search_lower = self.overview_search.to_lowercase();
        let mut filtered: Vec<usize> = self.videos
            .iter()
            .enumerate()
            .filter(|(_, v)| search_lower.is_empty() || v.filename.to_lowercase().contains(&search_lower))
            .map(|(i, _)| i)
            .collect();

        filtered.sort_by(|&a, &b| {
            let va = &self.videos[a];
            let vb = &self.videos[b];
            match self.overview_sort {
                SortMode::Name => va.filename.to_lowercase().cmp(&vb.filename.to_lowercase()),
                SortMode::Size => vb.size.cmp(&va.size),
                SortMode::Date => {
                    let ma = std::fs::metadata(&va.path).and_then(|m| m.modified()).ok();
                    let mb = std::fs::metadata(&vb.path).and_then(|m| m.modified()).ok();
                    mb.cmp(&ma).then_with(|| va.filename.to_lowercase().cmp(&vb.filename.to_lowercase()))
                }
            }
        });
        filtered
    }

    pub(super) fn prioritize_overview_thumbnails(&mut self, indices: &[usize]) {
        for &idx in indices.iter().rev() {
            if self.overview_thumbnails.contains_key(&idx)
                || self.thumbnail_loaded.contains(&idx)
                || self.thumbnail_errors.contains_key(&idx)
                || self.thumbnail_inflight.contains(&idx)
            { continue; }
            self.thumbnail_queue.retain(|queued| *queued != idx);
            self.thumbnail_queue.push_front(idx);
        }
    }

    pub(super) fn load_current_screenshots(&mut self) {
        if self.videos.is_empty() { return; }
        self.screenshot_start_sec = 0.0;
        self.request_current_screenshots();
    }

    pub(super) fn request_current_screenshots(&mut self) {
        if self.videos.is_empty() { return; }
        let key = self.screenshot_range_key(self.current_video_index, self.screenshot_start_sec);
        if let Some(paths) = self.screenshot_cached_ranges.get(&key).cloned() {
            self.screenshot_loading = false;
            self.screenshot_error = None;
            self.screenshot_paths = paths;
            self.screenshot_textures.clear();
            self.prefetch_adjacent_screenshot_ranges();
            return;
        }

        let video_path = self.videos[self.current_video_index].path.clone();
        let duration = self.videos[self.current_video_index].ensure_duration();
        let start_sec = self.screenshot_start_sec;
        let interval = self.screenshot_interval;
        let request_id = self.screenshot_request_id.wrapping_add(1);
        self.screenshot_request_id = request_id;
        self.screenshot_loading = true;
        self.screenshot_error = None;
        if self.screenshot_paths.is_empty() { self.screenshot_textures.clear(); }

        let tx = self.screenshot_tx.clone();
        let key_for_thread = key.clone();
        std::thread::spawn(move || {
            let result = Self::extract_screenshot_range(video_path, duration, start_sec, interval);
            let _ = match result {
                Ok(paths) if !paths.is_empty() => tx.send(ScreenshotResult::Loaded { request_id, key: key_for_thread, paths }),
                Ok(_) => tx.send(ScreenshotResult::Failed { request_id, reason: "ffmpeg 截图为空".to_string() }),
                Err(reason) => tx.send(ScreenshotResult::Failed { request_id, reason }),
            };
        });
    }

    pub(super) fn prefetch_adjacent_screenshot_ranges(&mut self) {
        if self.videos.is_empty() { return; }
        let duration = self.videos[self.current_video_index].duration_secs.unwrap_or(0.0);
        let step = self.screenshot_interval * 10.0;
        let max_start = if duration <= step { 0.0 } else { ((duration - 0.001) / step).floor() * step };
        let candidates = [self.screenshot_start_sec + step, self.screenshot_start_sec + step * 2.0, self.screenshot_start_sec - step];
        for start in candidates {
            if start < 0.0 || start > max_start { continue; }
            let key = self.screenshot_range_key(self.current_video_index, start);
            if self.screenshot_cached_ranges.contains_key(&key) || self.screenshot_prefetching.contains(&key) { continue; }
            self.screenshot_prefetching.insert(key.clone());
            let video_path = self.videos[self.current_video_index].path.clone();
            let interval = self.screenshot_interval;
            let tx = self.screenshot_tx.clone();
            let key_for_thread = key.clone();
            std::thread::spawn(move || {
                let result = Self::extract_screenshot_range(video_path, duration, start, interval);
                if let Ok(paths) = result {
                    if !paths.is_empty() {
                        let _ = tx.send(ScreenshotResult::Prefetched { key: key_for_thread, paths });
                    }
                }
            });
        }
    }

    pub(super) fn advance_screenshots(&mut self, backward: bool) {
        if self.videos.is_empty() { return; }
        let duration = self.videos[self.current_video_index].ensure_duration();
        let step = self.screenshot_interval * 10.0;
        if backward {
            self.screenshot_start_sec = (self.screenshot_start_sec - step).max(0.0);
        } else {
            let max_start = if duration <= step { 0.0 } else { ((duration - 0.001) / step).floor() * step };
            self.screenshot_start_sec = (self.screenshot_start_sec + step).min(max_start);
        }
        self.request_current_screenshots();
    }

    pub(super) fn add_label(&mut self, label: String) {
        self.set_current_category_label(label);
    }

    pub(super) fn undo_label(&mut self) {
        if self.is_starred && self.is_star_category() {
            self.is_starred = false;
            self.selected_tag_index = 0;
            return;
        }
        if let Some(label) = self.current_labels.pop() {
            self.undone_labels.push(label);
            self.select_category(self.current_labels.len().min(self.tag_library.category_count()));
        }
    }

    pub(super) fn redo_label(&mut self) {
        if let Some(label) = self.undone_labels.pop() {
            self.set_current_category_label(label);
        }
    }

    pub(super) fn finish_new_tag(&mut self) {
        if !self.new_tag_text.trim().is_empty() && !self.is_star_category() {
            self.tag_library.add_tag_to_category(self.active_category_index, &self.new_tag_text);
            self.tag_library.save();
            self.new_tag_text.clear();
        }
        self.editing_new_tag = false;
    }

    pub(super) fn finish_new_category(&mut self) {
        if !self.new_category_text.trim().is_empty() {
            if self.tag_library.add_category(&self.new_category_text) {
                self.tag_library.save();
            }
            self.new_category_text.clear();
        }
        self.editing_new_category = false;
    }

    pub(super) fn select_current_tag_and_advance(&mut self) {
        self.clamp_category_cursor();
        if self.is_star_category() {
            self.is_starred = self.selected_tag_index == 1;
            self.finalize_current_video();
            return;
        }

        let tags = self.current_category_tags();
        if let Some(label) = tags.get(self.selected_tag_index).cloned() {
            self.set_current_category_label(label);
        }
        self.select_category((self.active_category_index + 1).min(self.tag_library.category_count()));
    }

    pub(super) fn play_selected_screenshot_audio(&mut self) {
        if self.videos.is_empty() { return; }
        let idx = self.selected_screenshot_index.min(self.screenshot_paths.len().saturating_sub(1));
        let time_sec = self.screenshot_start_sec + idx as f64 * self.current_effective_interval();
        self.audio_player.play_clip(&self.videos[self.current_video_index].path, time_sec);
        self.playing_screenshot = Some(idx);
    }

    pub(super) fn skip_current_video(&mut self) {
        if self.videos.is_empty() { return; }
        let independent = self.independent_edit.is_some();
        let next_idx = self.current_video_index + 1;
        if independent {
            self.reset_edit_state();
            self.independent_edit = None;
            self.app_mode = AppMode::Overview;
            return;
        }
        if next_idx >= self.videos.len() {
            self.reset_edit_state();
            self.app_mode = AppMode::Overview;
            self.show_completion = true;
            return;
        }
        self.begin_edit_video(next_idx, false);
    }

    pub(super) fn finalize_current_video(&mut self) {
        if self.videos.is_empty() { return; }
        let video = self.videos[self.current_video_index].clone();
        let labels: Vec<String> = self.current_labels.iter().filter(|label| !label.trim().is_empty()).cloned().collect();
        let should_rename = !labels.is_empty() || self.is_starred;

        if let Some(ref prog) = self.folder_progress {
            let overwrite = self.config.shift_lock || self.pending_overwrite_once;
            let new_name = config::format_video_name(&prog.identifier, self.current_video_index, prog.digit_count, &labels, self.is_starred, &video.filename, &video.extension, overwrite);
            let parent = video.path.parent().unwrap_or(std::path::Path::new("."));
            let mut final_path = parent.join(&new_name);

            if should_rename {
                scanner::resolve_name_conflict(&mut final_path);
                if std::fs::rename(&video.path, &final_path).is_ok() {
                    if let Some(updated) = self.videos.get_mut(self.current_video_index) {
                        updated.path = final_path.clone();
                        updated.filename = final_path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or(new_name);
                        updated.extension = final_path.extension().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| video.extension.clone());
                    }
                    self.overview_thumbnails.remove(&self.current_video_index);
                    self.thumbnail_loaded.remove(&self.current_video_index);
                    self.thumbnail_errors.remove(&self.current_video_index);
                    self.thumbnail_inflight.remove(&self.current_video_index);
                }
            }
            if !labels.is_empty() {
                self.tag_library.record_usage(&labels);
                self.tag_library.save();
            }
        }

        if let Some(ref mut prog) = self.folder_progress {
            prog.last_processed = prog.last_processed.max(self.current_video_index + 1);
            if let Some(ref folder) = self.selected_folder { progress::save_progress(folder, prog); }
        }

        let independent = self.independent_edit.is_some();
        let next_idx = self.current_video_index + 1;
        if independent {
            self.reset_edit_state();
            self.independent_edit = None;
            self.app_mode = AppMode::Overview;
            return;
        }
        if next_idx >= self.videos.len() {
            self.reset_edit_state();
            self.app_mode = AppMode::Overview;
            self.show_completion = true;
            return;
        }
        self.begin_edit_video(next_idx, false);
    }

    pub(super) fn handle_keyboard_input(&mut self, ctx: &egui::Context) {
        let input = ctx.input(|i| i.clone());
        if self.editing_new_tag || self.editing_new_category {
            if input.key_pressed(egui::Key::Enter) || input.key_pressed(egui::Key::Space) {
                if self.editing_new_tag { self.finish_new_tag(); }
                if self.editing_new_category { self.finish_new_category(); }
            }
            return;
        }

        if input.key_pressed(egui::Key::Q) { self.advance_screenshots(true); }
        if input.key_pressed(egui::Key::E) { self.advance_screenshots(false); }
        if input.key_pressed(egui::Key::A) { if self.selected_screenshot_index % 5 > 0 { self.selected_screenshot_index -= 1; } }
        if input.key_pressed(egui::Key::D) { if self.selected_screenshot_index % 5 < 4 { self.selected_screenshot_index = (self.selected_screenshot_index + 1).min(9); } }
        if input.key_pressed(egui::Key::W) { self.selected_screenshot_index = self.selected_screenshot_index.saturating_sub(5); }
        if input.key_pressed(egui::Key::S) { self.selected_screenshot_index = (self.selected_screenshot_index + 5).min(9); }
        if input.key_pressed(egui::Key::X) { self.play_selected_screenshot_audio(); }

        if input.key_pressed(egui::Key::Delete) { self.undo_label(); }
        if input.key_pressed(egui::Key::ArrowUp) { self.select_category(self.active_category_index.saturating_sub(1)); }
        if input.key_pressed(egui::Key::ArrowDown) { self.select_category((self.active_category_index + 1).min(self.category_count_with_star().saturating_sub(1))); }
        if input.key_pressed(egui::Key::ArrowLeft) { self.selected_tag_index = self.selected_tag_index.saturating_sub(1); }
        if input.key_pressed(egui::Key::ArrowRight) {
            let max = self.current_category_tags().len().saturating_sub(1);
            self.selected_tag_index = (self.selected_tag_index + 1).min(max);
        }

        let num_keys = [egui::Key::Num1, egui::Key::Num2, egui::Key::Num3, egui::Key::Num4, egui::Key::Num5, egui::Key::Num6, egui::Key::Num7, egui::Key::Num8, egui::Key::Num9];
        for (n, key) in num_keys.iter().enumerate() {
            if input.key_pressed(*key) {
                if n < self.current_category_tags().len() {
                    self.selected_tag_index = n;
                    self.select_current_tag_and_advance();
                    return;
                }
            }
        }
        if input.key_pressed(egui::Key::Space) || input.key_pressed(egui::Key::Enter) {
            self.select_current_tag_and_advance();
        }
    }
}
