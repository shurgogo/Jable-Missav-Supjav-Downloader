use regex::Regex;
use std::error::Error;
use url::Url;
use wreq::Client;

use super::utils::{apply_cf_headers_safe, apply_referer, parse_hex};

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
