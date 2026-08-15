use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::Emitter;
use tokio::sync::Semaphore;
use url::Url;
use wreq::Client;

use crate::scraper::Site;

use super::m3u8::parse_master_or_media_m3u8;
use super::parsers::{parse_missav_page, parse_supjav_page, JableVideoPageInfo};
use super::task::{ProgressPayload, TaskControlState, TaskRegistry};
use super::utils::{
    apply_cf_headers_safe, apply_referer, get_iv_for_segment, sanitize_filename, strip_fake_header,
};

pub async fn download_video(
    client: Client,
    site: Site,
    url: String,
    save_dir: String,
    max_concurrent: usize,
    resolution_pref: String,
    window: tauri::Window,
    task_states: TaskRegistry,
    cf_clearance: String,
    user_agent: String,
) {
    let emit_fail = |err_msg: &str| {
        let msg = cloudflare_hint(err_msg);
        let _ = window.emit(
            "download-progress",
            ProgressPayload {
                url: url.clone(),
                title: "".to_string(),
                index: 0,
                total: 0,
                speed_kbps: 0.0,
                status: format!("failed: {}", msg),
            },
        );
        // The task is finished — drop its control entry so a retry/re-download
        // is not blocked by a stale "Running" state.
        let mut states = task_states.lock().unwrap();
        states.remove(&url);
    };

    println!("[Downloader] Starting/Resuming download for: {}", url);

    // Step 1: Parse page HTML
    let page_info = match site {
        Site::Missav => {
            let referer_url = if let Ok(parsed_url) = Url::parse(&url) {
                format!(
                    "{}://{}/",
                    parsed_url.scheme(),
                    parsed_url.host_str().unwrap_or("missav.ai")
                )
            } else {
                "https://missav.ai/".to_string()
            };

            let mut req = client.get(&url)
                .header("accept", "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8")
                .header("accept-language", "zh-TW,zh;q=0.9,en-US;q=0.8,en;q=0.7")
                .header("referer", &referer_url);

            req = crate::scraper::apply_cf_headers(req, &cf_clearance, &user_agent);

            match req.send().await {
                Ok(resp) => match resp.text().await {
                    Ok(text) => match parse_missav_page(&text) {
                        Ok(info) => info,
                        Err(e) => {
                            emit_fail(&format!("Failed to parse MissAV page: {}", e));
                            return;
                        }
                    },
                    Err(e) => {
                        emit_fail(&format!("Failed to read MissAV body: {}", e));
                        return;
                    }
                },
                Err(e) => {
                    emit_fail(&format!("Failed to request MissAV page: {}", e));
                    return;
                }
            }
        }
        Site::Supjav => match parse_supjav_page(&client, &url, &cf_clearance, &user_agent).await {
            Ok(info) => info,
            Err(e) => {
                emit_fail(&format!("Failed to parse SupJav page: {}", e));
                return;
            }
        },
        Site::Jable => {
            let mut req = client.get(&url).header("referer", "https://jable.tv/");
            req = crate::scraper::apply_cf_headers(req, &cf_clearance, &user_agent);

            match req.send().await {
                Ok(resp) => match resp.text().await {
                    Ok(html) => {
                        let doc = dom_query::Document::from(html.as_str());
                        let h1_text = doc.select("h1").text().to_string().trim().to_string();
                        let title = if !h1_text.is_empty() {
                            h1_text
                        } else {
                            doc.select("title").text().to_string().trim().to_string()
                        };

                        let mut m3u8_url = "".to_string();
                        let re_m3u8 = regex::Regex::new(r#"https?://[^\s'"\\]+\.m3u8"#).unwrap();
                        for node in doc.select("script").iter() {
                            let text = node.text();
                            if text.contains("hls") || text.contains("m3u8") {
                                if let Some(m) = re_m3u8.find(&text) {
                                    m3u8_url = m.as_str().to_string();
                                    break;
                                }
                            }
                        }

                        if m3u8_url.is_empty() {
                            emit_fail("Could not find JableTV M3U8 link in scripts");
                            return;
                        }

                        JableVideoPageInfo { title, m3u8_url }
                    }
                    Err(e) => {
                        emit_fail(&format!("Failed to read JableTV body: {}", e));
                        return;
                    }
                },
                Err(e) => {
                    emit_fail(&format!("Failed to request JableTV page: {}", e));
                    return;
                }
            }
        }
    };

    let safe_title = sanitize_filename(&page_info.title);

    // Register active metadata in the global registry
    {
        let mut states = task_states.lock().unwrap();
        if let Some(info) = states.get_mut(&url) {
            info.title = page_info.title.clone();
            info.save_dir = save_dir.clone();
            info.max_concurrent = max_concurrent;
            info.resolution = resolution_pref.clone();
        }
    }

    // Direct MP4 check
    let is_direct_mp4 =
        page_info.m3u8_url.contains(".mp4") || !page_info.m3u8_url.contains(".m3u8");
    if is_direct_mp4 {
        println!(
            "[Downloader] Direct MP4 stream detected. Downloading file directly: {}",
            page_info.m3u8_url
        );

        let base_save_path = std::path::PathBuf::from(&save_dir);
        let final_mp4_path = base_save_path.join(format!("{}.mp4", safe_title));

        // Check if file already exists
        if final_mp4_path.exists() {
            println!(
                "[Downloader] Direct MP4 file already exists: {:?}",
                final_mp4_path
            );

            // Remove from registry
            {
                let mut states = task_states.lock().unwrap();
                states.remove(&url);
            }

            // Completed
            let _ = window.emit(
                "download-progress",
                ProgressPayload {
                    url: url.clone(),
                    title: page_info.title,
                    index: 100,
                    total: 100,
                    speed_kbps: 0.0,
                    status: "completed".to_string(),
                },
            );
            return;
        }

        let mut req = client.get(&page_info.m3u8_url);
        if page_info.m3u8_url.contains("streamtape") {
            req = req.header("referer", "https://streamtape.com/");
            if !user_agent.trim().is_empty() {
                req = req.header("user-agent", user_agent.trim());
            }
        } else {
            req = apply_referer(req, &url);
            req = apply_cf_headers_safe(req, &page_info.m3u8_url, &url, &cf_clearance, &user_agent);
        }

        match req.send().await {
            Ok(resp) => {
                if !resp.status().is_success() {
                    emit_fail(&format!(
                        "Direct MP4 download failed: HTTP {}",
                        resp.status()
                    ));
                    return;
                }

                let total_bytes = resp.content_length().unwrap_or(0);
                println!("[Downloader] Direct MP4 file size: {} bytes", total_bytes);

                let mut file = match std::fs::File::create(&final_mp4_path) {
                    Ok(f) => f,
                    Err(e) => {
                        emit_fail(&format!("Failed to create output file: {}", e));
                        return;
                    }
                };

                let stream = resp.bytes_stream();
                use futures_util::StreamExt;
                let mut stream_pin = Box::pin(stream);

                let mut downloaded = 0;
                let mut last_emit = std::time::Instant::now();
                let mut last_emit_bytes = 0usize;

                while let Some(chunk_result) = stream_pin.next().await {
                    // Check task state (pause/cancel)
                    {
                        let states = task_states.lock().unwrap();
                        if let Some(info) = states.get(&url) {
                            if info.state == TaskControlState::Paused
                                || info.state == TaskControlState::Cancelled
                            {
                                drop(file);
                                if info.state == TaskControlState::Cancelled {
                                    let _ = std::fs::remove_file(&final_mp4_path);
                                }
                                return;
                            }
                        }
                    }

                    match chunk_result {
                        Ok(chunk) => {
                            use std::io::Write;
                            if let Err(e) = file.write_all(&chunk) {
                                emit_fail(&format!("Failed to write to file: {}", e));
                                return;
                            }
                            downloaded += chunk.len();

                            // Emit progress every 1000ms with instantaneous speed
                            if last_emit.elapsed().as_millis() > 1000 {
                                let now = std::time::Instant::now();
                                let dt = now.duration_since(last_emit).as_secs_f64();
                                let db = downloaded.saturating_sub(last_emit_bytes);
                                last_emit = now;
                                last_emit_bytes = downloaded;
                                let progress_percent = if total_bytes > 0 {
                                    (downloaded as f64 / total_bytes as f64 * 100.0) as usize
                                } else {
                                    50
                                };

                                let current_speed = if dt > 0.0 {
                                    (db as f64) / 1024.0 / dt
                                } else {
                                    0.0
                                };

                                let _ = window.emit(
                                    "download-progress",
                                    ProgressPayload {
                                        url: url.clone(),
                                        title: page_info.title.clone(),
                                        index: progress_percent,
                                        total: 100,
                                        speed_kbps: current_speed,
                                        status: "downloading".to_string(),
                                    },
                                );
                            }
                        }
                        Err(e) => {
                            emit_fail(&format!("Error downloading chunk: {}", e));
                            return;
                        }
                    }
                }

                // Remove from registry
                {
                    let mut states = task_states.lock().unwrap();
                    states.remove(&url);
                }

                // Completed
                let _ = window.emit(
                    "download-progress",
                    ProgressPayload {
                        url: url.clone(),
                        title: page_info.title,
                        index: 100,
                        total: 100,
                        speed_kbps: 0.0,
                        status: "completed".to_string(),
                    },
                );
                return;
            }
            Err(e) => {
                emit_fail(&format!("Failed to request direct MP4 stream: {}", e));
                return;
            }
        }
    }

    // Step 2: Parse Master or Media M3U8 based on resolution preference
    let m3u8_info = match parse_master_or_media_m3u8(
        &client,
        &page_info.m3u8_url,
        &resolution_pref,
        &url,
        &cf_clearance,
        &user_agent,
    )
    .await
    {
        Ok(info) => info,
        Err(e) => {
            emit_fail(&format!("Failed to parse M3U8 playlist: {}", e));
            return;
        }
    };

    // Step 3: Fetch Encryption Key if needed
    let mut key_bytes = None;
    if let Some(ref k_url) = m3u8_info.key_url {
        println!(
            "[Downloader] Stream is encrypted. Fetching key from: {}",
            k_url
        );
        let mut req = client.get(k_url);
        req = apply_referer(req, &url);
        req = apply_cf_headers_safe(req, k_url, &url, &cf_clearance, &user_agent);
        match req.send().await {
            Ok(resp) => match resp.bytes().await {
                Ok(bytes) => {
                    key_bytes = Some(bytes.to_vec());
                }
                Err(e) => {
                    emit_fail(&format!("Failed to download decryption key: {}", e));
                    return;
                }
            },
            Err(e) => {
                emit_fail(&format!("Failed to request decryption key: {}", e));
                return;
            }
        }
    }

    // Step 4: Create temp directory
    let base_save_path = PathBuf::from(&save_dir);

    // Final merged output paths (shared by the exists-check, merge and cleanup)
    let final_mp4_path = base_save_path.join(format!("{}.mp4", safe_title));
    let final_ts_path = base_save_path.join(format!("{}.ts", safe_title));

    // Check if the final merged video file already exists (non-empty only —
    // a truncated leftover from a crashed merge must not count as completed)
    let valid_final_exists = (final_mp4_path.exists()
        && final_mp4_path.metadata().map(|m| m.len()).unwrap_or(0) > 0)
        || (final_ts_path.exists() && final_ts_path.metadata().map(|m| m.len()).unwrap_or(0) > 0);
    if valid_final_exists {
        println!(
            "[Downloader] Target merged video file already exists: {:?}",
            final_mp4_path
        );

        // Clean up any stale temp directory
        let temp_dir_name = format!("temp_{}", safe_title);
        let temp_dir_path = base_save_path.join(&temp_dir_name);
        let _ = std::fs::remove_dir_all(&temp_dir_path);

        // Remove from registry
        {
            let mut states = task_states.lock().unwrap();
            states.remove(&url);
        }

        // Completed
        let total_segs = m3u8_info.segments.len();
        let _ = window.emit(
            "download-progress",
            ProgressPayload {
                url: url.clone(),
                title: page_info.title,
                index: total_segs,
                total: total_segs,
                speed_kbps: 0.0,
                status: "completed".to_string(),
            },
        );
        return;
    }
    // A partial/empty final file from an earlier failed merge: remove it so it
    // cannot shadow a future download or get appended to.
    let _ = std::fs::remove_file(&final_mp4_path);
    let _ = std::fs::remove_file(&final_ts_path);

    if base_save_path.is_file() {
        emit_fail("Specified save path is a file, not a directory. Please select a valid download folder in Settings.");
        return;
    }

    if !base_save_path.exists() {
        if let Err(e) = std::fs::create_dir_all(&base_save_path) {
            emit_fail(&format!("Failed to create save directory: {}. Please verify folder write permissions or select a valid path in Settings.", e));
            return;
        }
    }

    // Code-based folder lookup to match existing folders even if titles differ slightly
    let mut resolved_temp_path = None;
    let re_code = regex::Regex::new(r"(?i)([a-z0-9]{2,10}-\d{3,6})").unwrap();
    if let Some(caps) = re_code.captures(&url) {
        let url_code = caps.get(1).unwrap().as_str().to_lowercase();
        if let Ok(entries) = std::fs::read_dir(&base_save_path) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.starts_with("temp_") {
                        let folder_title = name.strip_prefix("temp_").unwrap_or(&name).to_string();
                        if let Some(folder_caps) = re_code.captures(&folder_title) {
                            let folder_code = folder_caps.get(1).unwrap().as_str().to_lowercase();
                            if folder_code == url_code {
                                resolved_temp_path = Some(entry.path());
                                println!("[Downloader] Found and reusing existing temp directory by code match: {:?}", entry.path());
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    let temp_dir_path = resolved_temp_path.unwrap_or_else(|| {
        let temp_dir_name = format!("temp_{}", safe_title);
        base_save_path.join(&temp_dir_name)
    });

    if temp_dir_path.is_file() {
        emit_fail("Temporary folder path conflicts with an existing file. Please remove the conflicting file or select another directory.");
        return;
    }

    if !temp_dir_path.exists() {
        if let Err(e) = std::fs::create_dir_all(&temp_dir_path) {
            emit_fail(&format!("Failed to create temporary folder: {}. Please verify folder write permissions or select a valid path in Settings.", e));
            return;
        }
    }

    // Save metadata for crash recovery
    #[derive(serde::Serialize)]
    struct TaskMetadata {
        site: Site,
        url: String,
        title: String,
        save_dir: String,
        max_concurrent: usize,
        resolution: String,
        total_segments: usize,
        m3u8_url: String,
    }
    let meta = TaskMetadata {
        site,
        url: url.clone(),
        title: page_info.title.clone(),
        save_dir: save_dir.clone(),
        max_concurrent,
        resolution: resolution_pref.clone(),
        total_segments: m3u8_info.segments.len(),
        m3u8_url: page_info.m3u8_url.clone(),
    };
    if let Ok(meta_json) = serde_json::to_string(&meta) {
        let _ = std::fs::write(temp_dir_path.join("task_metadata.json"), meta_json);
    }

    // fMP4 streams carry an init segment (#EXT-X-MAP). It must be fetched
    // before the media segments and prepended when merging, otherwise the
    // concatenated fragments are not playable. Ignored for TS playlists.
    let init_path = temp_dir_path.join("init.mp4");
    if let Some(init) = &m3u8_info.init_segment {
        println!("[Downloader] Downloading fMP4 init segment: {}", init.url);
        let mut req = client.get(&init.url);
        req = apply_referer(req, &url);
        req = apply_cf_headers_safe(req, &init.url, &url, &cf_clearance, &user_agent);
        if let Some((start, len)) = init.byte_range {
            let end = start + len.saturating_sub(1);
            req = req.header("range", format!("bytes={}-{}", start, end));
        }
        let init_data = match req.send().await {
            Ok(resp) => {
                if !resp.status().is_success() {
                    emit_fail(&format!(
                        "Failed to download init segment: HTTP {}",
                        resp.status()
                    ));
                    return;
                }
                match resp.bytes().await {
                    Ok(bytes) => bytes.to_vec(),
                    Err(e) => {
                        emit_fail(&format!("Failed to read init segment: {}", e));
                        return;
                    }
                }
            }
            Err(e) => {
                emit_fail(&format!("Failed to request init segment: {}", e));
                return;
            }
        };
        let mut init_data = init_data;
        // If the stream is AES-128 encrypted, the init segment is encrypted
        // too (same key, IV of the first media segment).
        if let Some(ref key) = key_bytes {
            let iv_bytes = get_iv_for_segment(0, &m3u8_info.iv);
            use aes::cipher::{block_padding::NoPadding, BlockDecryptMut, KeyIvInit};
            type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;
            if let Ok(dec) = Aes128CbcDec::new_from_slices(key, &iv_bytes) {
                if let Ok(decrypted) = dec.decrypt_padded_mut::<NoPadding>(&mut init_data) {
                    init_data = decrypted.to_vec();
                }
            }
        }
        if let Err(e) = std::fs::write(&init_path, &init_data) {
            emit_fail(&format!("Failed to write init segment: {}", e));
            return;
        }
    }

    // Step 5: Concurrent Downloading
    let total_segments = m3u8_info.segments.len();
    let semaphore = Arc::new(Semaphore::new(max_concurrent.clamp(1, 16)));
    let completed_count = Arc::new(AtomicUsize::new(0));
    let bytes_count = Arc::new(AtomicUsize::new(0));
    let speed_tracker: Arc<Mutex<Option<(Instant, usize)>>> = Arc::new(Mutex::new(None));
    let segments_done = Arc::new(AtomicBool::new(false));

    let key_bytes_arc = Arc::new(key_bytes);
    let custom_iv_arc = Arc::new(m3u8_info.iv);

    // Periodic progress reporter: keeps the UI alive while a (possibly slow)
    // segment is in flight and shows a real-time (instantaneous) speed instead
    // of a stale average.
    {
        let segments_done = Arc::clone(&segments_done);
        let completed = Arc::clone(&completed_count);
        let bytes = Arc::clone(&bytes_count);
        let speed_tracker = Arc::clone(&speed_tracker);
        let window_clone = window.clone();
        let url_clone = url.clone();
        let title_clone = page_info.title.clone();
        let task_states_clone = Arc::clone(&task_states);

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(1000)).await;
                if segments_done.load(Ordering::Relaxed) {
                    break;
                }
                let ctl = {
                    let states = task_states_clone.lock().unwrap();
                    states.get(&url_clone).map(|i| i.state)
                };
                if ctl == Some(TaskControlState::Paused) || ctl == Some(TaskControlState::Cancelled)
                {
                    break;
                }
                let speed = instant_speed(
                    &speed_tracker,
                    Instant::now(),
                    bytes.load(Ordering::Relaxed),
                );
                let _ = window_clone.emit(
                    "download-progress",
                    ProgressPayload {
                        url: url_clone.clone(),
                        title: title_clone.clone(),
                        index: completed.load(Ordering::Relaxed),
                        total: total_segments,
                        speed_kbps: speed,
                        status: "downloading".to_string(),
                    },
                );
            }
        });
    }

    let mut join_handles = Vec::new();

    for (index, segment) in m3u8_info.segments.iter().enumerate() {
        let sem = Arc::clone(&semaphore);
        let completed = Arc::clone(&completed_count);
        let bytes = Arc::clone(&bytes_count);
        let speed_tracker = Arc::clone(&speed_tracker);
        let client_clone = client.clone();
        let segment_url_clone = segment.url.clone();
        let byte_range = segment.byte_range;
        let temp_path = temp_dir_path.clone();
        let key_bytes_clone = Arc::clone(&key_bytes_arc);
        let custom_iv_clone = Arc::clone(&custom_iv_arc);
        let task_states_clone = Arc::clone(&task_states);

        let window_clone = window.clone();
        let url_clone = url.clone();
        let title_clone = page_info.title.clone();
        let cf_clearance_clone = cf_clearance.clone();
        let user_agent_clone = user_agent.clone();

        let handle = tokio::spawn(async move {
            let _permit = sem.acquire().await.map_err(|e| e.to_string())?;

            match download_segment(
                client_clone,
                index,
                segment_url_clone,
                byte_range,
                temp_path,
                (*key_bytes_clone).clone(),
                (*custom_iv_clone).clone(),
                site,
                Arc::clone(&task_states_clone),
                url_clone.clone(),
                cf_clearance_clone,
                user_agent_clone,
                Arc::clone(&bytes),
            )
            .await
            {
                Ok(_) => {
                    let done = completed.fetch_add(1, Ordering::Relaxed) + 1;
                    let total_bytes = bytes.load(Ordering::Relaxed);
                    let speed = instant_speed(&speed_tracker, Instant::now(), total_bytes);
                    let _ = window_clone.emit(
                        "download-progress",
                        ProgressPayload {
                            url: url_clone,
                            title: title_clone,
                            index: done,
                            total: total_segments,
                            speed_kbps: speed,
                            status: "downloading".to_string(),
                        },
                    );
                    Ok::<(), String>(())
                }
                Err(e) => Err(e),
            }
        });

        join_handles.push(handle);
    }

    // Wait for all worker handles and collect the segments that failed.
    let mut failed_indices: Vec<usize> = Vec::new();
    for (index, handle) in join_handles.into_iter().enumerate() {
        match handle.await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                println!("[Downloader] Segment {} failed: {}", index, e);
                failed_indices.push(index);
            }
            Err(e) => {
                println!("[Downloader] Segment {} task panicked: {}", index, e);
                failed_indices.push(index);
            }
        }
    }

    // Retry failed segments (transient network errors / CF throttling).
    let mut attempt = 0;
    while !failed_indices.is_empty() && attempt < 3 {
        attempt += 1;
        tokio::time::sleep(Duration::from_millis(300 * attempt as u64)).await;

        // Stop retrying if the task was paused/cancelled meanwhile.
        let ctl = {
            let states = task_states.lock().unwrap();
            states.get(&url).map(|i| i.state)
        };
        if ctl == Some(TaskControlState::Paused) || ctl == Some(TaskControlState::Cancelled) {
            break;
        }

        let mut still_failed = Vec::new();
        for &index in &failed_indices {
            match download_segment(
                client.clone(),
                index,
                m3u8_info.segments[index].url.clone(),
                m3u8_info.segments[index].byte_range,
                temp_dir_path.clone(),
                (*key_bytes_arc).clone(),
                (*custom_iv_arc).clone(),
                site,
                Arc::clone(&task_states),
                url.clone(),
                cf_clearance.clone(),
                user_agent.clone(),
                Arc::clone(&bytes_count),
            )
            .await
            {
                Ok(_) => {
                    let done = completed_count.fetch_add(1, Ordering::Relaxed) + 1;
                    let total_bytes = bytes_count.load(Ordering::Relaxed);
                    let speed = instant_speed(&speed_tracker, Instant::now(), total_bytes);
                    let _ = window.emit(
                        "download-progress",
                        ProgressPayload {
                            url: url.clone(),
                            title: page_info.title.clone(),
                            index: done,
                            total: total_segments,
                            speed_kbps: speed,
                            status: "downloading".to_string(),
                        },
                    );
                }
                Err(e) => {
                    println!(
                        "[Downloader] Segment {} retry {}/3 failed: {}",
                        index, attempt, e
                    );
                    still_failed.push(index);
                }
            }
        }
        failed_indices = still_failed;
    }

    segments_done.store(true, Ordering::Relaxed);

    // Re-evaluate control state
    let final_state = {
        let states = task_states.lock().unwrap();
        states
            .get(&url)
            .map(|info| info.state)
            .unwrap_or(TaskControlState::Running)
    };

    if final_state == TaskControlState::Cancelled {
        println!("[Downloader] Task cancelled. Deleting temp folder.");
        let _ = std::fs::remove_dir_all(&temp_dir_path);
        // Remove from registry
        let mut states = task_states.lock().unwrap();
        states.remove(&url);
        return;
    }

    if final_state == TaskControlState::Paused {
        println!("[Downloader] Task paused.");
        let _ = window.emit(
            "download-progress",
            ProgressPayload {
                url: url.clone(),
                title: page_info.title,
                index: completed_count.load(Ordering::Relaxed),
                total: total_segments,
                speed_kbps: 0.0,
                status: "paused".to_string(),
            },
        );
        return;
    }

    // Some segments could not be downloaded even after retries. Fail the task
    // but KEEP the temp directory so a later resume only fetches the missing
    // segments instead of starting over.
    if !failed_indices.is_empty() {
        let msg = format!(
            "{} 个分片下载失败（已重试 3 次），可点击「继续」断点续传",
            failed_indices.len()
        );
        println!(
            "[Downloader] Segments still missing after retries: {:?}",
            failed_indices
        );
        let _ = std::fs::remove_file(&final_mp4_path);
        let _ = std::fs::remove_file(&final_ts_path);
        emit_fail(&msg);
        return;
    }

    // Step 6: Merging segments (Only if still running)
    let _ = window.emit(
        "download-progress",
        ProgressPayload {
            url: url.clone(),
            title: page_info.title.clone(),
            index: total_segments,
            total: total_segments,
            speed_kbps: 0.0,
            status: "merging".to_string(),
        },
    );

    // fMP4 streams (declared via #EXT-X-MAP) cannot be merged with ffmpeg's
    // concat demuxer: the init segment must be prepended and the moof/mdat
    // fragments concatenated in order. Everything else keeps the TS path
    // (ffmpeg concat demuxer, falling back to a plain binary merge).
    let is_fmp4 = m3u8_info.init_segment.is_some();

    if is_fmp4 {
        // 1) Binary-concatenate init + fragments → a valid fragmented MP4.
        let raw_fmp4_path = temp_dir_path.join("raw_fragmented.mp4");
        let concat_res = (|| -> Result<(), std::io::Error> {
            let mut writer = std::io::BufWriter::new(std::fs::File::create(&raw_fmp4_path)?);
            if init_path.exists() {
                let mut init_file = std::fs::File::open(&init_path)?;
                std::io::copy(&mut init_file, &mut writer)?;
            } else {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "init segment missing",
                ));
            }
            for i in 0..total_segments {
                let segment_path = temp_dir_path.join(format!("{}.ts", i));
                let mut file = std::fs::File::open(segment_path)?;
                std::io::copy(&mut file, &mut writer)?;
            }
            Ok(())
        })();

        match concat_res {
            Ok(()) => {
                // 2) Remux to a clean (non-fragmented) MP4 when ffmpeg exists;
                //    otherwise keep the fragmented MP4 — it still plays.
                let remux_ok = std::process::Command::new("ffmpeg")
                    .arg("-y")
                    .arg("-i")
                    .arg(&raw_fmp4_path)
                    .arg("-c")
                    .arg("copy")
                    .arg("-movflags")
                    .arg("+faststart")
                    .arg(&final_mp4_path)
                    .output()
                    .map(|r| r.status.success())
                    .unwrap_or(false);
                if remux_ok {
                    println!("[Downloader] fMP4 remuxed: {:?}", final_mp4_path);
                } else {
                    let _ = std::fs::copy(&raw_fmp4_path, &final_mp4_path);
                    println!(
                        "[Downloader] ffmpeg remux unavailable, kept fragmented MP4: {:?}",
                        final_mp4_path
                    );
                }
                let _ = std::fs::remove_file(&raw_fmp4_path);
            }
            Err(e) => {
                // Keep the temp directory so a resume can retry the merge.
                let _ = std::fs::remove_file(&final_mp4_path);
                let _ = std::fs::remove_file(&final_ts_path);
                emit_fail(&format!("fMP4 merge failed: {}", e));
                return;
            }
        }
    } else {
        // TS / unknown container: ffmpeg concat demuxer first, then a plain
        // binary concatenation of the packets.
        let mut ffmpeg_success = false;
        let concat_file_path = temp_dir_path.join("concat.txt");

        let mut concat_content = String::new();
        for i in 0..total_segments {
            concat_content.push_str(&format!("file '{}.ts'\n", i));
        }
        if std::fs::write(&concat_file_path, concat_content).is_ok() {
            let output = std::process::Command::new("ffmpeg")
                .arg("-y")
                .arg("-f")
                .arg("concat")
                .arg("-safe")
                .arg("0")
                .arg("-i")
                .arg(&concat_file_path)
                .arg("-c")
                .arg("copy")
                .arg("-movflags")
                .arg("+faststart")
                .arg("-avoid_negative_ts")
                .arg("make_zero")
                .arg(&final_mp4_path)
                .output();

            if let Ok(res) = output {
                if res.status.success() {
                    ffmpeg_success = true;
                    println!(
                        "[Downloader] FFmpeg merged successfully: {:?}",
                        final_mp4_path
                    );
                }
            }
        }

        if !ffmpeg_success {
            println!("[Downloader] Falling back to binary merge...");
            let merge_res = (|| -> Result<(), std::io::Error> {
                let final_file = std::fs::File::create(&final_ts_path)?;
                let mut writer = std::io::BufWriter::new(final_file);
                for i in 0..total_segments {
                    let segment_path = temp_dir_path.join(format!("{}.ts", i));
                    let mut file = std::fs::File::open(segment_path)?;
                    std::io::copy(&mut file, &mut writer)?;
                }
                Ok(())
            })();

            if let Err(e) = merge_res {
                // Keep the temp directory (all segments are present) so the user
                // can retry the merge without re-downloading; only drop the
                // partial output file.
                let _ = std::fs::remove_file(&final_mp4_path);
                let _ = std::fs::remove_file(&final_ts_path);
                emit_fail(&format!("Binary merge failed: {}", e));
                return;
            }
            println!("[Downloader] Binary merge completed: {:?}", final_ts_path);
        }
    }

    // The user may have cancelled while ffmpeg was running — honour it.
    let post_merge_state = {
        let states = task_states.lock().unwrap();
        states
            .get(&url)
            .map(|info| info.state)
            .unwrap_or(TaskControlState::Running)
    };
    if post_merge_state == TaskControlState::Cancelled {
        println!("[Downloader] Task cancelled after merge. Cleaning up.");
        let _ = std::fs::remove_file(&final_mp4_path);
        let _ = std::fs::remove_file(&final_ts_path);
        let _ = std::fs::remove_dir_all(&temp_dir_path);
        let mut states = task_states.lock().unwrap();
        states.remove(&url);
        return;
    }
    if post_merge_state == TaskControlState::Paused {
        // Rare: paused exactly at merge time. The output is already fully
        // merged, so report it as completed rather than leaving a half-state.
        println!("[Downloader] Task paused after merge; treating as completed.");
        let _ = std::fs::remove_dir_all(&temp_dir_path);
        let mut states = task_states.lock().unwrap();
        states.remove(&url);
        drop(states);
        let _ = window.emit(
            "download-progress",
            ProgressPayload {
                url: url.clone(),
                title: page_info.title,
                index: total_segments,
                total: total_segments,
                speed_kbps: 0.0,
                status: "completed".to_string(),
            },
        );
        return;
    }

    // Clean up temporary files
    let _ = std::fs::remove_dir_all(&temp_dir_path);

    // Remove from registry
    {
        let mut states = task_states.lock().unwrap();
        states.remove(&url);
    }

    // Completed
    let _ = window.emit(
        "download-progress",
        ProgressPayload {
            url: url.clone(),
            title: page_info.title,
            index: total_segments,
            total: total_segments,
            speed_kbps: 0.0,
            status: "completed".to_string(),
        },
    );
}

