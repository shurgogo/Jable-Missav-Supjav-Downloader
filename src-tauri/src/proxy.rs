//! Proxy configuration: system-proxy detection, custom proxy parsing and
//! construction of the shared wreq client.
//!
//! The user can pick one of three modes in Settings:
//! - `system` (default): auto-detect the OS proxy and use it when present,
//!   otherwise fall back to a direct connection.
//! - `direct`: never use a proxy.
//! - `custom`: use a user-supplied `http(s)://` or `socks5://` URL.
//!
//! Detection is best-effort and platform-specific:
//! - macOS: reads the effective proxy from `scutil --proxy` (no network
//!   service guessing, unlike `networksetup`-based helpers).
//! - Windows: reads the WinINET registry settings with tolerant parsing of
//!   the `ProxyServer` value (bare `ip:port`, `http=..;https=..` lists, ...).
//! - Linux: environment variables, then gsettings via the `sysproxy` crate.
//! - All platforms: `HTTP(S)_PROXY` / `ALL_PROXY` environment variables as a
//!   final fallback.

use std::process::Command;

use sysproxy::Sysproxy;
use wreq_util::Emulation;

/// A parsed proxy endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyConfig {
    pub scheme: String,
    pub host: String,
    pub port: u16,
}

impl ProxyConfig {
    pub fn url(&self) -> String {
        format!("{}://{}:{}", self.scheme, self.host, self.port)
    }
}

/// Result of a system-proxy detection run.
#[derive(Debug, Clone, Default)]
pub struct Detection {
    pub proxy: Option<ProxyConfig>,
    /// Warning code for the UI ("pac_unsupported" | "parse_failed"), or None.
    pub warning: Option<&'static str>,
}

/// Serializable status reported to the frontend.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyStatus {
    /// "system" | "direct" | "custom"
    pub mode: String,
    /// Effective proxy URL, or null when direct.
    pub url: Option<String>,
    /// Warning code ("pac_unsupported" | "parse_failed" | "socks_unsupported"),
    /// or null.
    pub warning: Option<String>,
}

/// Build the shared wreq client. `proxy = None` means a direct connection.
/// Returns the client plus an optional warning code when the proxy could not
/// be applied (e.g. a SOCKS URL on a build without socks support).
pub fn build_client(proxy: Option<&ProxyConfig>) -> (wreq::Client, Option<String>) {
    let mut builder = wreq::Client::builder()
        .emulation(Emulation::Chrome120)
        .redirect(wreq::redirect::Policy::limited(10));

    let mut warning = None;
    if let Some(cfg) = proxy {
        let url = cfg.url();
        match wreq::Proxy::all(&url) {
            Ok(p) => {
                builder = builder.proxy(p);
                println!("[AVDL] Using proxy: {}", url);
            }
            Err(e) => {
                warning = Some(if cfg.scheme.starts_with("socks") {
                    "socks_unsupported".to_string()
                } else {
                    format!("invalid_proxy: {}", e)
                });
                println!("[AVDL] Proxy rejected ({}): {}", url, e);
            }
        }
    }

    (builder.build().expect("failed to build wreq client"), warning)
}

/// Detect the operating system's proxy settings.
pub fn detect_system_proxy() -> Detection {
    #[cfg(target_os = "macos")]
    {
        if let Some(d) = detect_macos_scutil() {
            return d;
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(d) = detect_windows_registry() {
            return d;
        }
    }

    if let Some(proxy) = detect_from_env() {
        return Detection {
            proxy: Some(proxy),
            warning: None,
        };
    }

    // Last-resort fallback (gsettings on Linux, networksetup on macOS).
    if let Some(proxy) = detect_sysproxy_crate() {
        return Detection {
            proxy: Some(proxy),
            warning: None,
        };
    }

    Detection::default()
}

/// macOS: read the effective system proxy from `scutil --proxy`.
#[cfg(target_os = "macos")]
fn detect_macos_scutil() -> Option<Detection> {
    let out = Command::new("scutil").arg("--proxy").output().ok()?;
    let text = String::from_utf8(out.stdout).ok()?;

    // PAC (WPAD / auto-config) without a static proxy cannot be consumed by
    // wreq — report it so the UI can explain the fallback to direct.
    if kv(&text, "ProxyAutoConfigEnable") == Some("1") {
        let has_static = kv(&text, "HTTPEnable") == Some("1")
            || kv(&text, "HTTPSEnable") == Some("1")
            || kv(&text, "SOCKSEnable") == Some("1");
        if !has_static {
            return Some(Detection {
                proxy: None,
                warning: Some("pac_unsupported"),
            });
        }
    }

    for (enable_key, host_key, port_key, scheme) in [
        ("HTTPEnable", "HTTPProxy", "HTTPPort", "http"),
        ("HTTPSEnable", "HTTPSProxy", "HTTPSPort", "https"),
        ("SOCKSEnable", "SOCKSProxy", "SOCKSPort", "socks5"),
    ] {
        if kv(&text, enable_key) == Some("1") {
            if let (Some(host), Some(port)) = (kv(&text, host_key), kv(&text, port_key)) {
                if let Ok(port) = port.parse::<u16>() {
                    return Some(Detection {
                        proxy: Some(ProxyConfig {
                            scheme: scheme.to_string(),
                            host: host.to_string(),
                            port,
                        }),
                        warning: None,
                    });
                }
            }
        }
    }

    None
}

/// Extract `key : value` from `scutil --proxy` output.
#[cfg(target_os = "macos")]
fn kv<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let marker = format!("{} : ", key);
    let idx = text.find(&marker)?;
    let rest = &text[idx + marker.len()..];
    let end = rest.find('\n').unwrap_or(rest.len());
    let val = rest[..end].trim();
    if val.is_empty() {
        None
    } else {
        Some(val)
    }
}

