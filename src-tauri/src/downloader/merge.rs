use std::path::{Path, PathBuf};
use std::time::Duration;
use wreq::Client;

use super::task::{TaskControlState, TaskRegistry};
use super::utils::{apply_cf_headers_safe, apply_referer, emit_failure, emit_progress, SpeedTracker};

/// Download a direct MP4 stream (Streamtape / non-M3U8 sources) to
/// `{save_dir}/{safe_title}.mp4`. All progress/failure events are emitted
/// here; nothing is left for the caller to do afterwards.
pub async fn download_direct_mp4(
    client: &Client,
    window: &tauri::Window,
    task_states: &TaskRegistry,
    url: &str,
    title: &str,
    m3u8_url: &str,
    save_dir: &str,
    safe_title: &str,
    cf_clearance: &str,
    user_agent: &str,
) {
    let final_mp4_path = PathBuf::from(save_dir).join(format!("{}.mp4", safe_title));

    // Already downloaded?
    if final_mp4_path.exists() {
        println!(
            "[Downloader] Direct MP4 file already exists: {:?}",
            final_mp4_path
        );
        task_states.lock().unwrap().remove(url);
        emit_progress(window, url, title, 100, 100, 0.0, "completed");
        return;
    }

    let mut req = client.get(m3u8_url);
    if m3u8_url.contains("streamtape") {
        req = req.header("referer", "https://streamtape.com/");
        if !user_agent.trim().is_empty() {
            req = req.header("user-agent", user_agent.trim());
        }
    } else {
        req = apply_referer(req, url);
        req = apply_cf_headers_safe(req, m3u8_url, url, cf_clearance, user_agent);
    }

    let resp = match req.send().await {
        Ok(resp) => resp,
        Err(e) => {
            emit_failure(window, task_states, url, &format!("Failed to request direct MP4 stream: {}", e));
            return;
        }
    };
    if !resp.status().is_success() {
        emit_failure(
            window,
            task_states,
            url,
            &format!("Direct MP4 download failed: HTTP {}", resp.status()),
        );
        return;
    }

    let total_bytes = resp.content_length().unwrap_or(0);
    println!("[Downloader] Direct MP4 file size: {} bytes", total_bytes);

    let mut file = match std::fs::File::create(&final_mp4_path) {
        Ok(f) => f,
        Err(e) => {
            emit_failure(window, task_states, url, &format!("Failed to create output file: {}", e));
            return;
        }
    };

    use futures_util::StreamExt;
    let mut stream_pin = Box::pin(resp.bytes_stream());

    let mut downloaded = 0;
    let mut last_emit = std::time::Instant::now();
    let mut speed_tracker = SpeedTracker::new(Duration::from_secs(3));

    while let Some(chunk_result) = stream_pin.next().await {
        // Honour pause/cancel while streaming.
        {
            let states = task_states.lock().unwrap();
            if let Some(info) = states.get(url) {
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
                    emit_failure(window, task_states, url, &format!("Failed to write to file: {}", e));
                    return;
                }
                downloaded += chunk.len();

                // Emit progress every 1000ms with windowed speed.
                if last_emit.elapsed().as_millis() > 1000 {
                    let now = std::time::Instant::now();
                    last_emit = now;
                    let progress_percent = if total_bytes > 0 {
                        (downloaded as f64 / total_bytes as f64 * 100.0) as usize
                    } else {
                        50
                    };
                    let current_speed = speed_tracker.sample(now, downloaded);
                    emit_progress(
                        window,
                        url,
                        title,
                        progress_percent,
                        100,
                        current_speed,
                        "downloading",
                    );
                }
            }
            Err(e) => {
                emit_failure(window, task_states, url, &format!("Error downloading chunk: {}", e));
                return;
            }
        }
    }

    task_states.lock().unwrap().remove(url);
    emit_progress(window, url, title, 100, 100, 0.0, "completed");
}

/// Merge downloaded segments into the final output file.
///
/// fMP4 streams (declared via #EXT-X-MAP) cannot be merged with ffmpeg's
/// concat demuxer: the init segment must be prepended and the moof/mdat
/// fragments concatenated in order. Everything else keeps the TS path
/// (ffmpeg concat demuxer, falling back to a plain binary merge).
///
/// On failure the temp directory is KEPT (all segments are present) so a
/// resume can retry the merge without re-downloading; only partial output
/// files are dropped. Returns true on success.
pub fn merge_segments(
    window: &tauri::Window,
    task_states: &TaskRegistry,
    url: &str,
    title: &str,
    temp_dir_path: &Path,
    init_path: &Path,
    total_segments: usize,
    final_mp4_path: &Path,
    final_ts_path: &Path,
    is_fmp4: bool,
) -> bool {
    emit_progress(window, url, title, total_segments, total_segments, 0.0, "merging");

    if is_fmp4 {
        // 1) Binary-concatenate init + fragments → a valid fragmented MP4.
        let raw_fmp4_path = temp_dir_path.join("raw_fragmented.mp4");
        let concat_res = (|| -> Result<(), std::io::Error> {
            let mut writer = std::io::BufWriter::new(std::fs::File::create(&raw_fmp4_path)?);
            if init_path.exists() {
                let mut init_file = std::fs::File::open(init_path)?;
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
                    .arg(final_mp4_path)
                    .output()
                    .map(|r| r.status.success())
                    .unwrap_or(false);
                if remux_ok {
                    println!("[Downloader] fMP4 remuxed: {:?}", final_mp4_path);
                } else {
                    let _ = std::fs::copy(&raw_fmp4_path, final_mp4_path);
                    println!(
                        "[Downloader] ffmpeg remux unavailable, kept fragmented MP4: {:?}",
                        final_mp4_path
                    );
                }
                let _ = std::fs::remove_file(&raw_fmp4_path);
                true
            }
            Err(e) => {
                let _ = std::fs::remove_file(final_mp4_path);
                let _ = std::fs::remove_file(final_ts_path);
                emit_failure(window, task_states, url, &format!("fMP4 merge failed: {}", e));
                false
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
                .arg(final_mp4_path)
                .output();

            if let Ok(res) = output {
                if res.status.success() {
                    ffmpeg_success = true;
                    println!("[Downloader] FFmpeg merged successfully: {:?}", final_mp4_path);
                }
            }
        }

        if !ffmpeg_success {
            println!("[Downloader] Falling back to binary merge...");
            let merge_res = (|| -> Result<(), std::io::Error> {
                let final_file = std::fs::File::create(final_ts_path)?;
                let mut writer = std::io::BufWriter::new(final_file);
                for i in 0..total_segments {
                    let segment_path = temp_dir_path.join(format!("{}.ts", i));
                    let mut file = std::fs::File::open(segment_path)?;
                    std::io::copy(&mut file, &mut writer)?;
                }
                Ok(())
            })();

            if let Err(e) = merge_res {
                // Keep the temp directory (all segments are present) so the
                // user can retry the merge without re-downloading; only drop
                // the partial output file.
                let _ = std::fs::remove_file(final_mp4_path);
                let _ = std::fs::remove_file(final_ts_path);
                emit_failure(window, task_states, url, &format!("Binary merge failed: {}", e));
                return false;
            }
            println!("[Downloader] Binary merge completed: {:?}", final_ts_path);
        }
        true
    }
}
