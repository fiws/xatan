use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct XatanConfig {
    pub org: Option<String>,
    pub project: Option<String>,
    pub database: Option<String>,
    pub fallback_parent: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedConfig {
    pub org: String,
    pub project: String,
    pub database: String,
    pub fallback_parent: String,
    pub api_key: String,
}

/// Recursively searches for `.xatanrc` or `xatan.json` starting from `start_dir` and going up.
pub fn find_config_file(start_dir: &Path) -> Option<PathBuf> {
    let mut current_dir = start_dir.to_path_buf();
    loop {
        let rc_path = current_dir.join(".xatanrc");
        if rc_path.is_file() {
            return Some(rc_path);
        }
        let json_path = current_dir.join("xatan.json");
        if json_path.is_file() {
            return Some(json_path);
        }
        if !current_dir.pop() {
            break;
        }
    }
    None
}

/// Reads and parses the config file if found.
pub fn load_config_file(path: &Path) -> Result<XatanConfig, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read config file {}: {}", path.display(), e))?;
    serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse config file {}: {}", path.display(), e))
}

/// Testable configuration resolution logic.
pub fn resolve_config_impl<E>(
    get_env: E,
    config_file: Option<XatanConfig>,
    defaults: (Option<String>, Option<String>, Option<String>),
) -> Result<ResolvedConfig, String>
where
    E: Fn(&str) -> Option<String>,
{
    // 1. Resolve API Key (required for all API-interacting commands)
    let api_key = get_env("XATA_API_KEY")
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| "XATA_API_KEY environment variable is missing or empty".to_string())?;

    // Unpack config file values
    let file_org = config_file.as_ref().and_then(|c| c.org.clone());
    let file_project = config_file.as_ref().and_then(|c| c.project.clone());
    let file_database = config_file.as_ref().and_then(|c| c.database.clone());
    let file_fallback_parent = config_file.as_ref().and_then(|c| c.fallback_parent.clone());

    let (def_org, def_project, def_database) = defaults;

    // 2. Resolve properties using environment variables with config file and autofill defaults fallbacks
    let org = get_env("XATA_ORG_ID")
        .or(file_org)
        .or(def_org)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| "Organization ID (org) could not be resolved".to_string())?;

    let project = get_env("XATA_PROJECT_ID")
        .or(file_project)
        .or(def_project)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| "Project ID (project) could not be resolved".to_string())?;

    let database = get_env("XATA_DATABASE_NAME")
        .or_else(|| get_env("XATA_DATABASE"))
        .or(file_database)
        .or(def_database)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| "Database name (database) could not be resolved".to_string())?;

    let fallback_parent = get_env("XATAN_FALLBACK_PARENT")
        .or(file_fallback_parent)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "main".to_string());

    Ok(ResolvedConfig {
        org: org.trim().to_string(),
        project: project.trim().to_string(),
        database: database.trim().to_string(),
        fallback_parent: fallback_parent.trim().to_string(),
        api_key: api_key.trim().to_string(),
    })
}

/// Resolves the configuration dynamically from environment variables and local `.xatanrc` or `xatan.json`.
pub fn resolve_config() -> Result<ResolvedConfig, String> {
    let current_dir = std::env::current_dir()
        .map_err(|e| format!("Failed to get current directory: {}", e))?;
    
    let config_file = if let Some(path) = find_config_file(&current_dir) {
        Some(load_config_file(&path)?)
    } else {
        None
    };

    let defaults = get_xata_defaults();

    resolve_config_impl(|key| std::env::var(key).ok(), config_file, defaults)
}

