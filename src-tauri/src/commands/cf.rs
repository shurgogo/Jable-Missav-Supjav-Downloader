use tauri::Emitter;
use tauri::Manager;
use tauri::State;

use super::{AppState, CfConfig};

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
    let window_label = format!("cf_verifier_{}", domain.replace('.', "_"));

    // 1. Close the existing old verification window for this specific domain
    if let Some(old_win) = app.get_webview_window(&window_label) {
        let _ = old_win.close();
    }

    // 2. Create the new verification window on the main thread
    let (tx, rx) = tokio::sync::oneshot::channel();
    let app_handle = app.clone();
    let webview_url = url.clone();
    let domain_title = domain.clone();
    let window_label_for_build = window_label.clone();

    app.run_on_main_thread(move || {
        let res = WebviewWindowBuilder::new(
            &app_handle,
            &window_label_for_build,
            WebviewUrl::External(webview_url),
        )
        .title(format!("防爬驗證 - {}", domain_title))
        .inner_size(680.0, 580.0)
        .resizable(true)
        .focused(true)
        .build();
        let _ = tx.send(res);
    })
    .map_err(|e| e.to_string())?;

    let verifier_window = rx
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;

    // 3. Recording the cf_clearance value before the verification starts, used to ignore this old value during subsequent polling
    let previous_cf_clearance = {
        let configs = state.cf_configs.lock().unwrap();
        configs
            .get(&domain)
            .map(|cfg| cfg.cf_clearance.clone())
            .unwrap_or_default()
    };

    // Attempt to purge any old cf_clearance cookie from webview session
    if let Ok(cookies) = verifier_window.cookies_for_url(url.clone()) {
        for c in cookies {
            if c.name() == "cf_clearance" {
                let _ = verifier_window.delete_cookie(c);
                println!("[Verifier] Purged initial cf_clearance cookie from webview session.");
            }
        }
    }

    let cf_configs_state = state.cf_configs.clone();
    let client_clone = state.client.clone();
    let app_handle = app.clone();
    let target_url = url.clone();
    let target_domain = domain.clone();
    let target_window_label = window_label.clone();
    let ua_for_polling = user_agent.clone();

    tokio::spawn(async move {
        println!(
            "[Verifier] Started background cookie polling for domain {} (label: {})",
            target_domain, target_window_label
        );
        // Cross-loop memory of the "most recently seen new cf_clearance", used for fallback storage when the window is manually closed
        let mut latest_seen_cf_clearance: Option<String> = None;
        let mut stable_cookie_candidate: Option<String> = None;
        let mut stable_counter: usize = 0;

        for _ in 0..240 {
            // Poll every 500ms, with a maximum of 120 seconds
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

            let current_window = match app_handle.get_webview_window(&target_window_label) {
                Some(w) => w,
                None => {
                    println!("[Verifier] Window closed by user for {}.", target_domain);
                    break;
                }
            };

            // Check the page title and URL to determine if the Cloudflare verification page is still in a challenge state
            let title = current_window.title().unwrap_or_default();
            let current_url = current_window
                .url()
                .map(|u| u.to_string())
                .unwrap_or_default();

            let is_still_challenge = title.contains("Just a moment")
                || title.contains("Attention Required")
                || title.contains("Checking your browser")
                || title.contains("Please wait")
                || title.contains("请稍候")
                || title.contains("验证中")
                || current_url.contains("cf_challenge");

            if let Ok(cookies) = current_window.cookies_for_url(target_url.clone()) {
                let fresh_cookie_value = cookies
                    .into_iter()
                    .find(|c| c.name() == "cf_clearance" && !c.value().trim().is_empty())
                    .map(|c| c.value().to_string());

                if let Some(fresh_cf_clearance) = fresh_cookie_value {
                    // If it is the same as the old value before verification started, it means it has not been refreshed, skip it
                    if fresh_cf_clearance == previous_cf_clearance {
                        continue;
                    }

                    latest_seen_cf_clearance = Some(fresh_cf_clearance.clone());

                    // 1. Debouncing mechanism: Detect if the Cookie is in a multi-stage change
                    if stable_cookie_candidate.as_ref() != Some(&fresh_cf_clearance) {
                        stable_cookie_candidate = Some(fresh_cf_clearance.clone());
                        stable_counter = 1;
                        println!(
                            "[Verifier] Detected new intermediate cf_clearance for {}. Waiting for stabilization...",
                            target_domain
                        );
                    } else {
                        stable_counter += 1;
                    }

                    // 2. Only when the challenge page disappears, and the Cookie is stable for 3 consecutive polls (1.5 seconds) without being replaced by Cloudflare, will the actual liveness test be initiated
                    if !is_still_challenge && stable_counter >= 3 {
                        println!(
                            "[Verifier] Cookie stabilized. Performing active HTTP probe for {}...",
                            target_domain
                        );

                        // 3. Backend actual measurement verification: Use wreq Client to send a lightweight probe request to the target URL with the controversial Cookie
                        let mut probe_req = client_clone
                            .get(target_url.as_str())
                            .header(
                                "accept",
                                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
                            )
                            .header(
                                "accept-language",
                                crate::scraper::get_accept_language_header(""),
                            );

                        probe_req = crate::scraper::apply_cf_headers(
                            probe_req,
                            &fresh_cf_clearance,
                            &ua_for_polling,
                        );

                        let is_authorized = match probe_req.send().await {
                            Ok(resp) => {
                                let status = resp.status();
                                println!("[Verifier] Active probe response status: {}", status);
                                status.is_success() || status.is_redirection()
                            }
                            Err(e) => {
                                println!("[Verifier] Active probe network error: {}", e);
                                false
                            }
                        };

                        if is_authorized {
                            println!(
                                "[Verifier] Cloudflare challenge PASSED and VERIFIED for {}! Final stable cf_clearance: {}",
                                target_domain, fresh_cf_clearance
                            );

                            // 保存到全局配置
                            {
                                let mut configs = cf_configs_state.lock().unwrap();
                                configs.insert(
                                    target_domain.clone(),
                                    CfConfig {
                                        cf_clearance: fresh_cf_clearance.clone(),
                                        user_agent: ua_for_polling.clone(),
                                    },
                                );
                            }

                            // 通知前端验证成功
                            #[derive(serde::Serialize, Clone)]
                            struct SuccessPayload {
                                domain: String,
                                cf_clearance: String,
                                user_agent: String,
                            }
                            let _ = app_handle.emit(
                                "cf-verification-success",
                                SuccessPayload {
                                    domain: target_domain.clone(),
                                    cf_clearance: fresh_cf_clearance,
                                    user_agent: ua_for_polling.clone(),
                                },
                            );

                            // 自动关闭当前域名的验证窗口
                            if let Some(w) = app_handle.get_webview_window(&target_window_label) {
                                let _ = w.close();
                            }
                            break;
                        } else {
                            println!(
                                "[Verifier] Cookie is not fully authorized yet by Cloudflare server (Probe returned 403). Retrying..."
                            );
                            // 归零重置计数，让 WebView 继续在后台接收后续可能签发的最终 Cookie
                            stable_counter = 0;
                        }
                    }
                }
            }
        }

        // 兜底逻辑：如果用户在拿到 cf_clearance 之后手动关闭了窗口
        if let Some(fresh_cf_clearance) = latest_seen_cf_clearance {
            let configs = cf_configs_state.lock().unwrap();
            let already_saved = configs.contains_key(&target_domain);
            drop(configs);

            if !already_saved {
                println!(
                    "[Verifier] Window closed with active cf_clearance: {}",
                    fresh_cf_clearance
                );
                {
                    let mut configs = cf_configs_state.lock().unwrap();
                    configs.insert(
                        target_domain.clone(),
                        CfConfig {
                            cf_clearance: fresh_cf_clearance.clone(),
                            user_agent: ua_for_polling.clone(),
                        },
                    );
                }
                #[derive(serde::Serialize, Clone)]
                struct SuccessPayload {
                    domain: String,
                    cf_clearance: String,
                    user_agent: String,
                }
                let _ = app_handle.emit(
                    "cf-verification-success",
                    SuccessPayload {
                        domain: target_domain.clone(),
                        cf_clearance: fresh_cf_clearance,
                        user_agent: ua_for_polling,
                    },
                );
            }
        }

        println!(
            "[Verifier] Ended background cookie polling for: {}",
            target_domain
        );
    });

    Ok(())
}
