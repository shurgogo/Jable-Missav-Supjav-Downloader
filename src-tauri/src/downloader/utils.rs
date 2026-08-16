use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};
use tauri::Emitter;
use url::Url;

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

pub fn unpack_js_eval(script_text: &str) -> Option<String> {
    let re = regex::Regex::new(
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

    let re_word = regex::Regex::new(r"\b\w+\b").ok()?;
    let unpacked = re_word.replace_all(packed, |caps: &regex::Captures| {
        let word = &caps[0];
        lookup.get(word).cloned().unwrap_or_else(|| word.to_string())
    });

    Some(unpacked.to_string())
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
pub fn parse_hex(s: &str) -> Option<Vec<u8>> {
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
pub fn get_iv_for_segment(index: usize, custom_iv: &Option<Vec<u8>>) -> [u8; 16] {
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

/// Append an actionable hint when an error smells like a Cloudflare block, so
/// users are not left with a "mysterious" failure.
pub fn cloudflare_hint(err: &str) -> String {
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

/// Emit a `download-progress` event to the frontend.
pub fn emit_progress(
    window: &tauri::Window,
    url: &str,
    title: &str,
    index: usize,
    total: usize,
    speed_kbps: f64,
    status: &str,
) {
    let _ = window.emit(
        "download-progress",
        super::task::ProgressPayload {
            url: url.to_string(),
            title: title.to_string(),
            index,
            total,
            speed_kbps,
            status: status.to_string(),
        },
    );
}

/// Emit a failure event and drop the task's control entry so a retry /
/// re-download is not blocked by a stale "Running" state.
pub fn emit_failure(window: &tauri::Window, task_states: &super::task::TaskRegistry, url: &str, msg: &str) {
    emit_progress(window, url, "", 0, 0, 0.0, &format!("failed: {}", cloudflare_hint(msg)));
    let mut states = task_states.lock().unwrap();
    states.remove(url);
}

/// Sliding-window download speed estimator.
///
/// Samples are (timestamp, cumulative-bytes) pairs; the speed is the byte
/// delta across the whole window divided by the window's elapsed time. This
/// smooths out the bursty arrival of network chunks: instead of flipping
/// between 0 KB/s and a spike whenever a chunk lands, the UI shows the
/// average rate over the last `window` seconds.
pub struct SpeedTracker {
    samples: VecDeque<(Instant, usize)>,
    window: Duration,
}

impl SpeedTracker {
    pub fn new(window: Duration) -> Self {
        SpeedTracker {
            samples: VecDeque::new(),
            window,
        }
    }

    pub fn add_sample(&mut self, now: Instant, bytes: usize) {
        self.samples.push_back((now, bytes));
        // Drop samples older than the window.
        while let Some((t, _)) = self.samples.front() {
            if now.duration_since(*t) > self.window {
                self.samples.pop_front();
            } else {
                break;
            }
        }
    }

    /// Average download speed (KB/s) over the sliding window, 0.0 when there
    /// is not enough data yet.
    pub fn get_speed(&self) -> f64 {
        if self.samples.len() < 2 {
            return 0.0;
        }
        let (start_t, start_b) = self.samples.front().unwrap();
        let (end_t, end_b) = self.samples.back().unwrap();
        let dt = end_t.duration_since(*start_t).as_secs_f64();
        let db = end_b.saturating_sub(*start_b);
        if dt > 0.0 {
            (db as f64) / 1024.0 / dt
        } else {
            0.0
        }
    }

    /// Record a sample and return the current windowed speed in one call.
    pub fn sample(&mut self, now: Instant, bytes: usize) -> f64 {
        self.add_sample(now, bytes);
        self.get_speed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speed_tracker_averages_over_window() {
        // 1 秒内均匀下载 1MB（4 个采样）→ 窗口速度应稳定在 1024 KB/s，
        // 而不是按相邻采样点跳动。
        let mut st = SpeedTracker::new(Duration::from_secs(3));
        let t0 = Instant::now();
        let mut speed = 0.0;
        for i in 1..=4 {
            speed = st.sample(t0 + Duration::from_millis(i * 250), (i * 256 * 1024) as usize);
        }
        assert!(
            (speed - 1024.0).abs() < 1.0,
            "windowed speed should be ~1024 KB/s, got {}",
            speed
        );
    }

    #[test]
    fn speed_tracker_drops_old_samples() {
        let mut st = SpeedTracker::new(Duration::from_secs(3));
        let t0 = Instant::now();
        st.add_sample(t0, 0);
        st.add_sample(t0 + Duration::from_secs(1), 1024 * 1024);
        // 4s 时窗口为 (1s, 4s]，两个采样字节相同 → 速度为 0（无新增流量）
        st.add_sample(t0 + Duration::from_secs(4), 1024 * 1024);
        assert_eq!(st.get_speed(), 0.0);
        // 5s 时 2MB：窗口内 t=4s(1MB)→t=5s(2MB)，1MB/1s = 1024 KB/s
        st.add_sample(t0 + Duration::from_secs(5), 2 * 1024 * 1024);
        let s = st.get_speed();
        assert!(
            (s - 1024.0).abs() < 1.0,
            "expected ~1024 KB/s after new traffic, got {}",
            s
        );
    }
}
