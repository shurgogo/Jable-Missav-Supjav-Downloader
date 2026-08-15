use regex::Regex;
use std::error::Error;
use url::Url;
use wreq::Client;

use super::utils::{apply_cf_headers_safe, apply_referer, parse_hex};

/// One media segment (or init segment) of an HLS playlist.
///
/// Segments are NOT assumed to be MPEG-TS: HLS can deliver fMP4 (`.m4s`),
/// ADTS AAC, raw files, etc. We simply record the URL plus the optional
/// byte-range from `#EXT-X-BYTERANGE` and let the downloader handle the data.
#[derive(Debug, Clone)]
pub struct Segment {
    pub url: String,
    /// (start, length) from `#EXT-X-BYTERANGE:length@start`, when present.
    pub byte_range: Option<(u64, u64)>,
}

#[derive(Debug, Clone)]
pub struct M3u8Info {
    pub segments: Vec<Segment>,
    /// fMP4 init segment from `#EXT-X-MAP` (must be prepended when merging).
    pub init_segment: Option<Segment>,
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

/// Parse a `BYTERANGE` attribute value (`"82112@752"` or `"82112"` when the
/// range continues from the previous one). Returns (start, length).
fn parse_byterange_attr(
    value: &str,
    last_end: Option<u64>,
) -> Option<(u64, u64)> {
    let value = value.trim().trim_matches('"');
    let mut parts = value.split('@');
    let len = parts.next()?.trim().parse::<u64>().ok()?;
    let start = match parts.next() {
        Some(s) if !s.trim().is_empty() => s.trim().parse::<u64>().ok()?,
        _ => last_end.unwrap_or(0),
    };
    Some((start, len))
}

/// Extract a quoted attribute like `URI="init.mp4"` from an EXT-X-* line.
fn extract_attr(attrs: &str, name: &str) -> Option<String> {
    let re = Regex::new(&format!(r#"{}=\"([^\"]+)\""#, regex::escape(name))).ok()?;
    re.captures(attrs)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}

/// Extract an unquoted attribute like `METHOD=AES-128` or `IV=0x...`.
fn extract_unquoted_attr(attrs: &str, name: &str) -> Option<String> {
    let re = Regex::new(&format!(r#"{}=([^,\s]+)"#, regex::escape(name))).ok()?;
    re.captures(attrs)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}

fn parse_media_m3u8_content(text: &str, base_url: &Url) -> Result<M3u8Info, Box<dyn Error>> {
    let mut segments = Vec::new();
    let mut init_segment = None;
    let mut key_url = None;
    let mut iv = None;
    let mut total_duration: f64 = 0.0;
    let mut current_segment_duration: Option<f64> = None;
    // Pending `#EXT-X-BYTERANGE` that applies to the next segment URI line.
    let mut pending_byterange: Option<(u64, u64)> = None;
    // End of the last byte-range, for BYTERANGE values without an explicit @start.
    let mut last_byterange_end: Option<u64> = None;

    for raw_line in text.lines() {
        // Trim whitespace and a possible UTF-8 BOM on the first line.
        let line = raw_line.trim().trim_start_matches('\u{feff}');
        if line.is_empty() {
            continue;
        }

        if line.starts_with('#') {
            if line.starts_with("#EXTINF:") {
                current_segment_duration = parse_extinf_duration(line);
            } else if line.starts_with("#EXT-X-KEY") {
                // Attribute order is not guaranteed by the spec; parse the
                // whole attribute list instead of a rigid regex.
                let attrs = line.strip_prefix("#EXT-X-KEY:").unwrap_or_default();
                // METHOD=NONE disables encryption for subsequent segments.
                if attrs.contains("METHOD=NONE") {
                    key_url = None;
                    iv = None;
                } else if extract_unquoted_attr(attrs, "METHOD").as_deref() == Some("AES-128") {
                    if let Some(uri) = extract_attr(attrs, "URI") {
                        let resolved_key_url = base_url.join(&uri)?.to_string();
                        key_url = Some(resolved_key_url);
                    }
                    if let Some(iv_hex) = extract_unquoted_attr(attrs, "IV") {
                        iv = parse_hex(&iv_hex);
                    }
                }
            } else if line.starts_with("#EXT-X-MAP") {
                // fMP4 init segment. URI is mandatory; BYTERANGE is optional.
                let attrs = line
                    .strip_prefix("#EXT-X-MAP:")
                    .unwrap_or_default()
                    .trim();
                if let Some(uri) = extract_attr(attrs, "URI") {
                    let resolved = base_url.join(&uri)?;
                    let range = extract_attr(attrs, "BYTERANGE")
                        .and_then(|v| parse_byterange_attr(&v, last_byterange_end));
                    if let Some((start, len)) = range {
                        last_byterange_end = Some(start + len);
                    }
                    init_segment = Some(Segment {
                        url: resolved.to_string(),
                        byte_range: range,
                    });
                }
            } else if line.starts_with("#EXT-X-BYTERANGE") {
                let value = line.strip_prefix("#EXT-X-BYTERANGE:").unwrap_or_default();
                if let Some(range) = parse_byterange_attr(value, last_byterange_end) {
                    last_byterange_end = Some(range.0 + range.1);
                    pending_byterange = Some(range);
                }
            }
            // All other tags (#EXT-X-DISCONTINUITY, #EXT-X-TARGETDURATION, …)
            // are intentionally ignored.
        } else {
            // Any non-# line is a media segment — no extension assumptions.
            let resolved_segment_url = base_url.join(line)?.to_string();
            segments.push(Segment {
                url: resolved_segment_url,
                byte_range: pending_byterange.take(),
            });
            if let Some(dur) = current_segment_duration.take() {
                total_duration += dur;
            }
        }
    }

    if total_duration == 0.0 && !segments.is_empty() {
        total_duration = segments.len() as f64 * 6.0;
    }

    Ok(M3u8Info {
        segments,
        init_segment,
        key_url,
        iv,
        total_duration,
    })
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

        // Transient network errors / CF throttling: retry a few times before
        // giving up on the playlist.
        let mut last_err: Option<String> = None;
        let mut text = String::new();
        for attempt in 1..=3 {
            let mut req = client.get(m3u8_url);
            req = apply_referer(req, original_url);
            req = apply_cf_headers_safe(req, m3u8_url, original_url, cf_clearance, user_agent);
            match req.send().await {
                Ok(resp) => {
                    if !resp.status().is_success() {
                        last_err = Some(format!(
                            "M3U8 playlist request failed: HTTP {} (attempt {}/3)",
                            resp.status(),
                            attempt
                        ));
                    } else {
                        match resp.text().await {
                            Ok(t) => {
                                text = t;
                                last_err = None;
                                break;
                            }
                            Err(e) => last_err = Some(e.to_string()),
                        }
                    }
                }
                Err(e) => last_err = Some(e.to_string()),
            }
            tokio::time::sleep(std::time::Duration::from_millis(300 * attempt as u64)).await;
        }
        if let Some(err) = last_err {
            let boxed: Box<dyn Error> = err.into();
            return Err(boxed);
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Url {
        Url::parse("https://cdn.example.com/video/index.m3u8").unwrap()
    }

    #[test]
    fn parses_plain_ts_playlist() {
        let text = "#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:10\n\
#EXTINF:10.0,\nseg0.ts\n#EXTINF:9.5,\nsub/seg1.ts\n";
        let info = parse_media_m3u8_content(text, &base()).unwrap();
        assert_eq!(info.segments.len(), 2);
        assert_eq!(info.segments[0].url, "https://cdn.example.com/video/seg0.ts");
        assert_eq!(info.segments[1].url, "https://cdn.example.com/video/sub/seg1.ts");
        assert_eq!(info.segments[0].byte_range, None);
        assert!(info.init_segment.is_none());
        assert!(info.key_url.is_none());
        assert_eq!(info.total_duration, 19.5);
    }

    #[test]
    fn parses_aes_key_and_iv() {
        let text = "#EXTM3U\n#EXT-X-KEY:METHOD=AES-128,URI=\"key.bin\",IV=0x0000000000000000000000000000002a\n\
#EXTINF:10.0,\ns0.ts\n";
        let info = parse_media_m3u8_content(text, &base()).unwrap();
        assert_eq!(
            info.key_url.as_deref(),
            Some("https://cdn.example.com/video/key.bin")
        );
        let mut expected_iv = [0u8; 16];
        expected_iv[15] = 0x2a;
        assert_eq!(info.iv.as_deref(), Some(&expected_iv[..]));
    }

    #[test]
    fn parses_fmp4_with_map_and_byterange() {
        let text = "#EXTM3U\n#EXT-X-VERSION:7\n#EXT-X-TARGETDURATION:6\n\
#EXT-X-MAP:URI=\"init.mp4\",BYTERANGE=\"720@0\"\n\
#EXTINF:6.0,\nseg1.m4s\n\
#EXT-X-BYTERANGE:82112@752\n#EXTINF:6.0,\nseg2.m4s\n\
#EXTINF:6.0,\n/other/seg3.m4s\n\
#EXTINF:6.0,\n//other.example.com/x/seg4.m4s\n";
        let info = parse_media_m3u8_content(text, &base()).unwrap();
        let init = info.init_segment.expect("init segment parsed");
        assert_eq!(init.url, "https://cdn.example.com/video/init.mp4");
        assert_eq!(init.byte_range, Some((0, 720)));

        assert_eq!(info.segments.len(), 4);
        assert_eq!(info.segments[0].url, "https://cdn.example.com/video/seg1.m4s");
        assert_eq!(info.segments[0].byte_range, None);
        assert_eq!(info.segments[1].url, "https://cdn.example.com/video/seg2.m4s");
        assert_eq!(info.segments[1].byte_range, Some((752, 82112)));
        // root-relative resolves against the host
        assert_eq!(info.segments[2].url, "https://cdn.example.com/other/seg3.m4s");
        // protocol-relative resolves with the base scheme
        assert_eq!(info.segments[3].url, "https://other.example.com/x/seg4.m4s");
    }

    #[test]
    fn byterange_without_offset_continues_from_previous() {
        let text = "#EXTM3U\n#EXT-X-MAP:URI=\"init.mp4\",BYTERANGE=\"720@0\"\n\
#EXTINF:6.0,\nseg1.m4s\n\
#EXT-X-BYTERANGE:82112\n#EXTINF:6.0,\nseg2.m4s\n";
        let info = parse_media_m3u8_content(text, &base()).unwrap();
        assert_eq!(info.init_segment.unwrap().byte_range, Some((0, 720)));
        assert_eq!(info.segments[1].byte_range, Some((720, 82112)));
    }

    #[test]
    fn key_method_none_disables_encryption() {
        let text = "#EXTM3U\n#EXT-X-KEY:METHOD=AES-128,URI=\"key.bin\"\n\
#EXTINF:10.0,\ns0.ts\n\
#EXT-X-KEY:METHOD=NONE\n\
#EXTINF:10.0,\ns1.ts\n";
        let info = parse_media_m3u8_content(text, &base()).unwrap();
        assert!(info.key_url.is_none(), "later METHOD=NONE must clear the key");
        assert_eq!(info.segments.len(), 2);
    }

    #[test]
    fn segments_with_query_strings_and_no_extension() {
        let text = "#EXTM3U\n#EXTINF:6.0,\nseg1?token=abc&exp=1\n#EXTINF:6.0,\nclip\n";
        let info = parse_media_m3u8_content(text, &base()).unwrap();
        assert_eq!(info.segments.len(), 2);
        assert_eq!(
            info.segments[0].url,
            "https://cdn.example.com/video/seg1?token=abc&exp=1"
        );
        assert_eq!(info.segments[1].url, "https://cdn.example.com/video/clip");
    }

    #[test]
    fn key_attributes_in_any_order_and_bom() {
        // Attribute order is not guaranteed; also starts with a UTF-8 BOM.
        let text = "\u{feff}#EXTM3U\n#EXT-X-KEY:URI=\"key.bin\",METHOD=AES-128,KEYFORMAT=\"identity\"\n\
#EXTINF:10.0,\ns0.ts\n";
        let info = parse_media_m3u8_content(text, &base()).unwrap();
        assert_eq!(
            info.key_url.as_deref(),
            Some("https://cdn.example.com/video/key.bin")
        );
        assert_eq!(info.segments.len(), 1, "BOM line must not become a segment");
    }
}
