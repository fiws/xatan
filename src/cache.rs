use std::collections::HashMap;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct XatanCache {
    pub branches: HashMap<String, String>,
}

/// Resolves path to .xata/cache.json inside repository root
fn get_cache_path() -> Option<PathBuf> {
    let root = crate::find_repository_root()?;
    Some(root.join(".xata").join("cache.json"))
}

pub fn load_cache() -> XatanCache {
    if let Some(path) = get_cache_path() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(cache) = serde_json::from_str::<XatanCache>(&content) {
                return cache;
            }
        }
    }
    XatanCache::default()
}

pub fn save_cache(cache: &XatanCache) {
    if let Some(path) = get_cache_path() {
        // Ensure .xata directory exists
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(cache) {
            let _ = std::fs::write(&path, json);
        }
    }
}

pub fn get_cached_url(branch_name: &str) -> Option<String> {
    let cache = load_cache();
    cache.branches.get(branch_name).cloned()
}

pub fn set_cached_url(branch_name: &str, url: &str) {
    let mut cache = load_cache();
    cache.branches.insert(branch_name.to_string(), url.to_string());
    save_cache(&cache);
}

pub fn remove_cached_url(branch_name: &str) {
    let mut cache = load_cache();
    if cache.branches.remove(branch_name).is_some() {
        save_cache(&cache);
    }
}
