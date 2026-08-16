use std::sync::atomic::Ordering;
use tauri::{Manager, State};

use crate::downloader::{download_video, TaskControlInfo, TaskControlState};
use super::AppState;

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct UnfinishedTask {
    pub site: Option<crate::scraper::Site>,
    pub url: String,
    pub title: String,
    pub save_dir: String,
    pub max_concurrent: usize,
    pub resolution: String,
    pub total_segments: usize,
    pub completed_segments: usize,
    pub m3u8_url: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DownloadRequest {
    pub site: crate::scraper::Site,
    pub url: String,
    pub save_dir: String,
    pub max_concurrent: usize,
    pub resolution: String,
}

#[tauri::command]
pub async fn start_download(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    req: DownloadRequest,
    window: tauri::Window,
) -> Result<(), String> {
    let client = state.client.clone();
    let task_states = state.task_states.clone();
    let url = req.url.clone();
    let save_dir = req.save_dir.clone();
    let max_concurrent = req.max_concurrent;
    let resolution = req.resolution.clone();
    let site = req.site;

    // Register the task as Running — but reject duplicates so two concurrent
    // tasks never write into the same temp directory / output file.
    {
        let mut states = task_states.lock().unwrap();
        if let Some(info) = states.get(&url) {
            if info.state == TaskControlState::Running {
                return Err(format!("该影片已在下载中，请勿重复添加: {}", url));
            }
        }
        states.insert(
            url.clone(),
            TaskControlInfo {
                state: TaskControlState::Running,
                title: "".to_string(), // Will be updated inside download_video
                save_dir: save_dir.clone(),
                max_concurrent,
                resolution: resolution.clone(),
                generation: state.task_generation.fetch_add(1, Ordering::Relaxed) + 1,
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
        crate::scraper::get_cf_headers_for_site(&configs, site)
    };

    tokio::spawn(async move {
        download_video(
            client,
            site,
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
    site: Option<crate::scraper::Site>,
    url: String,
    save_dir: Option<String>,
    max_concurrent: Option<usize>,
    resolution: Option<String>,
    window: tauri::Window,
) -> Result<(), String> {
    let site_val = site.unwrap_or(crate::scraper::Site::Jable);

    let (target_site, final_save_dir, max_c, res_pref) = {
        let mut states = state.task_states.lock().unwrap();
        if let Some(info) = states.get_mut(&url) {
            if info.state == TaskControlState::Running {
                return Err(format!("该影片已在下载中，请勿重复启动: {}", url));
            }
            info.state = TaskControlState::Running;
            // New instance — bump the generation so any lingering old task
            // for this URL recognises itself as superseded and backs off.
            info.generation = state.task_generation.fetch_add(1, Ordering::Relaxed) + 1;
            (
                site_val,
                info.save_dir.clone(),
                info.max_concurrent,
                info.resolution.clone(),
            )
        } else {
            // Use passed parameters or attempt to scan download directory to recover metadata
            let mut recovered = None;
            let download_dirs = vec![
                save_dir.clone().map(std::path::PathBuf::from),
                app_handle
                    .path()
                    .download_dir()
                    .ok()
                    .map(|d| d.join("avdl")),
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
                                            site: Option<crate::scraper::Site>,
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
                                                    sm.site,
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

            let (r_site, r_save, _r_title, r_max, r_res) = recovered.unwrap_or_else(|| {
                (
                    site,
                    save_dir.unwrap_or_else(|| "download".to_string()),
                    "".to_string(),
                    max_concurrent.unwrap_or(3),
                    resolution.unwrap_or_else(|| "highest".to_string()),
                )
            });

            let target_site = site.or(r_site).unwrap_or(crate::scraper::Site::Jable);

            states.insert(
                url.clone(),
                TaskControlInfo {
                    state: TaskControlState::Running,
                    title: "".to_string(),
                    save_dir: r_save.clone(),
                    max_concurrent: r_max,
                    resolution: r_res.clone(),
                    generation: state.task_generation.fetch_add(1, Ordering::Relaxed) + 1,
                },
            );

            (target_site, r_save, r_max, r_res)
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
        crate::scraper::get_cf_headers_for_site(&configs, target_site)
    };

    tokio::spawn(async move {
        download_video(
            client,
            target_site,
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
    // Set the state to Cancelled but KEEP the registry entry: the running
    // download task observes the flag, stops, deletes its temp folder and
    // removes the entry itself. Removing the entry here would leave the
    // spawned task blind to the cancellation and it would keep downloading.
    let info_opt = {
        let mut states = state.task_states.lock().unwrap();
        if let Some(info) = states.get_mut(&url) {
            info.state = TaskControlState::Cancelled;
        }
        states.get(&url).cloned()
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
                                    site: Option<crate::scraper::Site>,
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
                                        site: sm.site,
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
                                site: None,
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
