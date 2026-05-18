use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};

use crate::config::cache_dir;

pub struct ScreenshotCache {
    base_dir: PathBuf,
    max_size_mb: u64,
    current_size: u64,
    lru_order: VecDeque<String>,
    video_cache: HashMap<String, VideoCacheEntry>,
}

#[derive(Debug)]
struct VideoCacheEntry {
    screenshot_paths: HashMap<String, Vec<PathBuf>>,
    total_size: u64,
}

impl ScreenshotCache {
    pub fn new(max_size_mb: u64) -> Self {
        let base_dir = cache_dir();
        let _ = std::fs::create_dir_all(&base_dir);

        ScreenshotCache {
            base_dir,
            max_size_mb,
            current_size: 0,
            lru_order: VecDeque::new(),
            video_cache: HashMap::new(),
        }
    }

    pub fn video_hash(path: &Path) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        path.to_string_lossy().to_string().hash(&mut h);
        format!("{:016x}", h.finish())
    }

    pub fn video_dir(&self, hash: &str) -> PathBuf {
        self.base_dir.join(hash)
    }

    fn effective_interval(start_sec: f64, requested_interval: f64, count: usize, video_duration: f64) -> f64 {
        let count = count.max(1) as f64;
        if video_duration <= 0.0 {
            return requested_interval.max(0.1);
        }
        let remaining = (video_duration - start_sec).max(0.0);
        if video_duration < requested_interval * count || remaining < requested_interval * count {
            (remaining / count).max(0.1)
        } else {
            requested_interval.max(0.1)
        }
    }

    pub fn get_or_extract_screenshots(
        &mut self,
        video_path: &Path,
        start_sec: f64,
        interval: f64,
        count: usize,
        video_duration: f64,
    ) -> Vec<PathBuf> {
        let hash = Self::video_hash(video_path);
        let effective_interval = Self::effective_interval(start_sec, interval, count, video_duration);
        let range_key = format!("{}_{}", (start_sec * 10.0) as u64, (effective_interval * 1000.0) as u64);

        let cached_paths = self
            .video_cache
            .get(&hash)
            .and_then(|entry| entry.screenshot_paths.get(&range_key).cloned());

        if let Some(paths) = cached_paths {
            self.touch_lru(&hash);
            return paths;
        }

        let dir = self.video_dir(&hash);
        let paths = crate::ffmpeg::extract_screenshots(
            video_path,
            start_sec,
            effective_interval,
            count,
            &dir,
            &range_key,
        )
        .unwrap_or_default();

        let entry = self.video_cache.entry(hash.clone()).or_insert(VideoCacheEntry {
            screenshot_paths: HashMap::new(),
            total_size: 0,
        });

        entry.screenshot_paths.insert(range_key, paths.clone());
        entry.total_size = entry
            .screenshot_paths
            .values()
            .flat_map(|paths| paths.iter())
            .filter_map(|p| std::fs::metadata(p).ok().map(|m| m.len()))
            .sum();

        self.current_size = self.video_cache.values().map(|entry| entry.total_size).sum();
        self.touch_lru(&hash);
        self.evict_excess();

        paths
    }

    fn touch_lru(&mut self, hash: &str) {
        self.lru_order.retain(|h| h != hash);
        self.lru_order.push_back(hash.to_string());
    }

    fn evict_excess(&mut self) {
        let max_bytes = self.max_size_mb * 1024 * 1024;
        while self.current_size > max_bytes && !self.lru_order.is_empty() {
            if let Some(hash) = self.lru_order.pop_front() {
                if let Some(entry) = self.video_cache.remove(&hash) {
                    self.current_size = self.current_size.saturating_sub(entry.total_size);
                    let dir = self.video_dir(&hash);
                    let _ = std::fs::remove_dir_all(&dir);
                }
            }
        }
    }
}
