/// Computes deterministic stable FNV-1a 32-bit hash.
fn fnv1a_hash(s: &str) -> u32 {
    let mut hash = 2166136261u32;
    for byte in s.bytes() {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(16777619);
    }
    hash
}

use std::process::Command;

/// Converts raw identifier to an ASCII-safe prefix string.
/// Rules:
/// 1. Convert all characters to lowercase.
/// 2. Replace any continuous block of non-alphanumeric characters (including spaces, dots, @, +, hyphens, underscores) with a single hyphen `-`.
/// 3. Trim any leading or trailing hyphens.
pub fn slugify(s: &str) -> String {
    let mut result = String::new();
    let mut in_hyphen = false;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            result.push(c.to_ascii_lowercase());
            in_hyphen = false;
        } else {
            if !in_hyphen {
                result.push('-');
                in_hyphen = true;
            }
        }
    }
    // Trim leading/trailing hyphens
    let mut start = 0;
    while start < result.len() && result.as_bytes()[start] == b'-' {
        start += 1;
    }
    let mut end = result.len();
    while end > start && result.as_bytes()[end - 1] == b'-' {
        end -= 1;
    }
    result[start..end].to_string()
}

/// Query git config for a key. Returns trimmed stdout if successful and non-empty.
fn query_git_config(key: &str) -> Option<String> {
    Command::new("git")
        .args(["config", "--get", key])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Generic implementation of identity resolution to make it 100% unit-testable.
pub fn resolve_identity_impl<E, G, O>(
    get_env: E,
    get_git: G,
    get_os_user: O,
) -> Result<String, &'static str>
where
    E: Fn(&str) -> Option<String>,
    G: Fn(&str) -> Option<String>,
    O: Fn() -> Option<String>,
{
    // 1. Environment Override
    if let Some(override_prefix) = get_env("XATAN_PREFIX") {
        let trimmed = override_prefix.trim();
        if !trimmed.is_empty() {
            let slugged = slugify(trimmed);
            if !slugged.is_empty() {
                return Ok(slugged);
            }
        }
    }

    // 2. Git Email Extraction
    if let Some(email) = get_git("user.email") {
        let trimmed = email.trim();
        if !trimmed.is_empty() {
            let mut parts = trimmed.split('@');
            if let (Some(local_part), Some(domain)) = (parts.next(), parts.next()) {
                let local_part_trimmed = local_part.trim();
                let domain_trimmed = domain.trim();
                if !local_part_trimmed.is_empty() && !domain_trimmed.is_empty() {
                    let raw_id = if domain_trimmed.len() <= 12 {
                        format!("{}-{}", local_part_trimmed, domain_trimmed)
                    } else {
                        format!("{}-{:08x}", local_part_trimmed, fnv1a_hash(domain_trimmed))
                    };
                    let slugged = slugify(&raw_id);
                    if !slugged.is_empty() {
                        return Ok(slugged);
                    }
                }
            }
        }
    }

    // 3. Git Name Fallback
    if let Some(name) = get_git("user.name") {
        let trimmed = name.trim();
        if !trimmed.is_empty() {
            let slugged = slugify(trimmed);
            if !slugged.is_empty() {
                return Ok(slugged);
            }
        }
    }

    // 4. OS User Fallback
    if let Some(os_user) = get_os_user() {
        let trimmed = os_user.trim();
        if !trimmed.is_empty() {
            let slugged = slugify(trimmed);
            if !slugged.is_empty() {
                return Ok(slugged);
            }
        }
    }

    Err("Failed to resolve any valid identity fallback")
}

/// Resolves developer prefix using the Smart Identity Resolution Algorithm.
pub fn resolve_identity() -> Result<String, String> {
    resolve_identity_impl(
        |key| std::env::var(key).ok(),
        query_git_config,
        || {
            std::env::var("USER")
                .or_else(|_| std::env::var("LOGNAME"))
                .ok()
        },
    )
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slugify() {
        assert_eq!(slugify("Alice.Smith@company.com"), "alice-smith-company-com");
        assert_eq!(slugify("Dev+Sandbox@org.net"), "dev-sandbox-org-net");
        assert_eq!(slugify("Jane Doe"), "jane-doe");
        assert_eq!(slugify("admin_local"), "admin-local");
        assert_eq!(slugify("---hello---world---"), "hello-world");
        assert_eq!(slugify("   "), "");
        assert_eq!(slugify("Café"), "caf");
        assert_eq!(slugify("éñあ"), "");
    }

    #[test]
    fn test_resolve_identity_env_override() {
        let get_env = |key: &str| {
            if key == "XATAN_PREFIX" {
                Some("Custom_Prefix".to_string())
            } else {
                None
            }
        };
        let get_git = |_key: &str| None;
        let get_os_user = || None;

        assert_eq!(
            resolve_identity_impl(get_env, get_git, get_os_user),
            Ok("custom-prefix".to_string())
        );
    }

    #[test]
    fn test_resolve_identity_git_email() {
        let get_env = |_key: &str| None;
        let get_git = |key: &str| {
            if key == "user.email" {
                Some("Alice.Smith@company.com".to_string())
            } else {
                None
            }
        };
        let get_os_user = || None;

        assert_eq!(
            resolve_identity_impl(get_env, get_git, get_os_user),
            Ok("alice-smith-company-com".to_string())
        );
    }

    #[test]
    fn test_resolve_identity_git_name() {
        let get_env = |_key: &str| None;
        let get_git = |key: &str| {
            if key == "user.name" {
                Some("Jane Doe".to_string())
            } else {
                None
            }
        };
        let get_os_user = || None;

        assert_eq!(
            resolve_identity_impl(get_env, get_git, get_os_user),
            Ok("jane-doe".to_string())
        );
    }

    #[test]
    fn test_resolve_identity_os_user() {
        let get_env = |_key: &str| None;
        let get_git = |_key: &str| None;
        let get_os_user = || Some("admin_local".to_string());

        assert_eq!(
            resolve_identity_impl(get_env, get_git, get_os_user),
            Ok("admin-local".to_string())
        );
    }

    #[test]
    fn test_resolve_identity_empty_slug_fallback() {
        // Env override is non-empty but slugifies to empty string.
        // Git email is valid. It should skip the env override and use git email.
        let get_env = |key: &str| {
            if key == "XATAN_PREFIX" {
                Some("---".to_string())
            } else {
                None
            }
        };
        let get_git = |key: &str| {
            if key == "user.email" {
                Some("bob@work.com".to_string())
            } else {
                None
            }
        };
        let get_os_user = || None;

        assert_eq!(
            resolve_identity_impl(get_env, get_git, get_os_user),
            Ok("bob-work-com".to_string())
        );
    }

    #[test]
    fn test_resolve_identity_git_email_long_domain() {
        let get_env = |_key: &str| None;
        let get_git = |key: &str| {
            if key == "user.email" {
                Some("jane.doe@super-long-corporate-email-address.com".to_string())
            } else {
                None
            }
        };
        let get_os_user = || None;

        let res = resolve_identity_impl(get_env, get_git, get_os_user).unwrap();
        assert!(res.starts_with("jane-doe-"));
        assert_eq!(res.len(), "jane-doe-".len() + 8); // 8 hex digits FNV-1a hash
    }



    #[test]
    fn test_resolve_identity_failed() {
        let get_env = |_key: &str| None;
        let get_git = |_key: &str| None;
        let get_os_user = || None;

        assert_eq!(
            resolve_identity_impl(get_env, get_git, get_os_user),
            Err("Failed to resolve any valid identity fallback")
        );
    }
}