/// Internal helper to parse database URL from .xata/config.json or .xatarc
pub fn parse_database_url(url: &str) -> (Option<String>, Option<String>, Option<String>) {
    let without_scheme = url.trim_start_matches("https://").trim_start_matches("http://");
    let mut parts = without_scheme.split('/');
    let host = parts.next().unwrap_or("");

    let mut db_name = None;
    let path_segments: Vec<&str> = parts.collect();
    if let Some(pos) = path_segments.iter().position(|&s| s == "db") {
        if pos + 1 < path_segments.len() {
            let db_segment = path_segments[pos + 1];
            let clean_db = db_segment.split(':').next().unwrap_or(db_segment);
            db_name = Some(clean_db.to_string());
        }
    }

    let host_parts: Vec<&str> = host.split('.').collect();
    let subdomain = host_parts.first().unwrap_or(&"");

    let mut org = None;
    let mut project = None;
    if !subdomain.is_empty() {
        if let Some(idx) = subdomain.find('-') {
            let (o, p) = subdomain.split_at(idx);
            org = Some(o.to_string());
            project = Some(p.trim_start_matches('-').to_string());
        } else {
            org = Some(subdomain.to_string());
        }
    }

    (org, project, db_name)
}

/// Attempts to read default properties from environment variables or local files
pub fn get_xata_defaults() -> (Option<String>, Option<String>, Option<String>) {
    let mut org = std::env::var("XATA_ORG_ID").ok();
    let mut project = std::env::var("XATA_PROJECT_ID").ok();
    let mut database = std::env::var("XATA_DATABASE_NAME")
        .or_else(|_| std::env::var("XATA_DATABASE"))
        .ok();

    if org.is_none() || project.is_none() || database.is_none() {
        let files = [
            ".xata/config.json",
            ".xatarc",
            ".xatarc.json",
            "xatarc.json",
        ];
        for f in files {
            if let Ok(content) = std::fs::read_to_string(f) {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                    let db_url = val.get("databaseURL")
                        .or_else(|| val.get("databaseUrl"))
                        .and_then(|v| v.as_str());

                    if let Some(url) = db_url {
                        let (parsed_org, parsed_proj, parsed_db) = parse_database_url(url);
                        if org.is_none() {
                            org = parsed_org;
                        }
                        if project.is_none() {
                            project = parsed_proj;
                        }
                        if database.is_none() {
                            database = parsed_db;
                        }
                    }
                }
            }
        }
    }

    (org, project, database)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_config_all_env() {
        let envs = [
            ("XATA_API_KEY", "my-api-key"),
            ("XATA_ORG_ID", "my-org"),
            ("XATA_PROJECT_ID", "my-project"),
            ("XATA_DATABASE_NAME", "my-db"),
            ("XATAN_FALLBACK_PARENT", "my-parent"),
        ];
        let get_env = |key: &str| {
            envs.iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.to_string())
        };

        let resolved = resolve_config_impl(get_env, None, (None, None, None)).unwrap();
        assert_eq!(
            resolved,
            ResolvedConfig {
                org: "my-org".to_string(),
                project: "my-project".to_string(),
                database: "my-db".to_string(),
                fallback_parent: "my-parent".to_string(),
                api_key: "my-api-key".to_string(),
            }
        );
    }

    #[test]
    fn test_resolve_config_all_file() {
        let envs = [("XATA_API_KEY", "my-api-key")];
        let get_env = |key: &str| {
            envs.iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.to_string())
        };

        let file_config = XatanConfig {
            org: Some("file-org".to_string()),
            project: Some("file-project".to_string()),
            database: Some("file-db".to_string()),
            fallback_parent: Some("file-parent".to_string()),
        };

        let resolved = resolve_config_impl(get_env, Some(file_config), (None, None, None)).unwrap();
        assert_eq!(
            resolved,
            ResolvedConfig {
                org: "file-org".to_string(),
                project: "file-project".to_string(),
                database: "file-db".to_string(),
                fallback_parent: "file-parent".to_string(),
                api_key: "my-api-key".to_string(),
            }
        );
    }

    #[test]
    fn test_resolve_config_priority() {
        // Env should override file config
        let envs = [
            ("XATA_API_KEY", "my-api-key"),
            ("XATA_ORG_ID", "env-org"),
            ("XATA_DATABASE_NAME", "env-db"),
        ];
        let get_env = |key: &str| {
            envs.iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.to_string())
        };

        let file_config = XatanConfig {
            org: Some("file-org".to_string()),
            project: Some("file-project".to_string()),
            database: Some("file-db".to_string()),
            fallback_parent: Some("file-parent".to_string()),
        };

        let resolved = resolve_config_impl(get_env, Some(file_config), (None, None, None)).unwrap();
        assert_eq!(
            resolved,
            ResolvedConfig {
                org: "env-org".to_string(),
                project: "file-project".to_string(),
                database: "env-db".to_string(),
                fallback_parent: "file-parent".to_string(),
                api_key: "my-api-key".to_string(),
            }
        );
    }

    #[test]
    fn test_resolve_config_database_fallbacks() {
        // XATA_DATABASE_NAME preferred over XATA_DATABASE
        let envs = [
            ("XATA_API_KEY", "my-api-key"),
            ("XATA_ORG_ID", "my-org"),
            ("XATA_PROJECT_ID", "my-project"),
            ("XATA_DATABASE", "xata-db-fallback"),
            ("XATA_DATABASE_NAME", "xata-db-name"),
        ];
        let get_env = |key: &str| {
            envs.iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.to_string())
        };

        let resolved = resolve_config_impl(get_env, None, (None, None, None)).unwrap();
        assert_eq!(resolved.database, "xata-db-name");

        // Check fallback to XATA_DATABASE
        let envs2 = [
            ("XATA_API_KEY", "my-api-key"),
            ("XATA_ORG_ID", "my-org"),
            ("XATA_PROJECT_ID", "my-project"),
            ("XATA_DATABASE", "xata-db-fallback"),
        ];
        let get_env2 = |key: &str| {
            envs2
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.to_string())
        };
        let resolved2 = resolve_config_impl(get_env2, None, (None, None, None)).unwrap();
        assert_eq!(resolved2.database, "xata-db-fallback");
    }

    #[test]
    fn test_resolve_config_missing_required() {
        let envs = [("XATA_API_KEY", "my-api-key")];
        let get_env = |key: &str| {
            envs.iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.to_string())
        };

        let res = resolve_config_impl(get_env, None, (None, None, None));
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("Organization ID"));
    }

    #[test]
    fn test_resolve_config_missing_api_key() {
        let envs = [
            ("XATA_ORG_ID", "my-org"),
            ("XATA_PROJECT_ID", "my-project"),
            ("XATA_DATABASE_NAME", "my-db"),
        ];
        let get_env = |key: &str| {
            envs.iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.to_string())
        };

        let res = resolve_config_impl(get_env, None, (None, None, None));
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("XATA_API_KEY"));
    }

    #[test]
    fn test_parse_database_url() {
        let url = "https://org123-proj456.us-east-1.xata.sh/db/mydb";
        let (org, proj, db) = parse_database_url(url);
        assert_eq!(org, Some("org123".to_string()));
        assert_eq!(proj, Some("proj456".to_string()));
        assert_eq!(db, Some("mydb".to_string()));

        // With branch suffix
        let url_branch = "https://org123-proj456.us-east-1.xata.sh/db/mydb:main";
        let (org_b, proj_b, db_b) = parse_database_url(url_branch);
        assert_eq!(org_b, Some("org123".to_string()));
        assert_eq!(proj_b, Some("proj456".to_string()));
        assert_eq!(db_b, Some("mydb".to_string()));

        // Subdomain only
        let url_simple = "https://org123.xata.sh/db/mydb";
        let (org_s, proj_s, db_s) = parse_database_url(url_simple);
        assert_eq!(org_s, Some("org123".to_string()));
        assert_eq!(proj_s, None);
        assert_eq!(db_s, Some("mydb".to_string()));
    }

    #[test]
    fn test_resolve_config_with_defaults() {
        let envs = [("XATA_API_KEY", "my-api-key")];
        let get_env = |key: &str| {
            envs.iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.to_string())
        };

        let defaults = (
            Some("def-org".to_string()),
            Some("def-proj".to_string()),
            Some("def-db".to_string()),
        );

        let resolved = resolve_config_impl(get_env, None, defaults).unwrap();
        assert_eq!(
            resolved,
            ResolvedConfig {
                org: "def-org".to_string(),
                project: "def-proj".to_string(),
                database: "def-db".to_string(),
                fallback_parent: "main".to_string(),
                api_key: "my-api-key".to_string(),
            }
        );
    }
}

