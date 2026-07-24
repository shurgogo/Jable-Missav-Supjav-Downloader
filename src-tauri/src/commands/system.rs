use tauri::Manager;

#[derive(Debug, serde::Serialize)]
pub struct DiskSpaceInfo {
    pub total_space: u64,
    pub available_space: u64,
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

#[tauri::command]
pub async fn generate_debug_log(
    app_handle: tauri::AppHandle,
    save_dir: String,
    log_content: String,
) -> Result<String, String> {
    let mut resolved = std::path::PathBuf::from(&save_dir);
    if save_dir == "download" {
        if let Ok(download_path) = app_handle.path().download_dir() {
            resolved = download_path.join("avdl");
        }
    }

    if !resolved.exists() {
        let _ = std::fs::create_dir_all(&resolved).map_err(|e| e.to_string())?;
    }

    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let file_name = format!("avdl_debug_{}.log", secs);
    let log_path = resolved.join(&file_name);

    std::fs::write(&log_path, log_content).map_err(|e| e.to_string())?;

    Ok(log_path.to_string_lossy().to_string())
}

