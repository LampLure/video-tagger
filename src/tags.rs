use serde::{Deserialize, Serialize};

use crate::config::sanitize_filename;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagEntry {
    pub name: String,
    pub use_count: u64,
    pub last_used: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagLibrary {
    entries: Vec<TagEntry>,
}

impl TagLibrary {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn load() -> Self {
        let path = crate::config::tag_library_path();
        if path.exists() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_else(Self::new)
        } else {
            Self::new()
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

    pub fn add_tag(&mut self, name: &str) {
        let name = sanitize_filename(name.trim());
        if name.is_empty() {
            return;
        }
        if !self.entries.iter().any(|e| e.name == name) {
            self.entries.push(TagEntry {
                name,
                use_count: 0,
                last_used: chrono::Utc::now(),
            });
        }
    }

    pub fn remove_tag(&mut self, name: &str) {
        self.entries.retain(|e| e.name != name);
    }

    pub fn record_usage(&mut self, names: &[String]) {
        let now = chrono::Utc::now();
        for name in names {
            if let Some(entry) = self.entries.iter_mut().find(|e| e.name == *name) {
                entry.use_count += 1;
                entry.last_used = now;
            }
        }
    }

    pub fn entries_in_saved_order(&self) -> Vec<&TagEntry> {
        self.entries.iter().collect()
    }

    pub fn names_in_saved_order(&self) -> Vec<String> {
        self.entries
            .iter()
            .map(|e| e.name.clone())
            .collect()
    }

    pub fn sorted_entries(&self) -> Vec<&TagEntry> {
        let now = chrono::Utc::now();
        let mut entries: Vec<&TagEntry> = self.entries.iter().collect();
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
        self.sorted_entries()
            .into_iter()
            .map(|e| e.name.clone())
            .collect()
    }

    pub fn names_for_display(&self, locked: bool) -> Vec<String> {
        if locked {
            self.names_in_saved_order()
        } else {
            self.sorted_names()
        }
    }
}

fn decay_score(entry: &TagEntry, now: chrono::DateTime<chrono::Utc>) -> f64 {
    let days_since = (now - entry.last_used).num_hours() as f64 / 24.0;
    let decay = (0.95f64).powf(days_since.max(0.0));
    entry.use_count as f64 * decay
}

impl Default for TagLibrary {
    fn default() -> Self {
        Self::new()
    }
}