/// Download a single M3U8 segment (with decryption), honouring pause/cancel and
/// breakpoint-resume. Returns the number of bytes fetched, or 0 when the
/// segment was already present on disk.
///
/// `bytes_counter` is incremented *as chunks arrive from the network* (not at
/// segment completion), so the periodic progress reporter can show a live
/// speed that matches the actual traffic rate.
#[allow(clippy::too_many_arguments)]
async fn download_segment(
    client: Client,
    index: usize,
    segment_url: String,
    byte_range: Option<(u64, u64)>,
    temp_path: PathBuf,
    key_bytes: Option<Vec<u8>>,
    iv: Option<Vec<u8>>,
    site: Site,
    task_states: TaskRegistry,
    url: String,
    cf_clearance: String,
    user_agent: String,
    bytes_counter: Arc<AtomicUsize>,
) -> Result<usize, String> {
    // Honour pause/cancel before starting.
    {
        let states = task_states.lock().unwrap();
        if let Some(info) = states.get(&url) {
            if info.state == TaskControlState::Paused || info.state == TaskControlState::Cancelled {
                return Err("paused or cancelled".to_string());
            }
        }
    }

    // Breakpoint resume: skip existing non-empty segment files.
    let file_path = temp_path.join(format!("{}.ts", index));
    if file_path.exists() && file_path.metadata().map(|m| m.len()).unwrap_or(0) > 0 {
        return Ok(0);
    }

    let mut req = client.get(&segment_url);
    req = apply_referer(req, &url);
    req = apply_cf_headers_safe(req, &segment_url, &url, &cf_clearance, &user_agent);
    // #EXT-X-BYTERANGE segments: fetch only the requested byte range.
    if let Some((start, len)) = byte_range {
        let end = start + len.saturating_sub(1);
        req = req.header("range", format!("bytes={}-{}", start, end));
    }
    let resp = req
        .send()
        .await
        .map_err(|e| format!("request failed: {}", e))?;

    // Stream the body and count bytes as they arrive so the live speed meter
    // reflects real traffic instead of only jumping at segment completion.
    use futures_util::StreamExt;
    let mut data = Vec::new();
    {
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| format!("read body failed: {}", e))?;
            bytes_counter.fetch_add(chunk.len(), Ordering::Relaxed);
            data.extend_from_slice(&chunk);
        }
    }

    if let Some(ref key) = key_bytes {
        let iv_bytes = get_iv_for_segment(index, &iv);

        use aes::cipher::{block_padding::NoPadding, BlockDecryptMut, KeyIvInit};
        type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;

        let dec = Aes128CbcDec::new_from_slices(key, &iv_bytes).map_err(|e| e.to_string())?;

        // Decrypt in-place using NoPadding since TS streams are padded block-aligned
        let decrypted_data = dec
            .decrypt_padded_mut::<NoPadding>(&mut data)
            .map_err(|e| format!("decryption error: {}", e))?;

        let final_data = match site {
            Site::Supjav => strip_fake_header(decrypted_data),
            _ => decrypted_data,
        };

        std::fs::write(&file_path, final_data).map_err(|e| format!("write failed: {}", e))?;
    } else {
        let final_data = match site {
            Site::Supjav => strip_fake_header(&data),
            _ => &data,
        };
        std::fs::write(&file_path, final_data).map_err(|e| format!("write failed: {}", e))?;
    }

    Ok(data.len())
}

