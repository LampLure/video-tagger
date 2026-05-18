use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};

use crate::config::cache_dir;

pub struct ScreenshotCache {
    base_dir: PathBuf,
    max_size_mb: u64,
    current_size: u64,
    lru_order: VecDeque<String>, // video hash, most recently used at back
    video_cache: HashMap<String, VideoCacheEntry>,
}

#[derive(Debug)]
struct VideoCacheEntry {
    screenshot_paths: HashMap<String, Vec<PathBuf>>, // range_start -> [paths]
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

    pub fn get_or_extract_screenshots(
        &mut self,
        video_path: &Path,
        start_sec: f64,
        interval: f64,
        count: usize,
        video_duration: f64,
    ) -> Vec<PathBuf> {
        let hash = Self::video_hash(video_path);
        let range_key = format!("{}", (start_sec * 10.0) as u64);

        // Check cache
        let cached_paths: Option<Vec<PathBuf>> = {
            if let Some(entry) = self.video_cache.get(&hash) {
                entry.screenshot_paths.get(&range_key).cloned()
            } else {
                None
            }
        };

        if let Some(paths) = cached_paths {
            self.touch_lru(&hash);
            return paths;
        }

        // Extract via ffmpeg
        let effective_interval = if start_sec + (count as f64) * interval > video_duration {
            video_duration / count as f64
        } else {
            interval
        };

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

        // Compute size
        let mut total_size: u64 = 0;
        for p in &paths {
            if let Ok(meta) = std::fs::metadata(p) {
                total_size += meta.len();
            }
        }

        // Store in cache
        let entry = self.video_cache.entry(hash.clone()).or_insert(VideoCacheEntry {
            screenshot_paths: HashMap::new(),
            total_size: 0,
        });

        let old_size = entry.total_size;
        entry.screenshot_paths.insert(range_key, paths.clone());
        entry.total_size = total_size;
        self.current_size = self.current_size.saturating_sub(old_size);
        self.current_size += total_size;
        self.touch_lru(&hash);

        // Evict if over limit
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
                    // Delete cache files
                    let dir = self.video_dir(&hash);
                    let _ = std::fs::remove_dir_all(&dir);
                }
            }
        }
    }

    pub fn clear(&mut self) {
        self.video_cache.clear();
        self.lru_order.clear();
        self.current_size = 0;
        let _ = std::fs::remove_dir_all(&self.base_dir);
        let _ = std::fs::create_dir_all(&self.base_dir);
    }
}
