use super::FetchOptions;
use crate::scraper::{Category, VideoInfo, VideoListResponse};
use dom_query::Document;
use std::error::Error;
use std::sync::Mutex;
use std::sync::OnceLock;
use wreq::Client;
use url::Url;

static ACTIVE_DOMAIN: OnceLock<Mutex<String>> = OnceLock::new();

pub fn get_active_domain() -> String {
    let mutex = ACTIVE_DOMAIN.get_or_init(|| Mutex::new("https://missav.ws".to_string()));
    let domain = mutex.lock().unwrap().clone();
    domain
}

pub fn set_active_domain(domain: &str) {
    let mutex = ACTIVE_DOMAIN.get_or_init(|| Mutex::new("https://missav.ws".to_string()));
    let mut guard = mutex.lock().unwrap();
    *guard = domain.to_string();
}

pub async fn discover_working_domain(client: &wreq::Client) -> String {
    let mirrors = crate::scraper::get_mirror_domains(crate::scraper::Site::Missav);
    crate::scraper::discover_working_domain(client, mirrors).await
}

fn get_fallback_categories(working_domain: &str, lang: &str) -> Vec<Category> {
    let raw_cats = vec![
        ("今日熱門", "dm296/today-hot"),
        ("本週熱門", "dm170/weekly-hot"),
        ("本月熱門", "dm266/monthly-hot"),
        ("中文字幕", "dm278/chinese-subtitle"),
        ("最近更新", "dm539/new"),
        ("新作上市", "dm632/release"),
        ("無碼流出", "dm816/uncensored-leak"),
        ("SIRO", "dm36/siro"),
        ("FC2", "dm473/fc2"),
        ("麻豆傳媒", "dm63/madou"),
        ("東京熱", "dm42/tokyohot"),
        ("一本道", "dm4286298/1pondo"),
    ];

    let clean_lang = lang.trim().trim_matches('/');
    let mut list = Vec::new();
    for (name, path) in raw_cats {
        let final_url = if clean_lang.is_empty() {
            format!("{}/{}", working_domain, path)
        } else {
            let re = regex::Regex::new(r"^(dm\d+/)").unwrap();
            let new_path = re.replace(path, &format!("${{1}}{}/", clean_lang)).to_string();
            format!("{}/{}", working_domain, new_path)
        };

        list.push(Category {
            name: name.to_string(),
            url: final_url,
        });
    }
    list
}

