use serde::{Deserialize, Serialize};
use std::collections::HashMap;
const CACHE_VERSION: u8 = 2;

use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct XatanCache {
    #[serde(default)]
    version: u8,
    pub branches: HashMap<String, String>,
}

impl Default for XatanCache {
    fn default() -> Self {
        Self {
            version: CACHE_VERSION,
            branches: HashMap::new(),
        }
    }
}

fn fnv1a_64(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &byte in data {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn get_cache_filename(root: &std::path::Path) -> String {
    let canonical_root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let path_str = canonical_root.to_string_lossy();
    let hash = fnv1a_64(path_str.as_bytes());
    let repo_name = canonical_root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("root");
    format!("{}_{:016x}.json", repo_name, hash)
}

fn get_cache_path() -> Option<PathBuf> {
    let cache_dir = dirs::cache_dir()?;
    let xatan_cache_dir = cache_dir.join("xatan");
    let root = crate::find_repository_root()?;
    let filename = get_cache_filename(&root);
    Some(xatan_cache_dir.join(filename))
}

fn parse_cache(content: &str) -> XatanCache {
    serde_json::from_str::<XatanCache>(content)
        .ok()
        .filter(|cache| cache.version == CACHE_VERSION)
        .unwrap_or_default()
}

pub fn load_cache() -> XatanCache {
    let Some(path) = get_cache_path().filter(|p| p.exists()) else {
        return XatanCache::default();
    };

    let Ok(content) = std::fs::read_to_string(&path) else {
        return XatanCache::default();
    };

    parse_cache(&content)
}

pub fn save_cache(cache: &XatanCache) {
    let Some(path) = get_cache_path() else {
        return;
    };

    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    if let Ok(json) = serde_json::to_string_pretty(cache) {
        let _ = std::fs::write(&path, json);
    }
}

pub fn get_cached_url(branch_name: &str) -> Option<String> {
    let cache = load_cache();
    cache.branches.get(branch_name).cloned()
}

pub fn set_cached_url(branch_name: &str, url: &str) {
    let mut cache = load_cache();
    cache
        .branches
        .insert(branch_name.to_string(), url.to_string());
    save_cache(&cache);
}

pub fn remove_cached_url(branch_name: &str) {
    let mut cache = load_cache();
    if cache.branches.remove(branch_name).is_some() {
        save_cache(&cache);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_fnv1a_64() {
        assert_eq!(fnv1a_64(b""), 0xcbf29ce484222325);
        assert_eq!(fnv1a_64(b"hello"), 0xa430d84680aabd0b);
        assert_eq!(fnv1a_64(b"foobar"), 0x85944171f73967e8);
    }

    #[test]
    fn test_get_cache_filename() {
        let path1 = Path::new("/some/path/to/repo_a");
        let path2 = Path::new("/some/path/to/repo_b");

        let file1 = get_cache_filename(path1);
        let file2 = get_cache_filename(path2);

        assert_ne!(file1, file2);
        assert!(file1.starts_with("repo_a_"));
        assert!(file1.ends_with(".json"));
        assert!(file2.starts_with("repo_b_"));
        assert!(file2.ends_with(".json"));

        // Ensure same path yields same filename
        assert_eq!(get_cache_filename(path1), file1);
    }

    #[test]
    fn test_legacy_cache_is_invalidated() {
        let cache = parse_cache(
            r#"{"branches":{"main":"postgresql://xata@example-deprecated.xata.tech/postgres"}}"#,
        );

        assert!(cache.branches.is_empty());
        assert_eq!(cache.version, CACHE_VERSION);
    }
}
