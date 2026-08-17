use tauri::State;

use super::AppState;

/// Result of an update check, returned to the frontend.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub changelog: String,
    pub release_url: String,
    pub published_at: Option<String>,
}

/// GitHub repository owning the releases (the repo was renamed; the API
/// endpoint for the new name must be used).
const DEFAULT_API_BASE: &str =
    "https://api.github.com/repos/shurgogo/Jable-Missav-Supjav-Downloader";

/// API base URL. Overridable for local testing via the AVDL_RELEASE_API_URL
/// environment variable (e.g. point it at `scripts/mock-release-server.mjs`).
fn api_base() -> String {
    std::env::var("AVDL_RELEASE_API_URL").unwrap_or_else(|_| DEFAULT_API_BASE.to_string())
}

/// Compare two semver-ish strings ("1.2.3", "v1.2.3", "0.1.5-beta.1").
/// Returns true when `a` is strictly newer than `b`.
fn is_newer(a: &str, b: &str) -> bool {
    fn parse(v: &str) -> (u64, u64, u64) {
        let core = v
            .trim()
            .trim_start_matches('v')
            .split(['-', '+'])
            .next()
            .unwrap_or("0");
        let mut parts = core.split('.');
        let major = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        let minor = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        let patch = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        (major, minor, patch)
    }
    parse(a) > parse(b)
}

/// Parse a GitHub release JSON object into the fields we care about.
fn release_fields(json: &serde_json::Value) -> Option<(String, String, String, Option<String>)> {
    if !json.is_object() {
        return None;
    }
    let tag = json["tag_name"].as_str().unwrap_or("").to_string();
    let body = json["body"].as_str().unwrap_or("").to_string();
    let html_url = json["html_url"].as_str().unwrap_or("").to_string();
    let published_at = json["published_at"].as_str().map(|s| s.to_string());
    if tag.is_empty() || html_url.is_empty() {
        return None;
    }
    Some((tag, body, html_url, published_at))
}

/// Decide which release to present: the `latest` release, unless it is a
/// prerelease/draft — in which case pick the newest stable entry from `list`.
/// Returns (tag, changelog, html_url, published_at).
fn pick_release_fields(
    latest: &serde_json::Value,
    list: Option<&serde_json::Value>,
) -> Result<(String, String, String, Option<String>), String> {
    let bad = || "GitHub 返回数据异常".to_string();
    let is_unstable = latest["prerelease"].as_bool().unwrap_or(false)
        || latest["draft"].as_bool().unwrap_or(false);
    if !is_unstable {
        return release_fields(latest).ok_or_else(bad);
    }
    let arr = list.ok_or_else(bad)?.as_array().ok_or_else(bad)?;
    arr.iter()
        .find(|r| {
            !r["prerelease"].as_bool().unwrap_or(false)
                && !r["draft"].as_bool().unwrap_or(false)
        })
        .and_then(release_fields)
        .ok_or_else(|| "没有可用的正式版本".to_string())
}

/// GET a GitHub API endpoint and parse the JSON response.
async fn fetch_release(client: &wreq::Client, url: String) -> Result<serde_json::Value, String> {
    let resp = client
        .get(&url)
        .header("user-agent", "AVDL-Updater")
        .header("accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("网络请求失败: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("GitHub API 返回 HTTP {}", resp.status()));
    }
    let text = resp
        .text()
        .await
        .map_err(|e| format!("读取响应失败: {}", e))?;
    serde_json::from_str::<serde_json::Value>(&text).map_err(|e| format!("解析响应失败: {}", e))
}

/// Fetch the newest non-prerelease, non-draft release from GitHub and compare
/// it against the running version. Uses the app's proxy-aware HTTP client so
/// the check works even when GitHub requires a proxy.
#[tauri::command]
pub async fn check_for_update(state: State<'_, AppState>) -> Result<UpdateInfo, String> {
    let base = api_base();
    let latest_url = format!("{}/releases/latest", base);
    let list_url = format!("{}/releases?per_page=10", base);

    // Clone the client out of the lock before awaiting so the guard is not
    // held across the network calls (proxy settings may swap the client).
    let client = state.client.lock().unwrap().clone();

    let json = fetch_release(&client, latest_url).await?;

    // /releases/latest can point at a prerelease; fall back to the list and
    // pick the newest stable release in that case.
    let fields = if json["prerelease"].as_bool().unwrap_or(false) {
        let list = fetch_release(&client, list_url).await?;
        pick_release_fields(&json, Some(&list))?
    } else {
        pick_release_fields(&json, None)?
    };

    let (latest_version, changelog, release_url, published_at) = fields;
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    let update_available = is_newer(&latest_version, &current_version);

    Ok(UpdateInfo {
        current_version,
        latest_version,
        update_available,
        changelog,
        release_url,
        published_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn semver_comparison() {
        assert!(is_newer("0.1.6", "0.1.5"));
        assert!(is_newer("v0.1.6", "0.1.5"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(is_newer("0.2.0", "0.1.99"));
        assert!(!is_newer("0.1.5", "0.1.5"));
        assert!(!is_newer("0.1.4", "0.1.5"));
        assert!(!is_newer("0.1.5-beta.1", "0.1.5"));
        assert!(is_newer("0.1.6", "0.1.5-beta.1"));
    }

    #[test]
    fn update_info_serializes_with_camel_case_keys() {
        // The frontend reads camelCase keys; Tauri does not rename nested
        // values, so the struct itself must carry rename_all.
        let info = UpdateInfo {
            current_version: "0.1.5".to_string(),
            latest_version: "v0.1.6".to_string(),
            update_available: true,
            changelog: "fix download".to_string(),
            release_url: "https://example.com/releases/v0.1.6".to_string(),
            published_at: Some("2026-08-01T00:00:00Z".to_string()),
        };
        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(json["currentVersion"], "0.1.5");
        assert_eq!(json["latestVersion"], "v0.1.6");
        assert_eq!(json["updateAvailable"], true);
        assert_eq!(json["releaseUrl"], "https://example.com/releases/v0.1.6");
        assert_eq!(json["publishedAt"], "2026-08-01T00:00:00Z");
        assert!(json.get("current_version").is_none());
    }

    fn release(tag: &str, prerelease: bool, draft: bool) -> serde_json::Value {
        json!({
            "tag_name": tag,
            "body": format!("changelog for {}", tag),
            "html_url": format!("https://example.com/releases/{}", tag),
            "published_at": "2026-08-01T00:00:00Z",
            "prerelease": prerelease,
            "draft": draft,
        })
    }

    #[test]
    fn picks_latest_stable_release() {
        let latest = release("v0.1.6", false, false);
        let (tag, body, url, _) = pick_release_fields(&latest, None).unwrap();
        assert_eq!(tag, "v0.1.6");
        assert!(body.contains("v0.1.6"));
        assert!(url.contains("v0.1.6"));
    }

    #[test]
    fn falls_back_to_stable_when_latest_is_prerelease() {
        let latest = release("v0.2.0-beta.1", true, false);
        let list = json!([release("v0.1.6", false, false), latest]);
        let (tag, _, _, _) = pick_release_fields(&latest, Some(&list)).unwrap();
        assert_eq!(tag, "v0.1.6");
    }

    #[test]
    fn errors_when_no_stable_release_exists() {
        let latest = release("v0.2.0-beta.1", true, false);
        let list = json!([release("v0.2.0-beta.2", true, false)]);
        assert!(pick_release_fields(&latest, Some(&list)).is_err());
    }
}
