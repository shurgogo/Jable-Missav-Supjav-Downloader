use std::error::Error;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;
use tokio::sync::Semaphore;
use regex::Regex;
use url::Url;
use wreq::Client;
use tauri::Emitter;
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskControlState {
    Running,
    Paused,
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct TaskControlInfo {
    pub state: TaskControlState,
    pub title: String,
    pub save_dir: String,
    pub max_concurrent: usize,
    pub resolution: String,
}

pub type TaskRegistry = Arc<Mutex<HashMap<String, TaskControlInfo>>>;

pub fn apply_referer(mut req: wreq::RequestBuilder, original_url: &str) -> wreq::RequestBuilder {
    if let Ok(parsed_url) = Url::parse(original_url) {
        if let Some(host) = parsed_url.host_str() {
            let referer_url = format!("{}://{}/", parsed_url.scheme(), host);
            req = req.header("referer", &referer_url);
        }
    }
    req
}

pub fn apply_cf_headers_safe(
    mut req: wreq::RequestBuilder,
    target_url: &str,
    original_url: &str,
    cf_clearance: &str,
    user_agent: &str,
) -> wreq::RequestBuilder {
    let ua = user_agent.trim();
    if !ua.is_empty() {
        req = req.header("user-agent", ua);
    }

    let mut is_same_host = false;
    if let (Ok(u1), Ok(u2)) = (url::Url::parse(target_url), url::Url::parse(original_url)) {
        if let (Some(h1), Some(h2)) = (u1.host_str(), u2.host_str()) {
            let h1_clean = h1.trim_start_matches("www.").to_lowercase();
            let h2_clean = h2.trim_start_matches("www.").to_lowercase();
            
            // Allow related domains (e.g. missav.ws matches missav.ai, etc.)
            // Or if either contains the first 6 characters of the other's main domain name, but to be robust:
            // Let's check if the base domains are similar or if one is a substring of the other, 
            // or if it's the main site.
            let h1_base = h1_clean.split('.').next().unwrap_or("");
            let h2_base = h2_clean.split('.').next().unwrap_or("");
            
            if h1_clean.contains(&h2_clean)
                || h2_clean.contains(&h1_clean)
                || (!h1_base.is_empty() && h2_base.contains(h1_base))
                || (!h2_base.is_empty() && h1_base.contains(h2_base))
            {
                is_same_host = true;
            }
        }
    }

    if is_same_host {
        let cf = cf_clearance.trim();
        if !cf.is_empty() {
            let clean_cf = if cf.contains("cf_clearance=") {
                let re = regex::Regex::new(r#"cf_clearance=([^;,\s]+)"#).unwrap();
                if let Some(caps) = re.captures(cf) {
                    caps.get(1).map(|m| m.as_str().to_string()).unwrap_or_else(|| cf.to_string())
                } else {
                    cf.to_string()
                }
            } else {
                cf.to_string()
            };
            let clean_val = clean_cf.trim().trim_matches('"');
            req = req.header("cookie", &format!("cf_clearance={}", clean_val));
        }
    }
    
    req
}

pub fn decode_streamtape_url(html: &str) -> Option<String> {
    let re_tape = regex::Regex::new(
        r#"getElementById\(\s*['"]robotlink['"]\s*\)\.innerHTML\s*=\s*['"]([^'"]*)['"]\s*\+\s*(?:\['"\]{2}\s*\+\s*)?\(\s*['"]([^'"]*)['"]\s*\)((?:\.substring\(\s*\d+\s*\))+)"#
    ).ok()?;
    
    let caps = re_tape.captures(html)?;
    let prefix = caps.get(1)?.as_str();
    let suffix = caps.get(2)?.as_str();
    let subs = caps.get(3)?.as_str();
    
    let mut s = suffix.to_string();
    let re_sub = regex::Regex::new(r#"substring\(\s*(\d+)\s*\)"#).ok()?;
    for cap in re_sub.captures_iter(subs) {
        if let Some(off_str) = cap.get(1) {
            if let Ok(offset) = off_str.as_str().parse::<usize>() {
                if offset < s.len() {
                    s = s[offset..].to_string();
                }
            }
        }
    }
    
    let link = format!("{}{}", prefix, s).trim_start_matches('/').to_string();
    if !link.contains("get_video") {
        return None;
    }
    Some(format!("https://{}", link))
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProgressPayload {
    pub url: String,
    pub title: String,
    pub index: usize,
    pub total: usize,
    pub speed_kbps: f64,
    pub status: String, // "downloading" | "merging" | "completed" | "failed" | "paused"
}

pub struct JableVideoPageInfo {
    pub title: String,
    pub m3u8_url: String,
}

#[derive(Debug, Clone)]
pub struct M3u8Info {
    pub segments: Vec<String>,
    pub key_url: Option<String>,
    pub iv: Option<Vec<u8>>,
    pub total_duration: f64,
}

#[derive(Debug, Clone)]
pub struct Variant {
    pub uri: String,
    pub bandwidth: Option<usize>,
    pub resolution: Option<(usize, usize)>,
}

// Sanitization helper
pub fn sanitize_filename(title: &str) -> String {
    title
        .replace("/", "_")
        .replace("\\", "_")
        .replace(":", "_")
        .replace("*", "_")
        .replace("?", "_")
        .replace("\"", "_")
        .replace("<", "_")
        .replace(">", "_")
        .replace("|", "_")
}

// Helper to parse hex string into bytes
fn parse_hex(s: &str) -> Option<Vec<u8>> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    if s.len() % 2 != 0 {
        return None;
    }
    let mut bytes = Vec::new();
    for i in (0..s.len()).step_by(2) {
        let byte_str = &s[i..i+2];
        let byte = u8::from_str_radix(byte_str, 16).ok()?;
        bytes.push(byte);
    }
    Some(bytes)
}

// Helper to get IV for a segment index
fn get_iv_for_segment(index: usize, custom_iv: &Option<Vec<u8>>) -> [u8; 16] {
    if let Some(ref iv) = custom_iv {
        let mut res = [0u8; 16];
        let len = std::cmp::min(iv.len(), 16);
        res[16 - len..].copy_from_slice(&iv[..len]);
        res
    } else {
        let mut res = [0u8; 16];
        let bytes = (index as u128).to_be_bytes();
        res.copy_from_slice(&bytes[..]);
        res
    }
}

// 1. Parse JableTV page details
pub async fn parse_jable_page(client: &Client, url: &str) -> Result<JableVideoPageInfo, Box<dyn Error>> {
    let resp = client.get(url)
        .header("accept", "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8")
        .header("accept-language", "zh-TW,zh;q=0.9,en-US;q=0.8,en;q=0.7")
        .header("referer", "https://jable.tv/")
        .send()
        .await?;
        
    let text = resp.text().await?;
    
    let re_title = Regex::new(r#"og:title"\s+content="([^"]+)""#)?;
    let re_m3u8 = Regex::new(r#"https://[^\s"'`]+\.m3u8"#)?;
    
    let title = re_title.captures(&text)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| "jable_video".to_string());
        
    let m3u8_url = re_m3u8.find(&text)
        .map(|m| m.as_str().to_string())
        .ok_or_else(|| "Could not find M3U8 URL in the page HTML")?;
        
    Ok(JableVideoPageInfo { title, m3u8_url })
}

pub fn unpack_js_eval(script_text: &str) -> Option<String> {
    let re = Regex::new(
        r"eval\s*\(\s*function\s*\(\s*p\s*,\s*a\s*,\s*c\s*,\s*k\s*,\s*e\s*,\s*d\s*\)\s*\{[\s\S]*?\}\s*\(\s*'([\s\S]*?)'\s*,\s*(\d+)\s*,\s*(\d+)\s*,\s*'([\s\S]*?)'\s*\.split\s*\(\s*'\|'\s*\)"
    ).ok()?;

    let caps = re.captures(script_text)?;
    let packed = caps.get(1)?.as_str();
    let a: u32 = caps.get(2)?.as_str().parse().ok()?;
    let c: u32 = caps.get(3)?.as_str().parse().ok()?;
    let keys: Vec<&str> = caps.get(4)?.as_str().split('|').collect();

    if a <= 1 || c > 200000 {
        return None;
    }

    fn to_base(mut n: u32, base: u32) -> String {
        if n == 0 {
            return "0".to_string();
        }
        let digits = "0123456789abcdefghijklmnopqrstuvwxyz";
        let mut s = String::new();
        while n > 0 {
            let rem = (n % base) as usize;
            s.insert(0, digits.chars().nth(rem).unwrap());
            n /= base;
        }
        s
    }

    let mut lookup = HashMap::new();
    for i in 0..c {
        let key = to_base(i, a);
        let val = if i < keys.len() as u32 && !keys[i as usize].is_empty() {
            keys[i as usize].to_string()
        } else {
            key.clone()
        };
        lookup.insert(key, val);
    }

    let re_word = Regex::new(r"\b\w+\b").ok()?;
    let unpacked = re_word.replace_all(packed, |caps: &regex::Captures| {
        let word = &caps[0];
        lookup.get(word).cloned().unwrap_or_else(|| word.to_string())
    });

    Some(unpacked.to_string())
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

pub fn strip_fake_header(data: &[u8]) -> &[u8] {
    if data.is_empty() {
        return data;
    }
    if data[0] == 0x47 {
        return data;
    }
    
    let limit = if data.len() < 188 * 4 + 1 {
        return &[];
    } else {
        std::cmp::min(data.len() - 188 * 4 - 1, 8000)
    };
    
    let mut i = 0;
    while i <= limit {
        if let Some(pos) = data[i..=limit].iter().position(|&x| x == 0x47) {
            let j = i + pos;
            
            let mut all_match = true;
            for n in 0..5 {
                let check_idx = j + 188 * n;
                if check_idx >= data.len() || data[check_idx] != 0x47 {
                    all_match = false;
                    break;
                }
            }
            
            if all_match {
                return &data[j..];
            }
            i = j + 1;
        } else {
            break;
        }
    }
    
    &[]
}

fn parse_extinf_duration(line: &str) -> Option<f64> {
    let rest = line.strip_prefix("#EXTINF:")?;
    let dur_str = rest.split(',').next()?.trim();
    dur_str.parse::<f64>().ok().filter(|&d| d > 0.0)
}

fn parse_media_m3u8_content(text: &str, base_url: &Url) -> Result<M3u8Info, Box<dyn Error>> {
    let mut segments = Vec::new();
    let mut key_url = None;
    let mut iv = None;
    let mut total_duration: f64 = 0.0;
    let mut current_segment_duration: Option<f64> = None;
    
    let re_key = Regex::new(r#"#EXT-X-KEY:METHOD=AES-128,URI="([^"]+)"(?:,IV=0x([a-fA-F0-9]+))?"#)?;
    
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        
        if line.starts_with('#') {
            if line.starts_with("#EXTINF:") {
                current_segment_duration = parse_extinf_duration(line);
            } else if line.starts_with("#EXT-X-KEY") {
                if let Some(caps) = re_key.captures(line) {
                    if let Some(uri) = caps.get(1) {
                        let resolved_key_url = base_url.join(uri.as_str())?.to_string();
                        key_url = Some(resolved_key_url);
                    }
                    if let Some(iv_hex) = caps.get(2) {
                        iv = parse_hex(iv_hex.as_str());
                    }
                }
            }
        } else {
            let resolved_segment_url = base_url.join(line)?.to_string();
            segments.push(resolved_segment_url);
            if let Some(dur) = current_segment_duration.take() {
                total_duration += dur;
            }
        }
    }

    if total_duration == 0.0 && !segments.is_empty() {
        total_duration = segments.len() as f64 * 6.0;
    }
    
    Ok(M3u8Info { segments, key_url, iv, total_duration })
}

// Helper to parse stream inf resolution and bandwidth
fn parse_stream_inf(line: &str) -> (Option<usize>, Option<(usize, usize)>) {
    let mut bandwidth = None;
    let mut resolution = None;
    
    let parts = line.strip_prefix("#EXT-X-STREAM-INF:").unwrap_or("");
    for attr in parts.split(',') {
        let kv: Vec<&str> = attr.split('=').collect();
        if kv.len() == 2 {
            let key = kv[0].trim().to_uppercase();
            let val = kv[1].trim();
            if key == "BANDWIDTH" {
                bandwidth = val.parse::<usize>().ok();
            } else if key == "RESOLUTION" {
                let res_parts: Vec<&str> = val.split('x').collect();
                if res_parts.len() == 2 {
                    if let (Ok(w), Ok(h)) = (res_parts[0].parse::<usize>(), res_parts[1].parse::<usize>()) {
                        resolution = Some((w, h));
                    }
                }
            }
        }
    }
    (bandwidth, resolution)
}

// Helper to select variant based on resolution preference
fn select_variant(variants: &[Variant], pref: &str) -> Option<Variant> {
    if variants.is_empty() {
        return None;
    }
    
    let pref = pref.trim().to_lowercase();
    let mut sorted = variants.to_vec();
    sorted.sort_by(|a, b| {
        let ah = a.resolution.map(|r| r.1).unwrap_or(0);
        let bh = b.resolution.map(|r| r.1).unwrap_or(0);
        let ab = a.bandwidth.unwrap_or(0);
        let bb = b.bandwidth.unwrap_or(0);
        ah.cmp(&bh).then(ab.cmp(&bb))
    });
    
    if pref == "lowest" {
        return Some(sorted.first()?.clone());
    }
    if pref == "highest" {
        return Some(sorted.last()?.clone());
    }
    
    if let Ok(target_height) = pref.parse::<usize>() {
        let at_or_below: Vec<Variant> = sorted.iter()
            .filter(|v| v.resolution.map(|r| r.1).unwrap_or(0) <= target_height)
            .cloned()
            .collect();
            
        if !at_or_below.is_empty() {
            return Some(at_or_below.last()?.clone());
        }
    }
    
    Some(sorted.last()?.clone())
}

// 2. Parse Master or Media M3U8 (Supports recursion & sub-playlists)
pub fn parse_master_or_media_m3u8<'a>(
    client: &'a Client,
    m3u8_url: &'a str,
    resolution_pref: &'a str,
    original_url: &'a str,
    cf_clearance: &'a str,
    user_agent: &'a str,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<M3u8Info, Box<dyn Error>>> + Send + 'a>> {
    Box::pin(async move {
        parse_m3u8_recursive(client, m3u8_url, resolution_pref, original_url, cf_clearance, user_agent, 0).await
    })
}

fn parse_m3u8_recursive<'a>(
    client: &'a Client,
    m3u8_url: &'a str,
    resolution_pref: &'a str,
    original_url: &'a str,
    cf_clearance: &'a str,
    user_agent: &'a str,
    depth: usize,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<M3u8Info, Box<dyn Error>>> + Send + 'a>> {
    Box::pin(async move {
        if depth > 5 {
            let err: Box<dyn Error> = "M3U8 playlist recursion depth limit exceeded (> 5)".into();
            return Err(err);
        }

        let mut req = client.get(m3u8_url);
        req = apply_referer(req, original_url);
        req = apply_cf_headers_safe(req, m3u8_url, original_url, cf_clearance, user_agent);
        let resp = req.send().await?;
        let text = resp.text().await?;
        let base_url = Url::parse(m3u8_url)?;

        // 1. Check if it's a Master Playlist with #EXT-X-STREAM-INF
        if text.contains("#EXT-X-STREAM-INF") {
            println!("[Downloader] Master playlist detected at depth {}: {}", depth, m3u8_url);
            let mut variants = Vec::new();
            let lines: Vec<&str> = text.lines().map(|l| l.trim()).collect();
            
            let mut i = 0;
            while i < lines.len() {
                let line = lines[i];
                if line.starts_with("#EXT-X-STREAM-INF:") {
                    let (bandwidth, resolution) = parse_stream_inf(line);
                    let mut j = i + 1;
                    while j < lines.len() {
                        let next_line = lines[j];
                        if !next_line.is_empty() && !next_line.starts_with('#') {
                            if let Ok(resolved_uri) = base_url.join(next_line) {
                                variants.push(Variant {
                                    uri: resolved_uri.to_string(),
                                    bandwidth,
                                    resolution,
                                });
                            }
                            break;
                        }
                        j += 1;
                    }
                    i = j;
                }
                i += 1;
            }

            if variants.is_empty() {
                let err: Box<dyn Error> = "Master playlist contains no valid STREAM-INF variants".into();
                return Err(err);
            }

            let mut resolved_variants: Vec<(Variant, M3u8Info)> = Vec::new();
            for var in variants {
                match parse_m3u8_recursive(client, &var.uri, resolution_pref, original_url, cf_clearance, user_agent, depth + 1).await {
                    Ok(info) => {
                        resolved_variants.push((var, info));
                    }
                    Err(e) => {
                        println!("[Downloader] Failed to resolve variant {}: {}", var.uri, e);
                    }
                }
            }

            if resolved_variants.is_empty() {
                let err: Box<dyn Error> = "Failed to resolve any STREAM-INF variants".into();
                return Err(err);
            }

            // Prefer full video variants (total_duration >= 600s)
            let full_variants: Vec<&(Variant, M3u8Info)> = resolved_variants
                .iter()
                .filter(|(_, info)| info.total_duration >= 600.0)
                .collect();

            let candidates = if !full_variants.is_empty() {
                full_variants
            } else {
                resolved_variants.iter().collect()
            };

            let candidate_variants: Vec<Variant> = candidates.iter().map(|(v, _)| v.clone()).collect();
            if let Some(best) = select_variant(&candidate_variants, resolution_pref) {
                if let Some((_, info)) = candidates.into_iter().find(|(v, _)| v.uri == best.uri) {
                    println!(
                        "[Downloader] Selected variant stream: resolution={:?}, uri={}, duration={:.1}s, segments={}",
                        best.resolution, best.uri, info.total_duration, info.segments.len()
                    );
                    return Ok(info.clone());
                }
            }

            return Ok(resolved_variants[0].1.clone());
        }

        // 2. Check if it's a Media Playlist containing #EXTINF:
        if text.contains("#EXTINF:") {
            return parse_media_m3u8_content(&text, &base_url);
        }

        // 3. No #EXTINF: and no #EXT-X-STREAM-INF: check for sub-playlist .m3u8 URLs
        let mut sub_m3u8_urls = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let path_part = line.split('?').next().unwrap_or(line);
            if path_part.ends_with(".m3u8") || path_part.contains(".m3u8") {
                if let Ok(resolved) = base_url.join(line) {
                    sub_m3u8_urls.push(resolved.to_string());
                }
            }
        }

        if !sub_m3u8_urls.is_empty() {
            println!("[Downloader] Sub-playlist M3U8 links detected at depth {}: {:?}", depth, sub_m3u8_urls);
            let mut resolved_infos = Vec::new();
            for sub_url in sub_m3u8_urls {
                if let Ok(info) = parse_m3u8_recursive(client, &sub_url, resolution_pref, original_url, cf_clearance, user_agent, depth + 1).await {
                    resolved_infos.push(info);
                }
            }
            if !resolved_infos.is_empty() {
                if let Some(best_info) = resolved_infos.into_iter().max_by(|a, b| a.total_duration.partial_cmp(&b.total_duration).unwrap()) {
                    return Ok(best_info);
                }
            }
        }

        // Fallback parse as media playlist
        parse_media_m3u8_content(&text, &base_url)
    })
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
        req2 = crate::scraper::apply_cf_headers(req2, cf_clearance, user_agent);

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
                let mut probe_req = client.head(&mp4_url);
                probe_req = apply_referer(probe_req, url);
                probe_req = apply_cf_headers_safe(probe_req, &mp4_url, url, cf_clearance, user_agent);
                probe_req = probe_req.timeout(std::time::Duration::from_secs(5));

                let (content_length, is_ok) = match probe_req.send().await {
                    Ok(probe_resp) if probe_resp.status().is_success() => {
                        let cl = probe_resp.content_length().unwrap_or(0);
                        (cl, true)
                    }
                    _ => {
                        let mut get_probe = client.get(&mp4_url);
                        get_probe = apply_referer(get_probe, url);
                        get_probe = apply_cf_headers_safe(get_probe, &mp4_url, url, cf_clearance, user_agent);
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

// 3. Main async task downloader with control registry and user preferences
pub async fn download_video(
    client: Client,
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
        let _ = window.emit("download-progress", ProgressPayload {
            url: url.clone(),
            title: "".to_string(),
            index: 0,
            total: 0,
            speed_kbps: 0.0,
            status: format!("failed: {}", err_msg),
        });
    };

    println!("[Downloader] Starting/Resuming download for: {}", url);

    // Step 1: Parse page HTML
    let page_info = if url.contains("missav") {
        let referer_url = if let Ok(parsed_url) = Url::parse(&url) {
            format!("{}://{}/", parsed_url.scheme(), parsed_url.host_str().unwrap_or("missav.ai"))
        } else {
            "https://missav.ai/".to_string()
        };

        let mut req = client.get(&url)
            .header("accept", "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8")
            .header("accept-language", "zh-TW,zh;q=0.9,en-US;q=0.8,en;q=0.7")
            .header("referer", &referer_url);
            
        req = crate::scraper::apply_cf_headers(req, &cf_clearance, &user_agent);

        match req.send().await {
            Ok(resp) => {
                match resp.text().await {
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
                }
            }
            Err(e) => {
                emit_fail(&format!("Failed to request MissAV page: {}", e));
                return;
            }
        }
    } else if url.contains("supjav") {
        match parse_supjav_page(&client, &url, &cf_clearance, &user_agent).await {
            Ok(info) => info,
            Err(e) => {
                emit_fail(&format!("Failed to parse SupJav page: {}", e));
                return;
            }
        }
    } else {
        // JableTV
        let mut req = client.get(&url)
            .header("referer", "https://jable.tv/");
        req = crate::scraper::apply_cf_headers(req, &cf_clearance, &user_agent);

        match req.send().await {
            Ok(resp) => {
                // Since parse_jable_page expects client and url, let's load text manually and parse it to support cookies!
                match resp.text().await {
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
                }
            }
            Err(e) => {
                emit_fail(&format!("Failed to request JableTV page: {}", e));
                return;
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
    let is_direct_mp4 = page_info.m3u8_url.contains(".mp4") || !page_info.m3u8_url.contains(".m3u8");
    if is_direct_mp4 {
        println!("[Downloader] Direct MP4 stream detected. Downloading file directly: {}", page_info.m3u8_url);
        
        let base_save_path = std::path::PathBuf::from(&save_dir);
        let final_mp4_path = base_save_path.join(format!("{}.mp4", safe_title));
        
        // Check if file already exists
        if final_mp4_path.exists() {
            println!("[Downloader] Direct MP4 file already exists: {:?}", final_mp4_path);
            
            // Remove from registry
            {
                let mut states = task_states.lock().unwrap();
                states.remove(&url);
            }
            
            // Completed
            let _ = window.emit("download-progress", ProgressPayload {
                url: url.clone(),
                title: page_info.title,
                index: 100,
                total: 100,
                speed_kbps: 0.0,
                status: "completed".to_string(),
            });
            return;
        }
        
        let mut req = client.get(&page_info.m3u8_url);
        req = apply_referer(req, &url);
        req = apply_cf_headers_safe(req, &page_info.m3u8_url, &url, &cf_clearance, &user_agent);
        
        match req.send().await {
            Ok(resp) => {
                if !resp.status().is_success() {
                    emit_fail(&format!("Direct MP4 download failed: HTTP {}", resp.status()));
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
                
                while let Some(chunk_result) = stream_pin.next().await {
                    // Check task state (pause/cancel)
                    {
                        let states = task_states.lock().unwrap();
                        if let Some(info) = states.get(&url) {
                            if info.state == TaskControlState::Paused || info.state == TaskControlState::Cancelled {
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
                            
                            // Emit progress every 500ms
                            if last_emit.elapsed().as_millis() > 500 {
                                last_emit = std::time::Instant::now();
                                let progress_percent = if total_bytes > 0 {
                                    (downloaded as f64 / total_bytes as f64 * 100.0) as usize
                                } else {
                                    50
                                };
                                
                                let _ = window.emit("download-progress", ProgressPayload {
                                    url: url.clone(),
                                    title: page_info.title.clone(),
                                    index: progress_percent,
                                    total: 100,
                                    speed_kbps: 0.0,
                                    status: "downloading".to_string(),
                                });
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
                let _ = window.emit("download-progress", ProgressPayload {
                    url: url.clone(),
                    title: page_info.title,
                    index: 100,
                    total: 100,
                    speed_kbps: 0.0,
                    status: "completed".to_string(),
                });
                return;
            }
            Err(e) => {
                emit_fail(&format!("Failed to request direct MP4 stream: {}", e));
                return;
            }
        }
    }

    // Step 2: Parse Master or Media M3U8 based on resolution preference
    let m3u8_info = match parse_master_or_media_m3u8(&client, &page_info.m3u8_url, &resolution_pref, &url, &cf_clearance, &user_agent).await {
        Ok(info) => info,
        Err(e) => {
            emit_fail(&format!("Failed to parse M3U8 playlist: {}", e));
            return;
        }
    };

    // Step 3: Fetch Encryption Key if needed
    let mut key_bytes = None;
    if let Some(ref k_url) = m3u8_info.key_url {
        println!("[Downloader] Stream is encrypted. Fetching key from: {}", k_url);
        let mut req = client.get(k_url);
        req = apply_referer(req, &url);
        req = apply_cf_headers_safe(req, k_url, &url, &cf_clearance, &user_agent);
        match req.send().await {
            Ok(resp) => {
                match resp.bytes().await {
                    Ok(bytes) => {
                        key_bytes = Some(bytes.to_vec());
                    }
                    Err(e) => {
                        emit_fail(&format!("Failed to download decryption key: {}", e));
                        return;
                    }
                }
            }
            Err(e) => {
                emit_fail(&format!("Failed to request decryption key: {}", e));
                return;
            }
        }
    }

    // Step 4: Create temp directory
    let base_save_path = PathBuf::from(&save_dir);

    // Check if the final merged video file already exists
    let final_mp4_check = base_save_path.join(format!("{}.mp4", safe_title));
    let final_ts_check = base_save_path.join(format!("{}.ts", safe_title));
    if final_mp4_check.exists() || final_ts_check.exists() {
        println!("[Downloader] Target merged video file already exists: {:?}", final_mp4_check);

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
        let _ = window.emit("download-progress", ProgressPayload {
            url: url.clone(),
            title: page_info.title,
            index: total_segs,
            total: total_segs,
            speed_kbps: 0.0,
            status: "completed".to_string(),
        });
        return;
    }

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
        url: String,
        title: String,
        save_dir: String,
        max_concurrent: usize,
        resolution: String,
        total_segments: usize,
        m3u8_url: String,
    }
    let meta = TaskMetadata {
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

    // Step 5: Concurrent Downloading
    let total_segments = m3u8_info.segments.len();
    let semaphore = Arc::new(Semaphore::new(max_concurrent.clamp(1, 16)));
    let completed_count = Arc::new(AtomicUsize::new(0));
    let bytes_count = Arc::new(AtomicUsize::new(0));
    let start_time = Instant::now();

    let mut join_handles = Vec::new();
    let key_bytes_arc = Arc::new(key_bytes);
    let custom_iv_arc = Arc::new(m3u8_info.iv);

    for (index, segment_url) in m3u8_info.segments.iter().enumerate() {
        let sem = Arc::clone(&semaphore);
        let completed = Arc::clone(&completed_count);
        let bytes = Arc::clone(&bytes_count);
        let client_clone = client.clone();
        let segment_url_clone = segment_url.clone();
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
            // Check control state before proceeding
            {
                let states = task_states_clone.lock().unwrap();
                if let Some(info) = states.get(&url_clone) {
                    if info.state == TaskControlState::Paused || info.state == TaskControlState::Cancelled {
                        return Ok::<(), String>(());
                    }
                }
            }

            // Breakpoint resume logic: skip if file already exists and is non-empty
            let file_path = temp_path.join(format!("{}.ts", index));
            if file_path.exists() && file_path.metadata().map(|m| m.len()).unwrap_or(0) > 0 {
                let done = completed.fetch_add(1, Ordering::Relaxed) + 1;
                let _ = window_clone.emit("download-progress", ProgressPayload {
                    url: url_clone,
                    title: title_clone,
                    index: done,
                    total: total_segments,
                    speed_kbps: 0.0,
                    status: "downloading".to_string(),
                });
                return Ok::<(), String>(());
            }

            let _permit = sem.acquire().await.map_err(|e| e.to_string())?;
            
            // Re-check control state after acquiring permit
            {
                let states = task_states_clone.lock().unwrap();
                if let Some(info) = states.get(&url_clone) {
                    if info.state == TaskControlState::Paused || info.state == TaskControlState::Cancelled {
                        return Ok::<(), String>(());
                    }
                }
            }

            let mut req = client_clone.get(&segment_url_clone);
            req = apply_referer(req, &url_clone);
            req = apply_cf_headers_safe(req, &segment_url_clone, &url_clone, &cf_clearance_clone, &user_agent_clone);
            let resp = req.send().await.map_err(|e| e.to_string())?;
            let mut data = resp.bytes().await.map_err(|e| e.to_string())?.to_vec();
            
            // Decrypt in place if encrypted
            if let Some(ref key) = *key_bytes_clone {
                let iv = get_iv_for_segment(index, &*custom_iv_clone);
                
                use aes::cipher::{block_padding::NoPadding, BlockDecryptMut, KeyIvInit};
                type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;
                
                let dec = Aes128CbcDec::new_from_slices(key, &iv).map_err(|e| e.to_string())?;
                
                // Decrypt in-place using NoPadding since TS streams are padded block-aligned
                let decrypted_data = dec.decrypt_padded_mut::<NoPadding>(&mut data)
                    .map_err(|e| format!("Decryption error: {}", e))?;
                
                let final_data = if url_clone.contains("supjav") {
                    strip_fake_header(decrypted_data)
                } else {
                    decrypted_data
                };
                
                std::fs::write(&file_path, final_data).map_err(|e| e.to_string())?;
            } else {
                let final_data = if url_clone.contains("supjav") {
                    strip_fake_header(&data)
                } else {
                    &data
                };
                std::fs::write(&file_path, final_data).map_err(|e| e.to_string())?;
            }
            
            let done = completed.fetch_add(1, Ordering::Relaxed) + 1;
            bytes.fetch_add(data.len(), Ordering::Relaxed);
            
            let elapsed = start_time.elapsed().as_secs_f64();
            let speed = if elapsed > 0.0 {
                (bytes.load(Ordering::Relaxed) as f64) / 1024.0 / elapsed
            } else {
                0.0
            };
            
            let _ = window_clone.emit("download-progress", ProgressPayload {
                url: url_clone,
                title: title_clone,
                index: done,
                total: total_segments,
                speed_kbps: speed,
                status: "downloading".to_string(),
            });
            
            Ok::<(), String>(())
        });
        
        join_handles.push(handle);
    }

    // Wait for all worker handles to complete
    for handle in join_handles {
        let _ = handle.await;
    }

    // Re-evaluate control state
    let final_state = {
        let states = task_states.lock().unwrap();
        states.get(&url).map(|info| info.state).unwrap_or(TaskControlState::Running)
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
        let _ = window.emit("download-progress", ProgressPayload {
            url: url.clone(),
            title: page_info.title,
            index: completed_count.load(Ordering::Relaxed),
            total: total_segments,
            speed_kbps: 0.0,
            status: "paused".to_string(),
        });
        return;
    }

    // Step 6: Merging segments (Only if still running)
    let _ = window.emit("download-progress", ProgressPayload {
        url: url.clone(),
        title: page_info.title.clone(),
        index: total_segments,
        total: total_segments,
        speed_kbps: 0.0,
        status: "merging".to_string(),
    });

    let final_mp4_path = base_save_path.join(format!("{}.mp4", safe_title));
    let final_ts_path = base_save_path.join(format!("{}.ts", safe_title));

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
                println!("[Downloader] FFmpeg merged successfully: {:?}", final_mp4_path);
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
            emit_fail(&format!("Binary merge failed: {}", e));
            let _ = std::fs::remove_dir_all(&temp_dir_path);
            return;
        }
        println!("[Downloader] Binary merge completed: {:?}", final_ts_path);
    }

    // Clean up temporary files
    let _ = std::fs::remove_dir_all(&temp_dir_path);

    // Remove from registry
    {
        let mut states = task_states.lock().unwrap();
        states.remove(&url);
    }

    // Completed
    let _ = window.emit("download-progress", ProgressPayload {
        url: url.clone(),
        title: page_info.title,
        index: total_segments,
        total: total_segments,
        speed_kbps: 0.0,
        status: "completed".to_string(),
    });
}


