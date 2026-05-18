use serde::{Deserialize, Serialize};

use crate::config::sanitize_filename;

pub const MAX_TAG_CATEGORIES: usize = 8;
pub const MAX_TAGS_PER_CATEGORY: usize = 9;
pub const STAR_CATEGORY_NAME: &str = "星标";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagEntry {
    pub name: String,
    pub use_count: u64,
    pub last_used: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagCategory {
    pub name: String,
    #[serde(default)]
    pub entries: Vec<TagEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagLibrary {
    #[serde(default)]
    categories: Vec<TagCategory>,
    #[serde(default)]
    entries: Vec<TagEntry>,
}

impl TagLibrary {
    pub fn new() -> Self {
        Self {
            categories: Vec::new(),
            entries: Vec::new(),
        }
    }

    pub fn load() -> Self {
        let path = crate::config::tag_library_path();
        let mut library = if path.exists() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_else(Self::new)
        } else {
            Self::new()
        };
        library.normalize();
        library
    }

    fn normalize(&mut self) {
        if self.categories.is_empty() && !self.entries.is_empty() {
            self.categories.push(TagCategory {
                name: "类别1".to_string(),
                entries: std::mem::take(&mut self.entries),
            });
        }
        self.categories.truncate(MAX_TAG_CATEGORIES);
        for (idx, category) in self.categories.iter_mut().enumerate() {
            category.name = sanitize_filename(category.name.trim());
            if category.name.is_empty() {
                category.name = format!("类别{}", idx + 1);
            }
            category.entries.truncate(MAX_TAGS_PER_CATEGORY);
        }
    }

    pub fn save(&self) {
        let path = crate::config::tag_library_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(&path, json);
        }
    }

    pub fn categories(&self) -> &[TagCategory] { &self.categories }
    pub fn category_count(&self) -> usize { self.categories.len() }

    pub fn add_category(&mut self, name: &str) -> bool {
        if self.categories.len() >= MAX_TAG_CATEGORIES { return false; }
        let mut name = sanitize_filename(name.trim());
        if name.is_empty() { name = format!("类别{}", self.categories.len() + 1); }
        if self.categories.iter().any(|c| c.name == name) { return false; }
        self.categories.push(TagCategory { name, entries: Vec::new() });
        true
    }

    pub fn remove_category(&mut self, index: usize) {
        if index < self.categories.len() { self.categories.remove(index); }
    }

    pub fn rename_category(&mut self, index: usize, name: &str) {
        if let Some(category) = self.categories.get_mut(index) {
            let name = sanitize_filename(name.trim());
            if !name.is_empty() { category.name = name; }
        }
    }

    pub fn add_tag_to_category(&mut self, category_index: usize, name: &str) -> bool {
        let Some(category) = self.categories.get_mut(category_index) else { return false; };
        if category.entries.len() >= MAX_TAGS_PER_CATEGORY { return false; }
        let name = sanitize_filename(name.trim());
        if name.is_empty() || category.entries.iter().any(|e| e.name == name) { return false; }
        category.entries.push(TagEntry { name, use_count: 0, last_used: chrono::Utc::now() });
        true
    }

    pub fn remove_tag_from_category(&mut self, category_index: usize, name: &str) {
        if let Some(category) = self.categories.get_mut(category_index) {
            category.entries.retain(|entry| entry.name != name);
        }
    }

    pub fn category_names_for_display(&self, category_index: usize, locked: bool) -> Vec<String> {
        let Some(category) = self.categories.get(category_index) else { return Vec::new(); };
        if locked {
            category.entries.iter().map(|e| e.name.clone()).collect()
        } else {
            let now = chrono::Utc::now();
            let mut entries: Vec<&TagEntry> = category.entries.iter().collect();
            entries.sort_by(|a, b| {
                let score_a = decay_score(a, now);
                let score_b = decay_score(b, now);
                match score_b.partial_cmp(&score_a).unwrap_or(std::cmp::Ordering::Equal) {
                    std::cmp::Ordering::Equal => a.name.cmp(&b.name),
                    other => other,
                }
            });
            entries.into_iter().map(|e| e.name.clone()).collect()
        }
    }

    pub fn add_tag(&mut self, name: &str) {
        if self.categories.is_empty() { let _ = self.add_category("类别1"); }
        let _ = self.add_tag_to_category(0, name);
    }

    pub fn remove_tag(&mut self, name: &str) {
        for category in &mut self.categories { category.entries.retain(|entry| entry.name != name); }
    }

    pub fn record_usage(&mut self, names: &[String]) {
        let now = chrono::Utc::now();
        for name in names {
            for category in &mut self.categories {
                if let Some(entry) = category.entries.iter_mut().find(|e| e.name == *name) {
                    entry.use_count += 1;
                    entry.last_used = now;
                }
            }
        }
    }

    pub fn entries_in_saved_order(&self) -> Vec<&TagEntry> {
        self.categories.iter().flat_map(|c| c.entries.iter()).collect()
    }

    pub fn names_in_saved_order(&self) -> Vec<String> {
        self.entries_in_saved_order().into_iter().map(|e| e.name.clone()).collect()
    }

    pub fn sorted_entries(&self) -> Vec<&TagEntry> {
        let now = chrono::Utc::now();
        let mut entries: Vec<&TagEntry> = self.categories.iter().flat_map(|c| c.entries.iter()).collect();
        entries.sort_by(|a, b| {
            let score_a = decay_score(a, now);
            let score_b = decay_score(b, now);
            match score_b.partial_cmp(&score_a).unwrap_or(std::cmp::Ordering::Equal) {
                std::cmp::Ordering::Equal => a.name.cmp(&b.name),
                other => other,
            }
        });
        entries
    }

    pub fn sorted_names(&self) -> Vec<String> {
        self.names_for_display(crate::config::AppConfig::load().tag_position_lock)
    }

    pub fn names_for_display(&self, locked: bool) -> Vec<String> {
        if locked { self.names_in_saved_order() } else { self.sorted_entries().into_iter().map(|e| e.name.clone()).collect() }
    }
}

fn decay_score(entry: &TagEntry, now: chrono::DateTime<chrono::Utc>) -> f64 {
    let days_since = (now - entry.last_used).num_hours() as f64 / 24.0;
    let decay = (0.95f64).powf(days_since.max(0.0));
    entry.use_count as f64 * decay
}

impl Default for TagLibrary {
    fn default() -> Self { Self::new() }
}
