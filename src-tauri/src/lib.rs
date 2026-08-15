pub mod commands;
pub mod downloader;
pub mod error;
pub mod scraper;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use commands::cf::{start_cf_verifier, sync_cf_configs};
use commands::downloader::{
    cancel_download, pause_download, resume_download, scan_unfinished_tasks, start_download,
};
use commands::media::fetch_preview_video;
use commands::scraper::{fetch_video_list, get_categories, get_sidebar_tags, search_videos};
use commands::system::{
    generate_debug_log, get_disk_space_info, get_folder_size, open_download_folder, select_directory,
};
use commands::updater::check_for_update;
use commands::AppState;
use wreq_util::Emulation;

use sysproxy::Sysproxy;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = wreq::Client::builder()
        .emulation(Emulation::Chrome120)
        .redirect(wreq::redirect::Policy::limited(10));

    // Auto-detect system proxy (Clash Verge / System Proxy)
    let detected_proxy = match Sysproxy::get_system_proxy() {
        Ok(p) if p.enable => {
            let host_str = p.host.to_string();
            let scheme = if host_str.contains("://") {
                ""
            } else {
                "http://"
            };
            Some(format!("{}{}:{}", scheme, host_str, p.port))
        }
        _ => std::env::var("HTTP_PROXY")
            .or_else(|_| std::env::var("http_proxy"))
            .or_else(|_| std::env::var("ALL_PROXY"))
            .or_else(|_| std::env::var("all_proxy"))
            .ok(),
    };

    if let Some(ref proxy_url) = detected_proxy {
        println!("[AVDL] Auto-detected system proxy: {}", proxy_url);
        if let Ok(proxy) = wreq::Proxy::all(proxy_url) {
            builder = builder.proxy(proxy);
        }
    }

    let client = builder.build().expect("failed to build wreq client");

    let task_states = Arc::new(Mutex::new(HashMap::new()));
    let cf_configs = Arc::new(Mutex::new(HashMap::new()));

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            client,
            task_states,
            cf_configs,
        })
        .invoke_handler(tauri::generate_handler![
            fetch_preview_video,
            get_categories,
            fetch_video_list,
            search_videos,
            get_sidebar_tags,
            start_download,
            pause_download,
            resume_download,
            cancel_download,
            scan_unfinished_tasks,
            select_directory,
            open_download_folder,
            get_folder_size,
            get_disk_space_info,
            sync_cf_configs,
            start_cf_verifier,
            generate_debug_log,
            check_for_update,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
