pub mod jable;
pub mod missav;
pub mod supjav;

use serde::{Deserialize, Serialize};

// static MIRRORS: phf::Map<&'static str, &'static [&'static str]> = phf_map! {
//     "missav" => &[
//         "missav.ai",
//         "missav.ws",
//         "missav123.com",
//         "missav.live",
//     ],
//     "jable" => &[
//         "jable.tv",
//         "fs1.app",
//     ],
//     "supjav" => &[
//         "supjav.com",
//     ],
// };

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoInfo {
    pub title: String,
    pub url: String,
    pub image_url: String,
    pub duration: Option<String>,
    pub preview_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Category {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoListResponse {
    pub videos: Vec<VideoInfo>,
    pub total_pages: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagItem {
    pub name: String,
    pub slug: String,
    pub url: String,
}

pub fn apply_cf_headers(
    mut req: wreq::RequestBuilder,
    cf_clearance: &str,
    user_agent: &str,
) -> wreq::RequestBuilder {
    let ua = user_agent.trim();
    if !ua.is_empty() {
        req = req.header("user-agent", ua);
    }

    let cf = cf_clearance.trim();
    if !cf.is_empty() {
        let clean_cf = if cf.contains("cf_clearance=") {
            let re = regex::Regex::new(r#"cf_clearance=([^;,\s]+)"#).unwrap();
            if let Some(caps) = re.captures(cf) {
                caps.get(1)
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_else(|| cf.to_string())
            } else {
                cf.to_string()
            }
        } else {
            cf.to_string()
        };
        let clean_val = clean_cf.trim().trim_matches('"');
        req = req.header("cookie", &format!("cf_clearance={}", clean_val));
    }
    req
}

pub fn get_cf_headers_for_url(
    configs: &std::collections::HashMap<String, crate::commands::CfConfig>,
    target_url: &str,
) -> (String, String) {
    if let Ok(parsed) = url::Url::parse(target_url) {
        if let Some(host) = parsed.host_str() {
            for (domain, cfg) in configs.iter() {
                if host.contains(domain) {
                    return (cfg.cf_clearance.clone(), cfg.user_agent.clone());
                }
            }
        }
    }
    ("".to_string(), "".to_string())
}

pub fn get_accept_language_header(lang: &str) -> &'static str {
    let l = lang.trim().to_lowercase();
    match l.as_str() {
        "en" | "en-us" => "en-US,en;q=0.9",
        "ja" | "jp" | "ja-jp" => "ja-JP,ja;q=0.9,en;q=0.8",
        "cn" | "zh-cn" => "zh-CN,zh;q=0.9,en;q=0.8",
        "zh" | "zh-tw" | "" => "zh-TW,zh;q=0.9,en-US;q=0.8,en;q=0.7",
        _ => "zh-TW,zh;q=0.9,en-US;q=0.8,en;q=0.7",
    }
}