/// Windows: read the WinINET registry proxy settings. Parsing is tolerant of
/// the `ProxyServer` formats real tools write (`ip:port`, `localhost:port`,
/// `http=..;https=..` lists), unlike the strict `SocketAddr` parse used by
/// some helper crates.
#[cfg(target_os = "windows")]
fn detect_windows_registry() -> Option<Detection> {
    const SUB_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings";

    let enable = reg_query(SUB_KEY, "ProxyEnable")?;
    if enable.trim() != "0x1" {
        return None;
    }

    let server = reg_query(SUB_KEY, "ProxyServer")?;
    match parse_proxy_server(&server) {
        Some(proxy) => Some(Detection {
            proxy: Some(proxy),
            warning: None,
        }),
        None => Some(Detection {
            proxy: None,
            warning: Some("parse_failed"),
        }),
    }
}

/// Run `reg query <sub_key> /v <value>` and return the value text.
#[cfg(target_os = "windows")]
fn reg_query(sub_key: &str, value: &str) -> Option<String> {
    let out = Command::new("reg")
        .args(["query", sub_key, "/v", value])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines().find_map(|line| {
        let trimmed = line.trim();
        if trimmed.starts_with(value) {
            let mut parts = trimmed.split_whitespace();
            let _name = parts.next()?;
            let _kind = parts.next()?;
            parts.next().map(|v| v.to_string())
        } else {
            None
        }
    })
}

/// Parse a WinINET `ProxyServer` value.
#[cfg(target_os = "windows")]
fn parse_proxy_server(server: &str) -> Option<ProxyConfig> {
    let server = server.trim();
    if server.is_empty() {
        return None;
    }

    // Either a bare "host:port" or a per-protocol list:
    // "http=host:p;https=host:p;socks=host:p"
    let mut entries: Vec<(Option<String>, String)> = Vec::new();
    for part in server.split(';') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        match part.find('=') {
            Some(eq) => {
                let proto = part[..eq].trim().to_lowercase();
                entries.push((Some(proto), part[eq + 1..].trim().to_string()));
            }
            None => entries.push((None, part.to_string())),
        }
    }

    for scheme in ["https", "http", "socks"] {
        for (proto, addr) in &entries {
            if proto.as_deref() == Some(scheme) {
                let wreq_scheme = if scheme == "socks" { "socks5" } else { scheme };
                if let Some(cfg) = parse_addr(addr, wreq_scheme) {
                    return Some(cfg);
                }
            }
        }
    }

    // Bare "host:port" entry (the common Clash format).
    for (proto, addr) in &entries {
        if proto.is_none() {
            if let Some(cfg) = parse_addr(addr, "http") {
                return Some(cfg);
            }
        }
    }

    None
}

/// Parse a `host:port` (or `[ipv6]:port`) pair with an assumed scheme.
#[cfg(target_os = "windows")]
fn parse_addr(addr: &str, scheme: &str) -> Option<ProxyConfig> {
    let (host, port) = split_host_port(addr)?;
    Some(ProxyConfig {
        scheme: scheme.to_string(),
        host,
        port,
    })
}

/// Parse a proxy URL like `http://127.0.0.1:7890`, `socks5://host:1080` or a
/// bare `host:port` (assumed `default_scheme`). `socks://` is normalized to
/// `socks5://`.
pub fn parse_proxy_url(value: &str, default_scheme: &str) -> Option<ProxyConfig> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }

    let (scheme, rest) = match value.find("://") {
        Some(idx) => {
            let scheme = value[..idx].trim().to_lowercase();
            let scheme = if scheme == "socks" {
                "socks5".to_string()
            } else {
                scheme
            };
            (scheme, value[idx + 3..].trim())
        }
        None => (default_scheme.to_string(), value),
    };

    let (host, port) = split_host_port(rest)?;
    Some(ProxyConfig { scheme, host, port })
}

/// Split `host:port` into its parts, supporting IPv6 (`[::1]:7890`).
pub fn split_host_port(addr: &str) -> Option<(String, u16)> {
    let addr = addr.trim();
    if addr.is_empty() {
        return None;
    }

    let (host, port_str) = if let Some(rest) = addr.strip_prefix('[') {
        let end = rest.find(']')?;
        let port = rest[end + 1..].strip_prefix(':')?;
        (rest[..end].to_string(), port)
    } else {
        let idx = addr.rfind(':')?;
        (addr[..idx].to_string(), &addr[idx + 1..])
    };

    if host.is_empty() {
        return None;
    }
    let port: u16 = port_str.parse().ok()?;
    Some((host, port))
}

