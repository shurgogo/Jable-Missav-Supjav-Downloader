use tauri::State;
use crate::scraper::Site;
use super::AppState;

#[derive(Debug, serde::Deserialize)]
pub struct PreviewVideoRequest {
    pub site: Site,
    pub url: String,
}

#[tauri::command]
pub async fn fetch_preview_video(
    state: State<'_, AppState>,
    req: PreviewVideoRequest,
) -> Result<Vec<u8>, String> {
    let mut http_req = state
        .client
        .get(&req.url)
        .header(
            "User-Agent",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
        )
        .header("Accept", "*/*");

    match req.site {
        Site::Jable => {
            http_req = http_req.header("Referer", "https://jable.tv/");
        }
        Site::Missav => {
            let active_domain = crate::scraper::missav::get_active_domain();
            http_req = http_req.header("Referer", format!("{}/", active_domain));
        }
        Site::Supjav => {
            http_req = http_req.header("Referer", "https://supjav.com/");
        }
    }

    let resp = http_req.send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP status {}", resp.status()));
    }

    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    Ok(bytes.to_vec())
}
