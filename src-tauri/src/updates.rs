//! Updates settings backend: local version + GitHub release check.
//!
//! Compares the bundled version (`tauri.conf.json` / `package.json`) with
//! the latest `djeel/whisp` GitHub release. No auto-download: the frontend
//! shows the release link when a newer tag exists. Errors degrade to
//! `update_available: false` with a message (offline-friendly).

const REPO: &str = "djeel/whisp";
const API_URL: &str = "https://api.github.com/repos/djeel/whisp/releases/latest";

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current: String,
    pub latest: Option<String>,
    pub update_available: bool,
    pub url: Option<String>,
    pub message: Option<String>,
}

fn current_version() -> String {
    // Baked at compile time from Cargo (kept in sync with tauri.conf.json).
    env!("CARGO_PKG_VERSION").to_string()
}

/// Parse `v1.2.3` / `1.2.3` into comparable numbers. Non-numeric parts -> 0.
fn parse_ver(v: &str) -> Vec<u64> {
    v.trim()
        .trim_start_matches('v')
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<u64>().unwrap_or(0))
        .collect()
}

fn is_newer(latest: &str, current: &str) -> bool {
    let (a, b) = (parse_ver(latest), parse_ver(current));
    let n = a.len().max(b.len()).max(1);
    for i in 0..n {
        let (x, y) = (*a.get(i).unwrap_or(&0), *b.get(i).unwrap_or(&0));
        if x != y {
            return x > y;
        }
    }
    false
}

fn check_inner() -> UpdateInfo {
    let current = current_version();
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("whisp-update-check")
        .build();
    let client = match client {
        Ok(c) => c,
        Err(e) => {
            return UpdateInfo {
                current,
                latest: None,
                update_available: false,
                url: None,
                message: Some(format!("client http: {e}")),
            };
        }
    };
    let resp: Result<serde_json::Value, String> = client
        .get(API_URL)
        .send()
        .map_err(|e| format!("GitHub injoignable: {e}"))
        .and_then(|r| r.error_for_status().map_err(|e| format!("GitHub: {e}")))
        .and_then(|r| r.json().map_err(|e| format!("réponse GitHub: {e}")));
    match resp {
        Err(message) => UpdateInfo {
            current,
            latest: None,
            update_available: false,
            url: None,
            message: Some(message),
        },
        Ok(v) => {
            let tag = v
                .get("tag_name")
                .and_then(|t| t.as_str())
                .unwrap_or_default()
                .to_string();
            let url = v
                .get("html_url")
                .and_then(|u| u.as_str())
                .map(|s| s.to_string())
                .or_else(|| {
                    (!tag.is_empty())
                        .then(|| format!("https://github.com/{REPO}/releases/tag/{tag}"))
                });
            if tag.is_empty() {
                return UpdateInfo {
                    current,
                    latest: None,
                    update_available: false,
                    url: None,
                    message: Some("aucune release trouvée".to_string()),
                };
            }
            let newer = is_newer(&tag, &current);
            UpdateInfo {
                current,
                latest: Some(tag.clone()),
                update_available: newer,
                url,
                message: Some(if newer {
                    format!("Nouvelle version {tag} disponible.")
                } else {
                    "whisp est à jour.".to_string()
                }),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Local app version (from Cargo/tauri.conf.json).
#[tauri::command]
pub fn app_version() -> String {
    current_version()
}

/// Compare local version with the latest GitHub release.
#[tauri::command]
pub fn check_updates() -> UpdateInfo {
    check_inner()
}

#[cfg(test)]
mod tests {
    use super::{is_newer, parse_ver};

    #[test]
    fn version_compare() {
        assert_eq!(parse_ver("v1.2.3"), vec![1, 2, 3]);
        assert!(is_newer("v0.2.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.0.9", "0.1.0"));
    }
}
