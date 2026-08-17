use tauri::State;

use crate::commands::AppState;
use crate::proxy::{build_client, detect_system_proxy, parse_proxy_url, ProxyStatus};

/// Proxy settings submitted from the Settings page.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxySettings {
    /// "system" | "direct" | "custom"
    pub mode: String,
    pub custom_proxy: Option<String>,
}

/// Rebuild the shared HTTP client with the requested proxy mode and swap it
/// into the app state. In-flight downloads keep the client they started with
/// until they finish; every new request uses the new configuration.
#[tauri::command]
pub fn apply_proxy_settings(
    state: State<'_, AppState>,
    settings: ProxySettings,
) -> Result<ProxyStatus, String> {
    let mode = settings.mode.as_str();

    let (cfg, warning) = match mode {
        "direct" => (None, None),
        "custom" => {
            let raw = settings.custom_proxy.as_deref().unwrap_or("").trim();
            if raw.is_empty() {
                return Err("自定义代理地址不能为空".to_string());
            }
            let cfg = parse_proxy_url(raw, "http")
                .ok_or_else(|| format!("无法解析自定义代理地址: {}", raw))?;
            (Some(cfg), None)
        }
        "system" => {
            let d = detect_system_proxy();
            (d.proxy, d.warning.map(|w| w.to_string()))
        }
        other => return Err(format!("未知的代理模式: {}", other)),
    };

    let (client, build_warning) = build_client(cfg.as_ref());
    let warning = warning.or(build_warning);

    *state.client.lock().unwrap() = client;

    let status = ProxyStatus {
        mode: mode.to_string(),
        url: cfg.map(|c| c.url()),
        warning,
    };
    *state.proxy_status.lock().unwrap() = Some(status.clone());
    println!("[AVDL] Proxy settings applied: {:?}", status);

    Ok(status)
}

/// Current effective proxy status (what was applied last).
#[tauri::command]
pub fn get_proxy_status(state: State<'_, AppState>) -> ProxyStatus {
    state
        .proxy_status
        .lock()
        .unwrap()
        .clone()
        .unwrap_or(ProxyStatus {
            mode: "system".to_string(),
            url: None,
            warning: None,
        })
}