/// Instantaneous download speed (KB/s) over a rolling window, shared by every
/// progress emitter so the displayed speed reflects the last ~1s instead of
/// a monotonic task-average.
fn instant_speed(tracker: &Mutex<Option<(Instant, usize)>>, now: Instant, bytes: usize) -> f64 {
    let mut guard = tracker.lock().unwrap();
    match guard.take() {
        Some((last_time, last_bytes)) => {
            let dt = now.duration_since(last_time).as_secs_f64();
            let db = bytes.saturating_sub(last_bytes);
            *guard = Some((now, bytes));
            if dt > 0.0 {
                (db as f64) / 1024.0 / dt
            } else {
                0.0
            }
        }
        None => {
            *guard = Some((now, bytes));
            0.0
        }
    }
}

/// Append an actionable hint when an error smells like a Cloudflare block, so
/// users are not left with a "mysterious" failure.
fn cloudflare_hint(err: &str) -> String {
    let lower = err.to_lowercase();
    if lower.contains("403")
        || lower.contains("forbidden")
        || lower.contains("cloudflare")
        || lower.contains("cf-challenge")
    {
        format!(
            "{}（可能是 Cloudflare 验证已过期/被拦截，请在浏览页重新点击「验证」后重试）",
            err
        )
    } else {
        err.to_string()
    }
}