/// `HTTP(S)_PROXY` / `ALL_PROXY` environment variables.
fn detect_from_env() -> Option<ProxyConfig> {
    for (key, scheme) in [
        ("HTTPS_PROXY", "https"),
        ("https_proxy", "https"),
        ("HTTP_PROXY", "http"),
        ("http_proxy", "http"),
        ("ALL_PROXY", "http"),
        ("all_proxy", "http"),
    ] {
        if let Ok(val) = std::env::var(key) {
            if let Some(cfg) = parse_proxy_url(&val, scheme) {
                return Some(cfg);
            }
        }
    }
    None
}

/// Last-resort detection via the `sysproxy` crate (gsettings on Linux,
/// networksetup on macOS). It returns `host:port` without a scheme, so we
/// assume HTTP — Clash-style mixed ports accept HTTP CONNECT on the same port.
fn detect_sysproxy_crate() -> Option<ProxyConfig> {
    let p = Sysproxy::get_system_proxy().ok()?;
    if !p.enable || p.host.is_empty() || p.port == 0 {
        return None;
    }
    Some(ProxyConfig {
        scheme: "http".to_string(),
        host: p.host,
        port: p.port,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_host_port_basic() {
        assert_eq!(
            split_host_port("127.0.0.1:7890"),
            Some(("127.0.0.1".to_string(), 7890))
        );
        assert_eq!(
            split_host_port("localhost:8080"),
            Some(("localhost".to_string(), 8080))
        );
        assert_eq!(
            split_host_port("[::1]:7890"),
            Some(("::1".to_string(), 7890))
        );
        assert_eq!(split_host_port(""), None);
        assert_eq!(split_host_port("noport"), None);
        assert_eq!(split_host_port("host:notaport"), None);
    }

    #[test]
    fn parse_proxy_url_variants() {
        assert_eq!(
            parse_proxy_url("http://127.0.0.1:7890", "http"),
            Some(ProxyConfig {
                scheme: "http".to_string(),
                host: "127.0.0.1".to_string(),
                port: 7890
            })
        );
        assert_eq!(
            parse_proxy_url("socks5://127.0.0.1:1080", "http"),
            Some(ProxyConfig {
                scheme: "socks5".to_string(),
                host: "127.0.0.1".to_string(),
                port: 1080
            })
        );
        // socks:// is normalized to socks5://
        assert_eq!(
            parse_proxy_url("socks://127.0.0.1:1080", "http").unwrap().scheme,
            "socks5"
        );
        // bare host:port assumes the default scheme
        assert_eq!(
            parse_proxy_url("127.0.0.1:7890", "http").unwrap().scheme,
            "http"
        );
        assert_eq!(parse_proxy_url("", "http"), None);
        assert_eq!(parse_proxy_url("   ", "http"), None);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn parse_proxy_server_variants() {
        // Common Clash format: bare ip:port
        let cfg = parse_proxy_server("127.0.0.1:7897").unwrap();
        assert_eq!(cfg.host, "127.0.0.1");
        assert_eq!(cfg.port, 7897);
        assert_eq!(cfg.scheme, "http");

        // Hostname (sysproxy's strict SocketAddr parse would reject this)
        let cfg = parse_proxy_server("localhost:7897").unwrap();
        assert_eq!(cfg.host, "localhost");

        // Per-protocol list — prefer https, then http, then socks
        let cfg = parse_proxy_server("http=127.0.0.1:7890;https=127.0.0.1:7891").unwrap();
        assert_eq!(cfg.port, 7891);
        assert_eq!(cfg.scheme, "https");

        // socks entry maps to socks5
        let cfg = parse_proxy_server("socks=127.0.0.1:7892").unwrap();
        assert_eq!(cfg.scheme, "socks5");

        assert_eq!(parse_proxy_server(""), None);
        assert_eq!(parse_proxy_server("garbage"), None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn scutil_output_parsing() {
        let sample = "\
<dictionary> {
    ExceptionsList : <array> {
        0 : *.local
        1 : 169.254/16
    }
    FTPPassive : 1
    HTTPEnable : 1
    HTTPPort : 7897
    HTTPProxy : 127.0.0.1
    HTTPSEnable : 1
    HTTPSPort : 7897
    HTTPSProxy : 127.0.0.1
    ProxyAutoConfigEnable : 0
    SOCKSEnable : 0
    SOCKSProxy : 127.0.0.1
    SOCKSPort : 7897
}
";
        assert_eq!(kv(sample, "HTTPEnable"), Some("1"));
        assert_eq!(kv(sample, "HTTPProxy"), Some("127.0.0.1"));
        assert_eq!(kv(sample, "HTTPPort"), Some("7897"));
        assert_eq!(kv(sample, "SOCKSEnable"), Some("0"));
        assert_eq!(kv(sample, "ProxyAutoConfigEnable"), Some("0"));
        assert_eq!(kv(sample, "MissingKey"), None);
    }
}
