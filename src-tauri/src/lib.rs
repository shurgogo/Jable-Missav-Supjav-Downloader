pub mod commands;
pub mod downloader;
pub mod error;
pub mod proxy;
pub mod scraper;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use commands::cf::{start_cf_verifier, sync_cf_configs};
use commands::downloader::{
    cancel_download, pause_download, resume_download, scan_unfinished_tasks, start_download,
};
use commands::media::fetch_preview_video;
use commands::proxy::{apply_proxy_settings, get_proxy_status};
use commands::scraper::{fetch_video_list, get_categories, get_sidebar_tags, search_videos};
use commands::system::{
    generate_debug_log, get_disk_space_info, get_folder_size, open_download_folder, select_directory,
};
use commands::updater::check_for_update;
use commands::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Default proxy mode is "system": detect the OS proxy at startup. The
    // frontend re-applies the persisted user choice (system/direct/custom)
    // right after hydration via `apply_proxy_settings`.
    let detection = proxy::detect_system_proxy();
    let (client, build_warning) = proxy::build_client(detection.proxy.as_ref());

    let proxy_status = Arc::new(Mutex::new(Some(proxy::ProxyStatus {
        mode: "system".to_string(),
        url: detection.proxy.map(|p| p.url()),
        warning: build_warning.or_else(|| detection.warning.map(|w| w.to_string())),
    })));

    let task_states = Arc::new(Mutex::new(HashMap::new()));
    let cf_configs = Arc::new(Mutex::new(HashMap::new()));
    let task_generation = Arc::new(std::sync::atomic::AtomicU64::new(0));

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            client: Mutex::new(client),
            task_states,
            cf_configs,
            task_generation,
            proxy_status,
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
            apply_proxy_settings,
            get_proxy_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
