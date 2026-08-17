use serde::Deserialize;
use std::collections::HashMap;
use tauri::State;

use super::AppState;
use crate::scraper::{
    jable, missav, supjav, Category, FetchOptions, Site, TagItem, VideoListResponse,
};

/// 统一的 Scraper IPC 请求结构体
#[derive(Debug, Deserialize)]
pub struct ScraperRequest {
    pub site: Site,
    pub url: Option<String>,
    pub keyword: Option<String>,
    pub page: Option<usize>,
    pub sort_by: Option<String>,
    #[serde(default)]
    pub lang: String,
}

#[tauri::command]
pub async fn get_categories(
    state: State<'_, AppState>,
    req: ScraperRequest,
) -> Result<Vec<Category>, String> {
    // Clone the client out of the lock before awaiting (proxy settings may
    // swap it at runtime).
    let client = state.client.lock().unwrap().clone();
    match req.site {
        Site::Jable => Ok(jable::get_categories(&client, &req.lang).await),
        Site::Missav => Ok(missav::get_categories(&client, &req.lang).await),
        Site::Supjav => Ok(supjav::get_categories(&req.lang).await),
    }
}

#[tauri::command]
pub async fn fetch_video_list(
    state: State<'_, AppState>,
    req: ScraperRequest,
) -> Result<VideoListResponse, String> {
    let target_url = req.url.as_deref().unwrap_or_default();
    let page = req.page.unwrap_or(1);

    let client = state.client.lock().unwrap().clone();

    let (cf_clearance, user_agent) = {
        let configs = state.cf_configs.lock().unwrap();
        crate::scraper::get_cf_headers_for_url(&configs, target_url)
    };

    let opts = FetchOptions {
        client: &client,
        target: target_url,
        page,
        sort_by: req.sort_by.as_deref(),
        lang: &req.lang,
        cf_clearance: &cf_clearance,
        user_agent: &user_agent,
    };

    match req.site {
        Site::Jable => jable::fetch_list(&opts)
            .await
            .map_err(|e| crate::error::map_scraper_error(e.to_string(), target_url)),
        Site::Missav => missav::fetch_list(&opts)
            .await
            .map_err(|e| crate::error::map_scraper_error(e.to_string(), target_url)),
        Site::Supjav => supjav::fetch_list(&opts)
            .await
            .map_err(|e| crate::error::map_scraper_error(e.to_string(), target_url)),
    }
}

#[tauri::command]
pub async fn search_videos(
    state: State<'_, AppState>,
    req: ScraperRequest,
) -> Result<VideoListResponse, String> {
    let keyword = req.keyword.as_deref().unwrap_or_default();
    let page = req.page.unwrap_or(1);

    let client = state.client.lock().unwrap().clone();

    match req.site {
        Site::Jable => {
            let (cf_clearance, user_agent) = {
                let configs = state.cf_configs.lock().unwrap();
                crate::scraper::get_cf_headers_for_url(&configs, "https://jable.tv/")
            };
            let opts = FetchOptions {
                client: &client,
                target: keyword,
                page,
                sort_by: req.sort_by.as_deref(),
                lang: &req.lang,
                cf_clearance: &cf_clearance,
                user_agent: &user_agent,
            };
            jable::search_videos(&opts)
                .await
                .map_err(|e| crate::error::map_scraper_error(e.to_string(), "https://jable.tv/"))
        }
        Site::Missav => {
            let active_domain = missav::get_active_domain();
            let (cf_clearance, user_agent) = {
                let configs = state.cf_configs.lock().unwrap();
                crate::scraper::get_cf_headers_for_url(&configs, &active_domain)
            };
            let opts = FetchOptions {
                client: &client,
                target: keyword,
                page,
                sort_by: req.sort_by.as_deref(),
                lang: &req.lang,
                cf_clearance: &cf_clearance,
                user_agent: &user_agent,
            };
            missav::search_videos(&opts)
                .await
                .map_err(|e| crate::error::map_scraper_error(e.to_string(), &active_domain))
        }
        Site::Supjav => {
            let (cf_clearance, user_agent) = {
                let configs = state.cf_configs.lock().unwrap();
                crate::scraper::get_cf_headers_for_url(&configs, "https://supjav.com/")
            };
            let opts = FetchOptions {
                client: &client,
                target: keyword,
                page,
                sort_by: req.sort_by.as_deref(),
                lang: &req.lang,
                cf_clearance: &cf_clearance,
                user_agent: &user_agent,
            };
            supjav::search_videos(&opts)
                .await
                .map_err(|e| crate::error::map_scraper_error(e.to_string(), "https://supjav.com/"))
        }
    }
}

#[tauri::command]
pub async fn get_sidebar_tags(
    req: ScraperRequest,
) -> Result<HashMap<String, Vec<TagItem>>, String> {
    match req.site {
        Site::Jable => Ok(jable::get_sidebar_tags()),
        _ => Ok(HashMap::new()),
    }
}
