use regex::Regex;
use std::error::Error;
use wreq::Client;

use super::m3u8::parse_master_or_media_m3u8;
use super::utils::{decode_streamtape_url, unpack_js_eval};

pub struct JableVideoPageInfo {
    pub title: String,
    pub m3u8_url: String,
}

pub fn parse_missav_page(html: &str) -> Result<JableVideoPageInfo, Box<dyn Error>> {
    let re_title = Regex::new(r#"og:title"\s+content="([^"]+)""#)?;
    let title = re_title.captures(html)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| "missav_video".to_string());
        
    let re_script = Regex::new(r#"(?s)<script[^>]*>(.*?)</script>"#)?;
    let mut m3u8_url = None;
    
    for cap in re_script.captures_iter(html) {
        if let Some(script_content) = cap.get(1) {
            let script_text = script_content.as_str();
            if script_text.contains("eval(function") && script_text.contains("m3u8") {
                if let Some(unpacked) = unpack_js_eval(script_text) {
                    let re_m3u8_url = Regex::new(r#"source\s*=\s*[\\']*(https?://[^'\\;\s]+\.m3u8)"#)?;
                    if let Some(c_url) = re_m3u8_url.captures(&unpacked).and_then(|c| c.get(1)) {
                        m3u8_url = Some(c_url.as_str().to_string());
                        break;
                    }
                    
                    let re_any_m3u8 = Regex::new(r#"(https?://[^\'\\;\s]+\.m3u8)"#)?;
                    if let Some(c_url) = re_any_m3u8.captures(&unpacked).and_then(|c| c.get(1)) {
                        m3u8_url = Some(c_url.as_str().to_string());
                        break;
                    }
                }
            }
        }
    }
    
    let m3u8 = m3u8_url.ok_or_else(|| "Could not find M3U8 stream URL in MissAV page. Possibly Cloudflare challenged or script changed.")?;
    
    Ok(JableVideoPageInfo { title, m3u8_url: m3u8 })
}

pub async fn parse_supjav_page(
    client: &Client,
    url: &str,
    cf_clearance: &str,
    user_agent: &str,
) -> Result<JableVideoPageInfo, Box<dyn Error>> {
    let mut req = client.get(url)
        .header("referer", "https://supjav.com/");
    req = crate::scraper::apply_cf_headers(req, cf_clearance, user_agent);
    let resp = req.send().await?;
    let html = resp.text().await?;

    let (title, servers) = {
        let doc = dom_query::Document::from(html.as_str());
        
        let h1_text = doc.select("h1").text().to_string().trim().to_string();
        let title = if !h1_text.is_empty() {
            h1_text
        } else {
            doc.select("title").text().to_string().trim().to_string()
        };
        
        let anchors = doc.select("a.btn-server[data-link]");
        let mut servers = Vec::new();
        for node in anchors.iter() {
            let name = node.text().to_string().trim().to_uppercase();
            if let Some(link) = node.attr("data-link").map(|v| v.to_string()) {
                servers.push((name, link));
            }
        }
        (title, servers)
    };

    if servers.is_empty() {
        return Err("No server sources found on SupJav page".into());
    }

    let re_play = Regex::new(r#"urlPlay[\s=:\'"]+(https?://[^\s'"\\]+\.m3u8[^\s'"\\]*)"#)?;
    let re_any_m3u8 = Regex::new(r#"(https?://[^\s'"\\]+\.m3u8[^\s'"\\]*)"#)?;

    struct EvaluatedServer {
        name: String,
        stream_url: String,
        duration: f64,
    }

    let mut evaluated_servers: Vec<EvaluatedServer> = Vec::new();
    let mut last_error = None;

    for (name, link) in servers {
        println!("[Downloader] Evaluating SupJav server [{}]: data-link", name);
        let reversed_link: String = link.chars().rev().collect();
        let supreme_url = format!("https://lk1.supremejav.com/supjav.php?c={}", reversed_link);
        
        let mut req2 = client.get(&supreme_url)
            .header("referer", "https://supjav.com/");
        if !user_agent.trim().is_empty() {
            req2 = req2.header("user-agent", user_agent.trim());
        }

        let body = match req2.send().await {
            Ok(resp2) => match resp2.text().await {
                Ok(b) => b,
                Err(e) => {
                    println!("[Downloader] Failed to read body from server [{}]: {}", name, e);
                    last_error = Some(e.to_string());
                    continue;
                }
            },
            Err(e) => {
                println!("[Downloader] Failed to connect to server [{}]: {}", name, e);
                last_error = Some(e.to_string());
                continue;
            }
        };

        if name == "ST" {
            if let Some(mp4_url) = decode_streamtape_url(&body) {
                println!("[Downloader] Streamtape server [{}] direct MP4 URL: {}", name, mp4_url);
                let mut probe_req = client.head(&mp4_url)
                    .header("referer", "https://streamtape.com/");
                if !user_agent.trim().is_empty() {
                    probe_req = probe_req.header("user-agent", user_agent.trim());
                }
                probe_req = probe_req.timeout(std::time::Duration::from_secs(5));

                let (content_length, is_ok) = match probe_req.send().await {
                    Ok(probe_resp) if probe_resp.status().is_success() => {
                        let cl = probe_resp.content_length().unwrap_or(0);
                        (cl, true)
                    }
                    _ => {
                        let mut get_probe = client.get(&mp4_url)
                            .header("referer", "https://streamtape.com/");
                        if !user_agent.trim().is_empty() {
                            get_probe = get_probe.header("user-agent", user_agent.trim());
                        }
                        get_probe = get_probe.header("range", "bytes=0-100");
                        get_probe = get_probe.timeout(std::time::Duration::from_secs(5));
                        match get_probe.send().await {
                            Ok(r) if r.status().is_success() || r.status().as_u16() == 206 => {
                                (r.content_length().unwrap_or(0), true)
                            }
                            Err(e) => {
                                println!("[Downloader] Connection to Streamtape server [{}] failed: {}", name, e);
                                (0, false)
                            }
                            _ => (0, false),
                        }
                    }
                };

                if is_ok {
                    let is_full = content_length == 0 || content_length >= 150_000_000;
                    let dur = if is_full { 3600.0 } else { 180.0 };
                    let server_name = name.clone();
                    println!(
                        "[Downloader] Evaluated Streamtape server [{}]: size={:.1}MB, is_full={}",
                        server_name, content_length as f64 / 1_048_576.0, is_full
                    );
                    evaluated_servers.push(EvaluatedServer {
                        name: server_name,
                        stream_url: mp4_url,
                        duration: dur,
                    });
                    if is_full {
                        let selected = evaluated_servers.last().unwrap();
                        println!("[Downloader] Selected server [{}] (Streamtape Full Video)", selected.name);
                        return Ok(JableVideoPageInfo { title, m3u8_url: selected.stream_url.clone() });
                    } else {
                        let last_size = content_length as f64 / 1_048_576.0;
                        println!("[Downloader] Server [{}] (Streamtape) is a short preview clip (size={:.1}MB < 150MB). Skipping...", name, last_size);
                    }
                }
            } else {
                println!("[Downloader] Streamtape server [{}] robotlink not found", name);
            }
        } else {
            let body_clean = body.replace("\\/", "/");
            let m3u8 = if let Some(caps) = re_play.captures(&body_clean) {
                caps.get(1).map(|m| m.as_str().to_string())
            } else if let Some(m) = re_any_m3u8.captures(&body_clean) {
                m.get(1).map(|m| m.as_str().to_string())
            } else {
                None
            };

            if let Some(m3u8_url) = m3u8 {
                println!("[Downloader] Server [{}] returned initial M3U8: {}", name, m3u8_url);
                match parse_master_or_media_m3u8(client, &m3u8_url, "highest", url, cf_clearance, user_agent).await {
                    Ok(m3u8_info) => {
                        let dur = m3u8_info.total_duration;
                        let is_full = dur >= 600.0;
                        let server_name = name.clone();
                        println!(
                            "[Downloader] Evaluated server [{}]: total_duration={:.1}s ({:.1} min), segments={}, is_full={}",
                            server_name, dur, dur / 60.0, m3u8_info.segments.len(), is_full
                        );
                        evaluated_servers.push(EvaluatedServer {
                            name: server_name,
                            stream_url: m3u8_url,
                            duration: dur,
                        });
                        if is_full {
                            let selected = evaluated_servers.last().unwrap();
                            println!("[Downloader] Selected server [{}] (Full Video M3U8, duration={:.1} min)", selected.name, dur / 60.0);
                            return Ok(JableVideoPageInfo { title, m3u8_url: selected.stream_url.clone() });
                        } else {
                            println!("[Downloader] Server [{}] is a short preview/fake video (duration={:.1} min < 10 min). Skipping...", name, dur / 60.0);
                        }
                    }
                    Err(e) => {
                        println!("[Downloader] Failed to resolve M3U8 for server [{}]: {}", name, e);
                        last_error = Some(e.to_string());
                    }
                }
            }
        }
    }

    if !evaluated_servers.is_empty() {
        if let Some(best) = evaluated_servers.iter().max_by(|a, b| a.duration.partial_cmp(&b.duration).unwrap()) {
            println!(
                "[Downloader] WARNING: No server met full video threshold (>= 10 min). Falling back to longest available server [{}] (duration={:.1} min)",
                best.name, best.duration / 60.0
            );
            return Ok(JableVideoPageInfo { title, m3u8_url: best.stream_url.clone() });
        }
    }

    Err(format!(
        "All SupJav servers failed. Last error: {}",
        last_error.unwrap_or_else(|| "unknown error".to_string())
    ).into())
}
