use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use wreq::Client;

use crate::scraper::Site;

use super::merge::{download_direct_mp4, merge_segments};
use super::m3u8::{parse_master_or_media_m3u8, M3u8Info, Segment};
use super::page::fetch_page_info;
use super::task::{TaskControlState, TaskRegistry};
use super::utils::{
    apply_cf_headers_safe, apply_referer, emit_failure, emit_progress, get_iv_for_segment,
    sanitize_filename, strip_fake_header, SpeedTracker,
};

/// Result of the parallel segment-download phase.
enum SegmentsOutcome {
    Completed,
    Cancelled,
    Paused,
    /// A newer instance took over this URL (pause → resume race); the current
    /// task must back off without touching files or emitting events.
    Superseded,
    /// Some segments could not be downloaded even after retries.
    Failed(Vec<usize>),
}

/// Filesystem layout prepared for a download (Step 4).
struct Workspace {
    final_mp4_path: PathBuf,
    final_ts_path: PathBuf,
    temp_dir_path: PathBuf,
    init_path: PathBuf,
    total_segments: usize,
}

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
    println!("[Downloader] Starting/Resuming download for: {}", url);

    // This task instance's generation. If a newer instance for the same URL
    // takes over (e.g. pause → resume while this one is still winding down),
    // we must back off: no more events, no touching output files.
    let my_generation = {
        let states = task_states.lock().unwrap();
        states.get(&url).map(|i| i.generation).unwrap_or(0)
    };
    let is_current = |task_states: &TaskRegistry| -> bool {
        task_states
            .lock()
            .unwrap()
            .get(&url)
            .map(|i| i.generation == my_generation)
            .unwrap_or(false)
    };

    // Step 1: Parse page HTML
    let page_info = match fetch_page_info(&client, site, &url, &cf_clearance, &user_agent).await {
        Ok(info) => info,
        Err(msg) => {
            emit_failure(&window, &task_states, &url, &msg);
            return;
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

    // Direct MP4 (Streamtape etc.) — no M3U8 involved.
    let is_direct_mp4 =
        page_info.m3u8_url.contains(".mp4") || !page_info.m3u8_url.contains(".m3u8");
    if is_direct_mp4 {
        println!(
            "[Downloader] Direct MP4 stream detected. Downloading file directly: {}",
            page_info.m3u8_url
        );
        download_direct_mp4(
            &client,
            &window,
            &task_states,
            &url,
            &page_info.title,
            &page_info.m3u8_url,
            &save_dir,
            &safe_title,
            &cf_clearance,
            &user_agent,
        )
        .await;
        return;
    }

    // Step 2: Parse Master or Media M3U8 based on resolution preference
    let m3u8_info =
        match parse_master_or_media_m3u8(
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
                emit_failure(&window, &task_states, &url, &format!("Failed to parse M3U8 playlist: {}", e));
                return;
            }
        };

    // Step 3: Fetch Encryption Key if needed
    let key_bytes = match fetch_decryption_key(
        &client,
        &window,
        &task_states,
        &url,
        m3u8_info.key_url.as_deref(),
        &cf_clearance,
        &user_agent,
    )
    .await
    {
        Some(k) => k,
        None => return,
    };

    // Step 4: Prepare directories, metadata and the fMP4 init segment
    let ws = match prepare_workspace(
        &client,
        &window,
        &task_states,
        &url,
        site,
        &save_dir,
        &safe_title,
        &page_info.title,
        &page_info.m3u8_url,
        &m3u8_info,
        Some(key_bytes.as_slice()),
        max_concurrent,
        &resolution_pref,
        &cf_clearance,
        &user_agent,
    )
    .await
    {
        Some(ws) => ws,
        None => return,
    };

    // Step 5: Concurrently download all segments (with retries)
    let outcome = download_all_segments(
        client,
        window.clone(),
        task_states.clone(),
        &m3u8_info.segments,
        ws.temp_dir_path.clone(),
        Some(key_bytes),
        m3u8_info.iv.clone(),
        site,
        url.clone(),
        page_info.title.clone(),
        cf_clearance,
        user_agent,
        max_concurrent,
        my_generation,
    )
    .await;

    match outcome {
        SegmentsOutcome::Completed => {}
        SegmentsOutcome::Cancelled | SegmentsOutcome::Paused | SegmentsOutcome::Superseded => return,
        SegmentsOutcome::Failed(indices) => {
            // If a newer instance took over while we were finishing, back off
            // silently — it owns the files now.
            if !is_current(&task_states) {
                println!("[Downloader] Superseded by a newer instance; backing off for {}", url);
                return;
            }
            // Fail the task but KEEP the temp directory so a later resume
            // only fetches the missing segments instead of starting over.
            let msg = format!(
                "{} 个分片下载失败（已重试 3 次），可点击「继续」断点续传",
                indices.len()
            );
            println!(
                "[Downloader] Segments still missing after retries: {:?}",
                indices
            );
            let _ = std::fs::remove_file(&ws.final_mp4_path);
            let _ = std::fs::remove_file(&ws.final_ts_path);
            emit_failure(&window, &task_states, &url, &msg);
            return;
        }
    }

    // Step 6: Merge segments
    let is_fmp4 = m3u8_info.init_segment.is_some();
    if !merge_segments(
        &window,
        &task_states,
        &url,
        &page_info.title,
        &ws.temp_dir_path,
        &ws.init_path,
        ws.total_segments,
        &ws.final_mp4_path,
        &ws.final_ts_path,
        is_fmp4,
    ) {
        return;
    }

    // The user may have cancelled/paused while ffmpeg was running — honour it.
    let post_merge_state = {
        let states = task_states.lock().unwrap();
        states
            .get(&url)
            .map(|info| info.state)
            .unwrap_or(TaskControlState::Running)
    };
    if post_merge_state == TaskControlState::Cancelled {
        // Never clean up files that a newer instance may now own.
        if !is_current(&task_states) {
            return;
        }
        println!("[Downloader] Task cancelled after merge. Cleaning up.");
        let _ = std::fs::remove_file(&ws.final_mp4_path);
        let _ = std::fs::remove_file(&ws.final_ts_path);
        let _ = std::fs::remove_dir_all(&ws.temp_dir_path);
        task_states.lock().unwrap().remove(&url);
        return;
    }
    if post_merge_state == TaskControlState::Paused {
        // Rare: paused exactly at merge time. The output is already fully
        // merged, so report it as completed rather than leaving a half-state.
        println!("[Downloader] Task paused after merge; treating as completed.");
        let _ = std::fs::remove_dir_all(&ws.temp_dir_path);
        task_states.lock().unwrap().remove(&url);
        emit_progress(
            &window,
            &url,
            &page_info.title,
            ws.total_segments,
            ws.total_segments,
            0.0,
            "completed",
        );
        return;
    }

    // Clean up temporary files
    let _ = std::fs::remove_dir_all(&ws.temp_dir_path);
    task_states.lock().unwrap().remove(&url);
    emit_progress(
        &window,
        &url,
        &page_info.title,
        ws.total_segments,
        ws.total_segments,
        0.0,
        "completed",
    );
}

