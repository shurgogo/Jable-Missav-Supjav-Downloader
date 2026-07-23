pub mod commands;
pub mod downloader;
pub mod error;
pub mod scraper;

use commands::{
    cancel_download, fetch_jable_list, get_jable_categories, pause_download, resume_download,
    search_jable, start_download, get_jable_sidebar_tags, select_directory,
    get_missav_categories, fetch_missav_list, search_missav,
    get_supjav_categories, fetch_supjav_list, search_supjav,
    sync_cf_configs, start_cf_verifier, open_download_folder, get_folder_size, get_disk_space_info, scan_unfinished_tasks, fetch_preview_video, AppState,
};
use wreq_util::Emulation;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let client = wreq::Client::builder()
        .emulation(Emulation::Chrome120)
        .redirect(wreq::redirect::Policy::limited(10))
        .build()
        .expect("failed to build wreq client");

    let task_states = std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
    let cf_configs = std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            client,
            task_states,
            cf_configs,
        })
        .invoke_handler(tauri::generate_handler![
            get_jable_categories,
            fetch_jable_list,
            search_jable,
            start_download,
            pause_download,
            resume_download,
            cancel_download,
            get_jable_sidebar_tags,
            select_directory,
            get_missav_categories,
            fetch_missav_list,
            search_missav,
            get_supjav_categories,
            fetch_supjav_list,
            search_supjav,
            sync_cf_configs,
            start_cf_verifier,
            open_download_folder,
            get_folder_size,
            get_disk_space_info,
            scan_unfinished_tasks,
            fetch_preview_video
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