pub async fn get_categories(client: &Client, lang: &str) -> Vec<Category> {
    // 1. Ensure active domain is resolved
    let current_domain = get_active_domain();
    let mut working = false;
    
    let req = client.get(&current_domain)
        .header("accept-language", "zh-TW,zh;q=0.9,en-US;q=0.8,en;q=0.7")
        .header("referer", "https://missav.ai/");
        
    if let Ok(resp) = req.send().await {
        let status = resp.status();
        if status.is_success() || status.as_u16() == 403 {
            working = true;
        }
    }

    if !working {
        let new_domain = discover_working_domain(client).await;
        set_active_domain(&new_domain);
    }

    let working_domain = get_active_domain();
    println!("[MissAV Scraper] Using active domain: {}", working_domain);

    // 2. Fetch categories from the homepage
    let clean_lang = lang.trim().trim_matches('/');
    let home_url = if clean_lang.is_empty() {
        working_domain.clone()
    } else {
        format!("{}/{}/", working_domain, clean_lang)
    };
    
    let accept_lang = crate::scraper::get_accept_language_header(lang);
    let mut cats = Vec::new();
    
    let resp = match client.get(&home_url)
        .header("accept", "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8")
        .header("accept-language", accept_lang)
        .header("referer", &home_url)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            println!("[MissAV Scraper] Failed to fetch homepage categories: {}", e);
            return get_fallback_categories(&working_domain, lang);
        }
    };

    if !resp.status().is_success() {
        return get_fallback_categories(&working_domain, lang);
    }

    let html = match resp.text().await {
        Ok(t) => t,
        Err(_) => return get_fallback_categories(&working_domain, lang),
    };

    let doc = Document::from(html.as_str());
    
    struct MatchRule {
        suffix: &'static str,
        title: &'static str,
    }

    let rules = vec![
        MatchRule { suffix: "today-hot", title: "今日熱門" },
        MatchRule { suffix: "weekly-hot", title: "本週熱門" },
        MatchRule { suffix: "monthly-hot", title: "本月熱門" },
        MatchRule { suffix: "chinese-subtitle", title: "中文字幕" },
        MatchRule { suffix: "new", title: "最近更新" },
        MatchRule { suffix: "release", title: "新作上市" },
        MatchRule { suffix: "uncensored-leak", title: "無碼流出" },
        MatchRule { suffix: "siro", title: "SIRO" },
        MatchRule { suffix: "fc2", title: "FC2" },
        MatchRule { suffix: "madou", title: "麻豆傳媒" },
        MatchRule { suffix: "tokyohot", title: "東京熱" },
        MatchRule { suffix: "1pondo", title: "一本道" },
    ];

    let links = doc.select("a");
    for rule in rules {
        let mut found_url = None;
        for node in links.iter() {
            if let Some(href) = node.attr("href") {
                let href_str = href.to_string();
                let matches = if rule.suffix == "new" {
                    href_str.ends_with("/new") || href_str.contains("/new?")
                } else if rule.suffix == "release" {
                    href_str.ends_with("/release") || href_str.contains("/release?")
                } else {
                    href_str.contains(rule.suffix)
                };

                if matches {
                    found_url = Some(href_str);
                    break;
                }
            }
        }

        if let Some(url_val) = found_url {
            let normalized = if url_val.starts_with("http") {
                if let Ok(parsed) = Url::parse(&url_val) {
                    format!("{}{}", working_domain, parsed.path())
                } else {
                    url_val
                }
            } else {
                format!("{}{}{}", working_domain, if url_val.starts_with('/') { "" } else { "/" }, url_val)
            };

            cats.push(Category {
                name: rule.title.to_string(),
                url: normalized,
            });
        }
    }

    if cats.is_empty() {
        return get_fallback_categories(&working_domain, lang);
    }

    cats
}

