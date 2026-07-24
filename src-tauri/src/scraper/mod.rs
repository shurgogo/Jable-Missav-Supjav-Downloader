pub mod jable;
pub mod missav;
pub mod supjav;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Site {
    Jable,
    Missav,
    Supjav,
}

impl std::fmt::Display for Site {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Site::Jable => write!(f, "jable"),
            Site::Missav => write!(f, "missav"),
            Site::Supjav => write!(f, "supjav"),
        }
    }
}

pub fn get_mirror_domains(site: Site) -> &'static [&'static str] {
    match site {
        Site::Missav => &[
            "https://missav.ws",
            "https://missav.ai",
            "https://missav123.com",
            "https://missav.live",
        ],
        Site::Jable => &["https://jable.tv", "https://fs1.app"],
        Site::Supjav => &["https://supjav.com"],
    }
}

pub async fn discover_working_domain(
    client: &wreq::Client,
    candidate_domains: &[&str],
) -> String {
    if candidate_domains.is_empty() {
        return "".to_string();
    }

    for &domain in candidate_domains {
        let formatted = if !domain.starts_with("http://") && !domain.starts_with("https://") {
            format!("https://{}", domain)
        } else {
            domain.to_string()
        };

        println!("[Scraper] Checking domain: {}", formatted);
        let req = client
            .get(&formatted)
            .header(
                "accept",
                "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8",
            )
            .header("accept-language", "zh-TW,zh;q=0.9,en-US;q=0.8,en;q=0.7")
            .header("referer", &format!("{}/", formatted));

        match req.send().await {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() || status.as_u16() == 403 {
                    println!("[Scraper] Found working domain: {}", formatted);
                    return formatted;
                }
            }
            Err(e) => {
                println!("[Scraper] Domain {} failed check: {}", formatted, e);
            }
        }
    }

    let fallback = candidate_domains[0];
    if !fallback.starts_with("http://") && !fallback.starts_with("https://") {
        format!("https://{}", fallback)
    } else {
        fallback.to_string()
    }
}

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

pub struct FetchOptions<'a> {
    pub client: &'a wreq::Client,
    pub target: &'a str,
    pub page: usize,
    pub sort_by: Option<&'a str>,
    pub lang: &'a str,
    pub cf_clearance: &'a str,
    pub user_agent: &'a str,
}

pub fn append_query_params(base_url: &str, params: &[(&str, &str)]) -> String {
    if params.is_empty() {
        return base_url.to_string();
    }
    match url::Url::parse(base_url) {
        Ok(mut u) => {
            {
                let mut pairs = u.query_pairs_mut();
                for (k, v) in params {
                    if !v.is_empty() {
                        pairs.append_pair(k, v);
                    }
                }
            }
            u.to_string()
        }
        Err(_) => base_url.to_string(),
    }
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

pub fn get_cf_headers_for_site(
    configs: &std::collections::HashMap<String, crate::commands::CfConfig>,
    site: Site,
) -> (String, String) {
    let site_str = site.to_string();
    for (domain, cfg) in configs.iter() {
        if domain.to_lowercase().contains(&site_str) {
            return (cfg.cf_clearance.clone(), cfg.user_agent.clone());
        }
    }
    ("".to_string(), "".to_string())
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
