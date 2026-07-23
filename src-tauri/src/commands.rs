use crate::downloader::{download_video, TaskControlInfo, TaskControlState};
use crate::scraper::{jable, missav, supjav, Category, TagItem, VideoListResponse};
use tauri::{Emitter, Manager, State};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CfConfig {
    pub cf_clearance: String,
    pub user_agent: String,
}

pub struct AppState {
    pub client: wreq::Client,
    pub task_states: crate::downloader::TaskRegistry,
    pub cf_configs: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, CfConfig>>>,
}

#[tauri::command]
pub async fn sync_cf_configs(
    state: State<'_, AppState>,
    configs: std::collections::HashMap<String, CfConfig>,
) -> Result<(), String> {
    let mut current = state.cf_configs.lock().unwrap();
    *current = configs;
    println!(
        "[AppState] Synchronized Cloudflare configurations for {} domains.",
        current.len()
    );
    Ok(())
}

#[tauri::command]
pub async fn start_cf_verifier(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    url_str: String,
    user_agent: String,
) -> Result<(), String> {
    use tauri::webview::WebviewWindowBuilder;
    use tauri::WebviewUrl;

    let url = url::Url::parse(&url_str).map_err(|e| e.to_string())?;
    let domain = url.host_str().ok_or("Invalid host in URL")?.to_string();

    // 1. Close existing window if any (run on main thread)
    let app_for_close = app.clone();
    let (close_tx, close_rx) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        if let Some(existing) = app_for_close.get_webview_window("cf_verifier") {
            let _ = existing.close();
        }
        let _ = close_tx.send(());
    })
    .map_err(|e| e.to_string())?;
    let _ = close_rx.await;

    // 2. Build the window on the main thread
    let app_for_build = app.clone();
    let url_for_build = url.clone();
    let domain_for_build = domain.clone();
    let (tx, rx) = tokio::sync::oneshot::channel();

    app.run_on_main_thread(move || {
        let builder_res = WebviewWindowBuilder::new(
            &app_for_build,
            "cf_verifier",
            WebviewUrl::External(url_for_build),
        )
        .title(format!("請完成 {} 的 Cloudflare 驗證", domain_for_build))
        .inner_size(680.0, 580.0)
        .resizable(true)
        .focused(true)
        .build();
        let _ = tx.send(builder_res);
    })
    .map_err(|e| e.to_string())?;

    let verifier_win = rx
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;

    // Pre-emptively clear pre-existing cf_clearance cookie from webview session so we start clean
    let verifier_win_clear = verifier_win.clone();
    let url_for_clear = url.clone();
    tokio::spawn(async move {
        for _ in 0..10 {
            if let Ok(cookies) = verifier_win_clear.cookies_for_url(url_for_clear.clone()) {
                let mut found = false;
                for c in cookies {
                    if c.name() == "cf_clearance" {
                        found = true;
                        let _ = verifier_win_clear.delete_cookie(c);
                    }
                }
                if !found {
                    break;
                }
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
        println!("[Verifier] Cookie jar initialized for domain verification.");
    });

    let cf_configs_clone = state.cf_configs.clone();
    let app_clone = app.clone();
    let url_clone = url.clone();
    let domain_clone = domain.clone();
    let user_agent_clone = user_agent.clone();

    tokio::spawn(async move {
        println!(
            "[Verifier] Started background cookie polling for: {}",
            domain_clone
        );
        // Wait 800ms before starting polling to allow launch cookie clearance
        tokio::time::sleep(tokio::time::Duration::from_millis(800)).await;

        let mut last_cf_found: Option<String> = None;

        for _ in 0..240 {
            // poll for up to 120 seconds (500ms intervals)
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

            let verifier_win = match app_clone.get_webview_window("cf_verifier") {
                Some(w) => w,
                None => {
                    println!("[Verifier] Window closed by user.");
                    break;
                }
            };

            if let Ok(cookies) = verifier_win.cookies_for_url(url_clone.clone()) {
                let mut cf_val = None;
                for c in cookies {
                    if c.name() == "cf_clearance" && !c.value().trim().is_empty() {
                        cf_val = Some(c.value().to_string());
                        break;
                    }
                }

                if let Some(cf) = cf_val {
                    println!(
                        "[Verifier] Cloudflare verification completed! Retrieved cf_clearance: {}",
                        cf
                    );
                    last_cf_found = Some(cf.clone());

                    // Save to global configs
                    {
                        let mut configs = cf_configs_clone.lock().unwrap();
                        configs.insert(
                            domain_clone.clone(),
                            CfConfig {
                                cf_clearance: cf.clone(),
                                user_agent: user_agent_clone.clone(),
                            },
                        );
                    }

                    // Emit success to frontend
                    #[derive(serde::Serialize, Clone)]
                    struct SuccessPayload {
                        domain: String,
                        cf_clearance: String,
                        user_agent: String,
                    }
                    let _ = app_clone.emit(
                        "cf-verification-success",
                        SuccessPayload {
                            domain: domain_clone.clone(),
                            cf_clearance: cf,
                            user_agent: user_agent_clone.clone(),
                        },
                    );

                    // Automatically close verifier window on main thread
                    let app_for_close = app_clone.clone();
                    let app_for_close_inner = app_for_close.clone();
                    let _ = app_for_close.run_on_main_thread(move || {
                        if let Some(w) = app_for_close_inner.get_webview_window("cf_verifier") {
                            let _ = w.close();
                        }
                    });
                    break;
                }
            }
        }

        // Fallback: If user manually closed window after cf_clearance was set
        if let Some(cf) = last_cf_found {
            let configs = cf_configs_clone.lock().unwrap();
            let already_saved = configs.contains_key(&domain_clone);
            drop(configs);

            if !already_saved {
                println!(
                    "[Verifier] Window closed with active cf_clearance: {}",
                    cf
                );
                {
                    let mut configs = cf_configs_clone.lock().unwrap();
                    configs.insert(
                        domain_clone.clone(),
                        CfConfig {
                            cf_clearance: cf.clone(),
                            user_agent: user_agent_clone.clone(),
                        },
                    );
                }
                #[derive(serde::Serialize, Clone)]
                struct SuccessPayload {
                    domain: String,
                    cf_clearance: String,
                    user_agent: String,
                }
                let _ = app_clone.emit(
                    "cf-verification-success",
                    SuccessPayload {
                        domain: domain_clone.clone(),
                        cf_clearance: cf,
                        user_agent: user_agent_clone,
                    },
                );
            }
        }

        println!(
            "[Verifier] Ended background cookie polling for: {}",
            domain_clone
        );
    });

    Ok(())
}

#[tauri::command]
pub async fn get_jable_categories(
    state: State<'_, AppState>,
    lang: String,
) -> Result<Vec<Category>, String> {
    Ok(jable::get_categories(&state.client, &lang).await)
}

#[tauri::command]
pub async fn fetch_jable_list(
    state: State<'_, AppState>,
    url: String,
    page: usize,
    sort_by: Option<String>,
    lang: String,
) -> Result<VideoListResponse, String> {
    let (cf_clearance, user_agent) = {
        let configs = state.cf_configs.lock().unwrap();
        crate::scraper::get_cf_headers_for_url(&configs, &url)
    };
    jable::fetch_list(
        &state.client,
        &url,
        page,
        sort_by,
        &lang,
        &cf_clearance,
        &user_agent,
    )
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn search_jable(
    state: State<'_, AppState>,
    keyword: String,
    page: usize,
    sort_by: Option<String>,
    lang: String,
) -> Result<VideoListResponse, String> {
    let (cf_clearance, user_agent) = {
        let configs = state.cf_configs.lock().unwrap();
        crate::scraper::get_cf_headers_for_url(&configs, "https://jable.tv/")
    };
    jable::search_videos(
        &state.client,
        &keyword,
        page,
        sort_by,
        &lang,
        &cf_clearance,
        &user_agent,
    )
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_jable_sidebar_tags(
) -> Result<std::collections::HashMap<String, Vec<TagItem>>, String> {
    Ok(jable::get_sidebar_tags())
}

#[tauri::command]
pub async fn get_missav_categories(
    state: State<'_, AppState>,
    lang: String,
) -> Result<Vec<Category>, String> {
    Ok(missav::get_categories(&state.client, &lang).await)
}

#[tauri::command]
pub async fn fetch_missav_list(
    state: State<'_, AppState>,
    url: String,
    page: usize,
    lang: String,
) -> Result<VideoListResponse, String> {
    let (cf_clearance, user_agent) = {
        let configs = state.cf_configs.lock().unwrap();
        crate::scraper::get_cf_headers_for_url(&configs, &url)
    };
    missav::fetch_list(&state.client, &url, page, &lang, &cf_clearance, &user_agent)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn search_missav(
    state: State<'_, AppState>,
    keyword: String,
    page: usize,
    lang: String,
) -> Result<VideoListResponse, String> {
    let active_domain = missav::get_active_domain();
    let (cf_clearance, user_agent) = {
        let configs = state.cf_configs.lock().unwrap();
        crate::scraper::get_cf_headers_for_url(&configs, &active_domain)
    };
    missav::search_videos(
        &state.client,
        &keyword,
        page,
        &lang,
        &cf_clearance,
        &user_agent,
    )
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_supjav_categories(lang: String) -> Result<Vec<Category>, String> {
    Ok(supjav::get_categories(&lang).await)
}

#[tauri::command]
pub async fn fetch_supjav_list(
    state: State<'_, AppState>,
    url: String,
    page: usize,
    lang: String,
) -> Result<VideoListResponse, String> {
    let (cf_clearance, user_agent) = {
        let configs = state.cf_configs.lock().unwrap();
        crate::scraper::get_cf_headers_for_url(&configs, &url)
    };
    supjav::fetch_list(&state.client, &url, page, &lang, &cf_clearance, &user_agent)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn search_supjav(
    state: State<'_, AppState>,
    keyword: String,
    page: usize,
    lang: String,
) -> Result<VideoListResponse, String> {
    let (cf_clearance, user_agent) = {
        let configs = state.cf_configs.lock().unwrap();
        crate::scraper::get_cf_headers_for_url(&configs, "https://supjav.com/")
    };
    supjav::search_videos(
        &state.client,
        &keyword,
        page,
        &lang,
        &cf_clearance,
        &user_agent,
    )
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn start_download(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    url: String,
    save_dir: String,
    max_concurrent: usize,
    resolution: String,
    window: tauri::Window,
) -> Result<(), String> {
    let client = state.client.clone();
    let task_states = state.task_states.clone();

    // Register the task as Running
    {
        let mut states = task_states.lock().unwrap();
        states.insert(
            url.clone(),
            TaskControlInfo {
                state: TaskControlState::Running,
                title: "".to_string(), // Will be updated inside download_video
                save_dir: save_dir.clone(),
                max_concurrent,
                resolution: resolution.clone(),
            },
        );
    }

    let mut resolved_save_dir = std::path::PathBuf::from(&save_dir);
    if save_dir == "download" {
        if let Ok(download_path) = app_handle.path().download_dir() {
            resolved_save_dir = download_path.join("avdl");
        }
    }

    let resolved_save_dir_str = resolved_save_dir.to_string_lossy().to_string();
    let resolution_clone = resolution.clone();

    let (cf_clearance, user_agent) = {
        let configs = state.cf_configs.lock().unwrap();
        crate::scraper::get_cf_headers_for_url(&configs, &url)
    };

    tokio::spawn(async move {
        download_video(
            client,
            url,
            resolved_save_dir_str,
            max_concurrent,
            resolution_clone,
            window,
            task_states,
            cf_clearance,
            user_agent,
        )
        .await;
    });
    Ok(())
}

#[tauri::command]
pub async fn pause_download(state: State<'_, AppState>, url: String) -> Result<(), String> {
    let mut states = state.task_states.lock().unwrap();
    if let Some(info) = states.get_mut(&url) {
        info.state = TaskControlState::Paused;
        println!("[Commands] Pausing task: {}", url);
    }
    Ok(())
}

#[tauri::command]
pub async fn resume_download(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    url: String,
    save_dir: Option<String>,
    max_concurrent: Option<usize>,
    resolution: Option<String>,
    window: tauri::Window,
) -> Result<(), String> {
    let (final_save_dir, max_c, res_pref) = {
        let mut states = state.task_states.lock().unwrap();
        if let Some(info) = states.get_mut(&url) {
            info.state = TaskControlState::Running;
            (
                info.save_dir.clone(),
                info.max_concurrent,
                info.resolution.clone(),
            )
        } else {
            // Use passed parameters or attempt to scan download directory to recover metadata
            let mut recovered = None;
            let download_dirs = vec![
                save_dir.clone().map(std::path::PathBuf::from),
                app_handle.path().download_dir().ok().map(|d| d.join("avdl")),
            ];

            for dir_opt in download_dirs {
                if let Some(dir) = dir_opt {
                    if let Ok(entries) = std::fs::read_dir(dir) {
                        for entry in entries.flatten() {
                            if entry.path().is_dir() {
                                let meta_path = entry.path().join("task_metadata.json");
                                if meta_path.exists() {
                                    if let Ok(content) = std::fs::read_to_string(&meta_path) {
                                        #[derive(serde::Deserialize)]
                                        struct SavedMeta {
                                            url: String,
                                            title: String,
                                            save_dir: String,
                                            max_concurrent: usize,
                                            resolution: String,
                                        }
                                        if let Ok(sm) = serde_json::from_str::<SavedMeta>(&content)
                                        {
                                            if sm.url == url {
                                                recovered = Some((
                                                    sm.save_dir,
                                                    sm.title,
                                                    sm.max_concurrent,
                                                    sm.resolution,
                                                ));
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            let (r_save, _r_title, r_max, r_res) = recovered.unwrap_or_else(|| {
                (
                    save_dir.unwrap_or_else(|| "download".to_string()),
                    "".to_string(),
                    max_concurrent.unwrap_or(3),
                    resolution.unwrap_or_else(|| "highest".to_string()),
                )
            });

            states.insert(
                url.clone(),
                TaskControlInfo {
                    state: TaskControlState::Running,
                    title: "".to_string(),
                    save_dir: r_save.clone(),
                    max_concurrent: r_max,
                    resolution: r_res.clone(),
                },
            );

            (r_save, r_max, r_res)
        }
    };

    let mut resolved_save_dir = std::path::PathBuf::from(&final_save_dir);
    if final_save_dir == "download" {
        if let Ok(download_path) = app_handle.path().download_dir() {
            resolved_save_dir = download_path.join("avdl");
        }
    }

    let resolved_save_dir_str = resolved_save_dir.to_string_lossy().to_string();
    let client = state.client.clone();
    let task_states = state.task_states.clone();

    let (cf_clearance, user_agent) = {
        let configs = state.cf_configs.lock().unwrap();
        crate::scraper::get_cf_headers_for_url(&configs, &url)
    };

    tokio::spawn(async move {
        download_video(
            client,
            url,
            resolved_save_dir_str,
            max_c,
            res_pref,
            window,
            task_states,
            cf_clearance,
            user_agent,
        )
        .await;
    });

    Ok(())
}

fn delete_temp_folder(save_dir: &str, url: &str, title: &str, app_handle: &tauri::AppHandle) {
    let mut resolved_save_dir = std::path::PathBuf::from(save_dir);
    if save_dir == "download" {
        if let Ok(download_path) = app_handle.path().download_dir() {
            resolved_save_dir = download_path.join("avdl");
        }
    }

    let re_code = regex::Regex::new(r"(?i)([a-z0-9]{2,10}-\d{3,6})").unwrap();
    let mut deleted = false;

    // 1. Try to find and delete by URL AV code matching
    if let Some(caps) = re_code.captures(url) {
        let url_code = caps.get(1).unwrap().as_str().to_lowercase();
        if let Ok(entries) = std::fs::read_dir(&resolved_save_dir) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.starts_with("temp_") {
                        let folder_title = name.strip_prefix("temp_").unwrap_or(&name).to_string();
                        if let Some(folder_caps) = re_code.captures(&folder_title) {
                            let folder_code = folder_caps.get(1).unwrap().as_str().to_lowercase();
                            if folder_code == url_code {
                                let _ = std::fs::remove_dir_all(entry.path());
                                println!(
                                    "[Commands] Deleted temp directory by code match: {:?}",
                                    entry.path()
                                );
                                deleted = true;
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    // 2. Fallback to exact title-based deletion if not deleted by code
    if !deleted {
        let safe_title = crate::downloader::sanitize_filename(title);
        let temp_dir_path = resolved_save_dir.join(format!("temp_{}", safe_title));
        if temp_dir_path.exists() {
            let _ = std::fs::remove_dir_all(&temp_dir_path);
            println!(
                "[Commands] Deleted temp directory by title fallback: {:?}",
                temp_dir_path
            );
        }
    }
}

#[tauri::command]
pub async fn cancel_download(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    url: String,
) -> Result<(), String> {
    println!("[Commands] Cancelling task: {}", url);
    let info_opt = {
        let mut states = state.task_states.lock().unwrap();
        if let Some(info) = states.get_mut(&url) {
            info.state = TaskControlState::Cancelled;
        }
        states.remove(&url)
    };

    if let Some(info) = info_opt {
        delete_temp_folder(&info.save_dir, &url, &info.title, &app_handle);
    } else {
        // Fallback for recovered/paused tasks
        delete_temp_folder("download", &url, "", &app_handle);
    }

    Ok(())
}

#[tauri::command]
pub async fn select_directory() -> Result<Option<String>, String> {
    let result = tokio::task::spawn_blocking(|| {
        rfd::FileDialog::new()
            .pick_folder()
            .map(|path| path.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| e.to_string())?;

    Ok(result)
}

#[tauri::command]
pub async fn open_download_folder(
    app_handle: tauri::AppHandle,
    save_dir: String,
) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;

    let mut resolved = std::path::PathBuf::from(&save_dir);
    if save_dir == "download" {
        if let Ok(download_path) = app_handle.path().download_dir() {
            resolved = download_path.join("avdl");
        }
    }

    if !resolved.exists() {
        let _ = std::fs::create_dir_all(&resolved);
    }

    app_handle
        .opener()
        .open_path(
            resolved.to_string_lossy().to_string(),
            Option::<String>::None,
        )
        .map_err(|e| e.to_string())
}

fn get_dir_size(path: &std::path::Path) -> u64 {
    let mut total = 0;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_dir() {
                    total += get_dir_size(&entry.path());
                } else {
                    total += meta.len();
                }
            }
        }
    }
    total
}

#[tauri::command]
pub async fn get_folder_size(
    app_handle: tauri::AppHandle,
    save_dir: String,
) -> Result<u64, String> {
    let mut resolved = std::path::PathBuf::from(&save_dir);
    if save_dir == "download" {
        if let Ok(download_path) = app_handle.path().download_dir() {
            resolved = download_path.join("avdl");
        }
    }

    if !resolved.exists() {
        return Ok(0);
    }

    let total_size = tokio::task::spawn_blocking(move || get_dir_size(&resolved))
        .await
        .map_err(|e| e.to_string())?;

    Ok(total_size)
}

#[derive(Debug, serde::Serialize)]
pub struct DiskSpaceInfo {
    pub total_space: u64,
    pub available_space: u64,
}

#[tauri::command]
pub async fn get_disk_space_info(
    app_handle: tauri::AppHandle,
    save_dir: String,
) -> Result<DiskSpaceInfo, String> {
    use sysinfo::Disks;

    let mut resolved = std::path::PathBuf::from(&save_dir);
    if save_dir == "download" {
        if let Ok(download_path) = app_handle.path().download_dir() {
            resolved = download_path.join("avdl");
        }
    }

    let abs_path = std::fs::canonicalize(&resolved).unwrap_or_else(|_| resolved.clone());

    let disks = Disks::new_with_refreshed_list();

    let mut best_match: Option<&sysinfo::Disk> = None;
    let mut best_match_len = 0;

    for disk in &disks {
        let mount_path = disk.mount_point();
        if abs_path.starts_with(mount_path) {
            let mount_str = mount_path.to_string_lossy();
            if mount_str.len() > best_match_len {
                best_match_len = mount_str.len();
                best_match = Some(disk);
            }
        }
    }

    if let Some(disk) = best_match {
        Ok(DiskSpaceInfo {
            total_space: disk.total_space(),
            available_space: disk.available_space(),
        })
    } else {
        if let Some(first_disk) = disks.iter().next() {
            Ok(DiskSpaceInfo {
                total_space: first_disk.total_space(),
                available_space: first_disk.available_space(),
            })
        } else {
            Err("No disks found".to_string())
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct UnfinishedTask {
    pub url: String,
    pub title: String,
    pub save_dir: String,
    pub max_concurrent: usize,
    pub resolution: String,
    pub total_segments: usize,
    pub completed_segments: usize,
    pub m3u8_url: String,
}

#[tauri::command]
pub async fn scan_unfinished_tasks(
    app_handle: tauri::AppHandle,
    save_dir: String,
) -> Result<Vec<UnfinishedTask>, String> {
    let mut resolved = std::path::PathBuf::from(&save_dir);
    if save_dir == "download" {
        if let Ok(download_path) = app_handle.path().download_dir() {
            resolved = download_path.join("avdl");
        }
    }

    if !resolved.exists() {
        return Ok(Vec::new());
    }

    let tasks = tokio::task::spawn_blocking(move || {
        let mut list = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&resolved) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let name = path.file_name().unwrap_or_default().to_string_lossy();
                    if name.starts_with("temp_") {
                        let meta_path = path.join("task_metadata.json");
                        let mut loaded = false;

                        if meta_path.exists() {
                            if let Ok(content) = std::fs::read_to_string(&meta_path) {
                                #[derive(serde::Deserialize)]
                                struct SavedMeta {
                                    url: String,
                                    title: String,
                                    save_dir: String,
                                    max_concurrent: usize,
                                    resolution: String,
                                    total_segments: usize,
                                    m3u8_url: String,
                                }
                                if let Ok(sm) = serde_json::from_str::<SavedMeta>(&content) {
                                    let mut completed_segments = 0;
                                    if let Ok(sub_entries) = std::fs::read_dir(&path) {
                                        for sub_entry in sub_entries.flatten() {
                                            if let Some(ext) = sub_entry.path().extension() {
                                                if ext == "ts" {
                                                    completed_segments += 1;
                                                }
                                            }
                                        }
                                    }

                                    list.push(UnfinishedTask {
                                        url: sm.url,
                                        title: sm.title,
                                        save_dir: sm.save_dir,
                                        max_concurrent: sm.max_concurrent,
                                        resolution: sm.resolution,
                                        total_segments: sm.total_segments,
                                        completed_segments,
                                        m3u8_url: sm.m3u8_url,
                                    });
                                    loaded = true;
                                }
                            }
                        }

                        // Fallback parsing if metadata file doesn't exist (compatibility with older downloads)
                        if !loaded {
                            let title = name.strip_prefix("temp_").unwrap_or(&name).to_string();
                            let re_code =
                                regex::Regex::new(r"(?i)([a-z0-9]{2,10}-\d{3,6})").unwrap();
                            let code_opt = re_code
                                .captures(&title)
                                .map(|caps| caps.get(1).unwrap().as_str().to_lowercase());

                            let url = if let Some(ref c) = code_opt {
                                format!("https://jable.tv/videos/{}/", c)
                            } else {
                                format!("https://jable.tv/search/{}/", title)
                            };

                            let mut completed_segments = 0;
                            if let Ok(sub_entries) = std::fs::read_dir(&path) {
                                for sub_entry in sub_entries.flatten() {
                                    if let Some(ext) = sub_entry.path().extension() {
                                        if ext == "ts" {
                                            completed_segments += 1;
                                        }
                                    }
                                }
                            }

                            let total_segments = if completed_segments > 0 {
                                completed_segments + 10
                            } else {
                                100
                            };

                            list.push(UnfinishedTask {
                                url,
                                title,
                                save_dir: save_dir.clone(),
                                max_concurrent: 3,
                                resolution: "highest".to_string(),
                                total_segments,
                                completed_segments,
                                m3u8_url: "".to_string(),
                            });
                        }
                    }
                }
            }
        }
        list
    })
    .await
    .map_err(|e| e.to_string())?;

    Ok(tasks)
}

#[tauri::command]
pub async fn fetch_preview_video(
    state: tauri::State<'_, AppState>,
    url: String,
) -> Result<Vec<u8>, String> {
    let mut req = state
        .client
        .get(&url)
        .header(
            "User-Agent",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
        )
        .header("Accept", "*/*");

    if url.contains("jable.tv") || url.contains("jable") {
        req = req.header("Referer", "https://jable.tv/");
    } else if url.contains("fourhoi.com") || url.contains("surrit.com") || url.contains("missav") {
        let active_domain = crate::scraper::missav::get_active_domain();
        req = req.header("Referer", format!("{}/", active_domain));
    }

    let resp = req.send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP status {}", resp.status()));
    }

    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    Ok(bytes.to_vec())
}