pub async fn fetch_list(
    opts: &FetchOptions<'_>,
) -> Result<VideoListResponse, Box<dyn Error>> {
    println!("[MissAV Scraper] Fetching list from: {}", opts.target);

    let page_str = opts.page.to_string();
    let mut params = vec![("page", page_str.as_str())];
    if !opts.lang.is_empty() {
        params.push(("lang", opts.lang));
    }

    let final_url = crate::scraper::append_query_params(opts.target, &params);

    let accept_lang = crate::scraper::get_accept_language_header(opts.lang);
    let working_domain = get_active_domain();
    let referer_val = format!("{}/", working_domain.trim_end_matches('/'));
    
    let mut req = opts
        .client
        .get(&final_url)
        .header("accept", "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8")
        .header("accept-language", accept_lang)
        .header("referer", &referer_val);
        
    req = crate::scraper::apply_cf_headers(req, opts.cf_clearance, opts.user_agent);
    let resp = req.send().await?;

    let status = resp.status();
    println!("[MissAV Scraper] Response status: {}", status);

    if !status.is_success() {
        return Err(format!("HTTP Error: {} - Failed to access site", status).into());
    }

    let html = resp.text().await?;
    
    if html.contains("Just a moment...") || html.contains("cf-challenge") || (html.contains("cloudflare") && html.contains("checking your browser")) {
        println!("[MissAV Scraper] Blocked by Cloudflare (Challenge Page detected)");
        return Err("遭到 Cloudflare 安全驗證阻擋，請嘗試使用 VPN 或稍後重試。".into());
    }

    let doc = Document::from(html.as_str());

    let mut total_pages = 1;
    let pagination_links = doc.select("a[href*=\"page=\"]");
    let re_page = regex::Regex::new(r#"page=(\d+)"#).unwrap();
    
    for node in pagination_links.iter() {
        let href = node.attr("href").unwrap_or_default();
        if let Some(caps) = re_page.captures(&href) {
            if let Some(m) = caps.get(1) {
                if let Ok(p) = m.as_str().parse::<usize>() {
                    if p > total_pages {
                        total_pages = p;
                    }
                }
            }
        }
        let text_val = node.text().to_string();
        if let Ok(p) = text_val.trim().parse::<usize>() {
            if p > total_pages {
                total_pages = p;
            }
        }
    }

    let mut videos = Vec::new();
    let re_digit = regex::Regex::new(r"\d").unwrap();
    let cards = doc.select("div.thumbnail, div.relative.aspect-w-16, div.space-y-2");

    for node in cards.iter() {
        let link_node = node.select("a[href]");
        if link_node.length() == 0 {
            continue;
        }

        let href_val = link_node.attr("href").map(|v| v.to_string()).unwrap_or_default();
        if href_val.is_empty() || href_val.contains("/search/") {
            continue;
        }

        let video_url = if href_val.starts_with("http") {
            href_val
        } else {
            Url::parse(&working_domain)?.join(&href_val)?.to_string()
        };

        let last_segment = video_url.trim_end_matches('/').rsplit('/').next().unwrap_or("").split('?').next().unwrap_or("");
        if !re_digit.is_match(last_segment) {
            continue;
        }

        if videos.iter().any(|v: &VideoInfo| v.url == video_url) {
            continue;
        }

        let img_node = node.select("img");
        let mut img_url = img_node
            .attr("data-src")
            .or_else(|| img_node.attr("src"))
            .map(|v| v.to_string())
            .unwrap_or_default();

        if !img_url.is_empty() && !img_url.starts_with("http") {
            img_url = format!("{}{}", working_domain, img_url);
        }

        let mut title_val = img_node.attr("alt").map(|v| v.to_string()).unwrap_or_default();
        let title_a = node.select("div.my-2 a, div.truncate a");
        if title_a.length() > 0 {
            let inner_text = title_a.text().to_string().trim().to_string();
            if !inner_text.is_empty() {
                title_val = inner_text;
            }
        }

        if title_val.is_empty() {
            continue;
        }

        let video_node = node.select("video.preview, video[data-src], video[src], video");
        let mut preview_url = video_node
            .attr("data-src")
            .or_else(|| video_node.attr("src"))
            .or_else(|| video_node.attr(":data-src"))
            .map(|v| v.to_string())
            .filter(|v| !v.is_empty() && !v.contains("javascript:;"));

        if let Some(ref mut p_url) = preview_url {
            if p_url.starts_with("//") {
                *p_url = format!("https:{}", p_url);
            } else if p_url.starts_with('/') {
                *p_url = format!("{}{}", working_domain, p_url);
            }
        }

        // Robust fallback: derive preview.mp4 from thumbnail image URL (works across all MissAV cards)
        if preview_url.is_none() && !img_url.is_empty() && img_url.starts_with("http") {
            if let Some(last_slash_idx) = img_url.rfind('/') {
                let base_dir = &img_url[..last_slash_idx];
                preview_url = Some(format!("{}/preview.mp4", base_dir));
            }
        }

        println!("[MissAV Scraper Debug] card title: '{}', img: '{}', preview: '{:?}'", title_val, img_url, preview_url);

        let duration_span = node.select("span.absolute.bottom-1.right-1");
        let duration_text = duration_span.text().to_string().trim().to_string();
        let duration_opt = if duration_text.is_empty() {
            None
        } else {
            Some(duration_text)
        };

        videos.push(VideoInfo {
            title: title_val,
            url: video_url,
            image_url: img_url,
            duration: duration_opt,
            preview_url,
        });
    }

    println!("[MissAV Scraper] Parsed {} valid video listings", videos.len());
    Ok(VideoListResponse { videos, total_pages })
}

pub async fn search_videos(
    opts: &FetchOptions<'_>,
) -> Result<VideoListResponse, Box<dyn Error>> {
    let encoded_keyword = url::form_urlencoded::byte_serialize(opts.target.as_bytes()).collect::<String>();
    let clean_lang = opts.lang.trim().trim_matches('/');
    let working_domain = get_active_domain();
    let url = if clean_lang.is_empty() {
        format!("{}/search/{}", working_domain, encoded_keyword)
    } else {
        format!("{}/{}/search/{}", working_domain, clean_lang, encoded_keyword)
    };
    let search_opts = FetchOptions {
        client: opts.client,
        target: &url,
        page: opts.page,
        sort_by: opts.sort_by,
        lang: opts.lang,
        cf_clearance: opts.cf_clearance,
        user_agent: opts.user_agent,
    };
    fetch_list(&search_opts).await
}