/// Download the AES-128 decryption key, if the playlist declares one.
/// Returns None after emitting a failure event.
async fn fetch_decryption_key(
    client: &Client,
    window: &tauri::Window,
    task_states: &TaskRegistry,
    url: &str,
    k_url: Option<&str>,
    cf_clearance: &str,
    user_agent: &str,
) -> Option<Vec<u8>> {
    let k_url = k_url?;
    println!("[Downloader] Stream is encrypted. Fetching key from: {}", k_url);
    let mut req = client.get(k_url);
    req = apply_referer(req, url);
    req = apply_cf_headers_safe(req, k_url, url, cf_clearance, user_agent);
    match req.send().await {
        Ok(resp) => match resp.bytes().await {
            Ok(bytes) => Some(bytes.to_vec()),
            Err(e) => {
                emit_failure(window, task_states, url, &format!("Failed to download decryption key: {}", e));
                None
            }
        },
        Err(e) => {
            emit_failure(window, task_states, url, &format!("Failed to request decryption key: {}", e));
            None
        }
    }
}

/// Prepare the download workspace: check for an existing output, create the
/// temp directory, write crash-recovery metadata and fetch the fMP4 init
/// segment. Returns None after emitting a failure event.
async fn prepare_workspace(
    client: &Client,
    window: &tauri::Window,
    task_states: &TaskRegistry,
    url: &str,
    site: Site,
    save_dir: &str,
    safe_title: &str,
    title: &str,
    m3u8_url: &str,
    m3u8_info: &M3u8Info,
    key_bytes: Option<&[u8]>,
    max_concurrent: usize,
    resolution_pref: &str,
    cf_clearance: &str,
    user_agent: &str,
) -> Option<Workspace> {
    let base_save_path = PathBuf::from(save_dir);
    let final_mp4_path = base_save_path.join(format!("{}.mp4", safe_title));
    let final_ts_path = base_save_path.join(format!("{}.ts", safe_title));
    let total_segments = m3u8_info.segments.len();

    // Non-empty final file only — a truncated leftover from a crashed merge
    // must not count as completed.
    let valid_final_exists = (final_mp4_path.exists()
        && final_mp4_path.metadata().map(|m| m.len()).unwrap_or(0) > 0)
        || (final_ts_path.exists() && final_ts_path.metadata().map(|m| m.len()).unwrap_or(0) > 0);
    if valid_final_exists {
        println!(
            "[Downloader] Target merged video file already exists: {:?}",
            final_mp4_path
        );
        let _ = std::fs::remove_dir_all(base_save_path.join(format!("temp_{}", safe_title)));
        task_states.lock().unwrap().remove(url);
        emit_progress(window, url, title, total_segments, total_segments, 0.0, "completed");
        return None;
    }
    // A partial/empty final file from an earlier failed merge: remove it so it
    // cannot shadow a future download or get appended to.
    let _ = std::fs::remove_file(&final_mp4_path);
    let _ = std::fs::remove_file(&final_ts_path);

    if base_save_path.is_file() {
        emit_failure(window, task_states, url, "Specified save path is a file, not a directory. Please select a valid download folder in Settings.");
        return None;
    }
    if !base_save_path.exists() {
        if let Err(e) = std::fs::create_dir_all(&base_save_path) {
            emit_failure(
                window,
                task_states,
                url,
                &format!("Failed to create save directory: {}. Please verify folder write permissions or select a valid path in Settings.", e),
            );
            return None;
        }
    }

    // Code-based folder lookup to match existing folders even if titles differ
    let mut resolved_temp_path = None;
    let re_code = regex::Regex::new(r"(?i)([a-z0-9]{2,10}-\d{3,6})").unwrap();
    if let Some(caps) = re_code.captures(url) {
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
        emit_failure(window, task_states, url, "Temporary folder path conflicts with an existing file. Please remove the conflicting file or select another directory.");
        return None;
    }
    if !temp_dir_path.exists() {
        if let Err(e) = std::fs::create_dir_all(&temp_dir_path) {
            emit_failure(
                window,
                task_states,
                url,
                &format!("Failed to create temporary folder: {}. Please verify folder write permissions or select a valid path in Settings.", e),
            );
            return None;
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
        url: url.to_string(),
        title: title.to_string(),
        save_dir: save_dir.to_string(),
        max_concurrent,
        resolution: resolution_pref.to_string(),
        total_segments,
        m3u8_url: m3u8_url.to_string(),
    };
    if let Ok(meta_json) = serde_json::to_string(&meta) {
        let _ = std::fs::write(temp_dir_path.join("task_metadata.json"), meta_json);
    }

    // fMP4 init segment (#EXT-X-MAP): must be fetched before the media
    // segments and prepended when merging, otherwise the concatenated
    // fragments are not playable. Ignored for TS playlists.
    let init_path = temp_dir_path.join("init.mp4");
    if let Some(init) = &m3u8_info.init_segment {
        println!("[Downloader] Downloading fMP4 init segment: {}", init.url);
        let mut req = client.get(&init.url);
        req = apply_referer(req, url);
        req = apply_cf_headers_safe(req, &init.url, url, cf_clearance, user_agent);
        if let Some((start, len)) = init.byte_range {
            let end = start + len.saturating_sub(1);
            req = req.header("range", format!("bytes={}-{}", start, end));
        }
        let mut init_data = match req.send().await {
            Ok(resp) => {
                if !resp.status().is_success() {
                    emit_failure(
                        window,
                        task_states,
                        url,
                        &format!("Failed to download init segment: HTTP {}", resp.status()),
                    );
                    return None;
                }
                match resp.bytes().await {
                    Ok(bytes) => bytes.to_vec(),
                    Err(e) => {
                        emit_failure(window, task_states, url, &format!("Failed to read init segment: {}", e));
                        return None;
                    }
                }
            }
            Err(e) => {
                emit_failure(window, task_states, url, &format!("Failed to request init segment: {}", e));
                return None;
            }
        };
        // If the stream is AES-128 encrypted, the init segment is encrypted
        // too (same key, IV of the first media segment).
        if let Some(key) = key_bytes {
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
            emit_failure(window, task_states, url, &format!("Failed to write init segment: {}", e));
            return None;
        }
    }

    Some(Workspace {
        final_mp4_path,
        final_ts_path,
        temp_dir_path,
        init_path,
        total_segments,
    })
}

/// Concurrently download every segment (with retries) while a periodic
/// reporter keeps the UI speed alive. Returns the outcome for the caller.
async fn download_all_segments(
    client: Client,
    window: tauri::Window,
    task_states: TaskRegistry,
    segments: &[Segment],
    temp_dir_path: PathBuf,
    key_bytes: Option<Vec<u8>>,
    iv: Option<Vec<u8>>,
    site: Site,
    url: String,
    title: String,
    cf_clearance: String,
    user_agent: String,
    max_concurrent: usize,
    my_generation: u64,
) -> SegmentsOutcome {
    let total_segments = segments.len();
    let semaphore = Arc::new(Semaphore::new(max_concurrent.clamp(1, 16)));
    let completed_count = Arc::new(AtomicUsize::new(0));
    let bytes_count = Arc::new(AtomicUsize::new(0));
    let speed_tracker: Arc<Mutex<SpeedTracker>> =
        Arc::new(Mutex::new(SpeedTracker::new(Duration::from_secs(3))));
    let segments_done = Arc::new(AtomicBool::new(false));

    let key_bytes_arc = Arc::new(key_bytes);
    let custom_iv_arc = Arc::new(iv);

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
        let title_clone = title.clone();
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
                let speed = speed_tracker
                    .lock()
                    .unwrap()
                    .sample(Instant::now(), bytes.load(Ordering::Relaxed));
                emit_progress(
                    &window_clone,
                    &url_clone,
                    &title_clone,
                    completed.load(Ordering::Relaxed),
                    total_segments,
                    speed,
                    "downloading",
                );
            }
        });
    }

    let mut join_handles = Vec::new();

    for (index, segment) in segments.iter().enumerate() {
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
        let title_clone = title.clone();
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
                    let speed = speed_tracker
                        .lock()
                        .unwrap()
                        .sample(Instant::now(), total_bytes);
                    emit_progress(
                        &window_clone,
                        &url_clone,
                        &title_clone,
                        done,
                        total_segments,
                        speed,
                        "downloading",
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
                segments[index].url.clone(),
                segments[index].byte_range,
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
                    let speed = speed_tracker
                        .lock()
                        .unwrap()
                        .sample(Instant::now(), total_bytes);
                    emit_progress(
                        &window,
                        &url,
                        &title,
                        done,
                        total_segments,
                        speed,
                        "downloading",
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

    // A newer instance for this URL (pause → resume race) bumps the
    // generation; the old task must back off without emitting events or
    // deleting files that the new instance now owns.
    let superseded = {
        let states = task_states.lock().unwrap();
        states
            .get(&url)
            .map(|i| i.generation != my_generation)
            .unwrap_or(false)
    };
    if superseded {
        println!("[Downloader] Superseded by a newer instance; backing off for {}", url);
        return SegmentsOutcome::Superseded;
    }

    if final_state == TaskControlState::Cancelled {
        println!("[Downloader] Task cancelled. Deleting temp folder.");
        let _ = std::fs::remove_dir_all(&temp_dir_path);
        task_states.lock().unwrap().remove(&url);
        return SegmentsOutcome::Cancelled;
    }

    if final_state == TaskControlState::Paused {
        println!("[Downloader] Task paused.");
        emit_progress(
            &window,
            &url,
            &title,
            completed_count.load(Ordering::Relaxed),
            total_segments,
            0.0,
            "paused",
        );
        return SegmentsOutcome::Paused;
    }

    if !failed_indices.is_empty() {
        return SegmentsOutcome::Failed(failed_indices);
    }

    SegmentsOutcome::Completed
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
            if info.state == TaskControlState::Paused
                || info.state == TaskControlState::Cancelled
            {
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
