use super::FetchOptions;
use crate::scraper::{Category, VideoInfo, VideoListResponse};
use dom_query::Document;
use std::error::Error;
use url::Url;

pub async fn get_categories(lang: &str) -> Vec<Category> {
    let clean_lang = lang.trim().trim_matches('/');
    let prefix = if clean_lang.is_empty() {
        "https://supjav.com".to_string()
    } else {
        format!("https://supjav.com/{}", clean_lang)
    };

    let raw_cats = vec![
        ("最近更新", format!("{}/", prefix)),
        ("本週熱門", format!("{}/popular?sort=week", prefix)),
        ("本月熱門", format!("{}/popular?sort=month", prefix)),
        ("無碼", format!("{}/category/uncensored-jav", prefix)),
        ("有碼", format!("{}/category/censored-jav", prefix)),
        ("素人", format!("{}/category/amateur", prefix)),
        ("中文字幕", format!("{}/category/chinese-subtitles", prefix)),
        ("英文字幕", format!("{}/category/english-subtitles", prefix)),
        ("破壞版", format!("{}/category/reducing-mosaic", prefix)),
    ];

    raw_cats
        .into_iter()
        .map(|(name, url)| Category {
            name: name.to_string(),
            url,
        })
        .collect()
}

pub fn build_supjav_page_url(base_url: &str, page: usize) -> String {
    if page <= 1 {
        return base_url.to_string();
    }
    
    if base_url.contains("?s=") || base_url.contains("&s=") {
        let parts: Vec<&str> = base_url.splitn(2, '?').collect();
        let root = parts[0].trim_end_matches('/');
        let qs = parts.get(1).unwrap_or(&"");
        return format!("{}/page/{}/?{}", root, page, qs);
    }
    
    if base_url.contains('?') {
        return format!("{}&page={}", base_url, page);
    }
    
    format!("{}/page/{}", base_url.trim_end_matches('/'), page)
}

pub async fn fetch_list(
    opts: &FetchOptions<'_>,
) -> Result<VideoListResponse, Box<dyn Error>> {
    println!("[SupJav Scraper] Fetching list from: {}", opts.target);

    let final_url = build_supjav_page_url(opts.target, opts.page);
    println!("[SupJav Scraper] Fetching list from final URL: {}", final_url);

    let accept_lang = crate::scraper::get_accept_language_header(opts.lang);
    let mut req = opts
        .client
        .get(&final_url)
        .header("accept", "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8")
        .header("accept-language", accept_lang)
        .header("referer", "https://supjav.com/");
        
    req = crate::scraper::apply_cf_headers(req, opts.cf_clearance, opts.user_agent);
    let resp = req.send().await?;

    let status = resp.status();
    println!("[SupJav Scraper] Response status: {}", status);

    if !status.is_success() {
        return Err(format!("HTTP Error: {} - Failed to access site", status).into());
    }

    let html = resp.text().await?;
    
    if html.contains("Just a moment...") || html.contains("cf-challenge") || (html.contains("cloudflare") && html.contains("checking your browser")) {
        println!("[SupJav Scraper] Blocked by Cloudflare (Challenge Page detected)");
        return Err("遭到 Cloudflare 安全驗證阻擋，請嘗試使用 VPN 或稍後重試。".into());
    }

    let doc = Document::from(html.as_str());

    // Parse total pages from pagination links
    let mut total_pages = 1;
    let pagination_links = doc.select("a[href*=\"/page/\"], a.page-numbers");
    let re_page = regex::Regex::new(r#"/page/(\d+)"#).unwrap();
    
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
    
    if total_pages < opts.page {
        total_pages = opts.page;
    }

    let mut videos = Vec::new();
    let cards = doc.select("div.post");
    println!("[SupJav Scraper] Found {} post cards", cards.length());

    for node in cards.iter() {
        let link_node = node.select("a[href*=\".html\"]");
        if link_node.length() == 0 {
            continue;
        }

        let href_val = link_node.attr("href").map(|v| v.to_string()).unwrap_or_default();
        if href_val.is_empty() {
            continue;
        }

        // Must resolve to absolute url
        let video_url = if href_val.starts_with("http") {
            href_val
        } else {
            Url::parse("https://supjav.com/")?.join(&href_val)?.to_string()
        };

        if videos.iter().any(|v: &VideoInfo| v.url == video_url) {
            continue;
        }

        let mut title_val = link_node.attr("title").map(|v| v.to_string()).unwrap_or_default();
        if title_val.is_empty() {
            title_val = link_node.text().to_string().trim().to_string();
        }
        
        let img_node = node.select("img");
        let mut img_url = img_node
            .attr("data-original")
            .or_else(|| img_node.attr("data-src"))
            .or_else(|| img_node.attr("src"))
            .map(|v| v.to_string())
            .unwrap_or_default();

        if img_url.starts_with("data:") {
            img_url = "".to_string();
        }

        if !img_url.is_empty() && !img_url.starts_with("http") {
            img_url = format!("https://supjav.com{}", img_url);
        }

        if title_val.is_empty() {
            continue;
        }

        videos.push(VideoInfo {
            title: title_val,
            url: video_url,
            image_url: img_url,
            duration: None, // SupJav lists do not explicitly show durations on cover grids
            preview_url: None,
        });
    }

    println!("[SupJav Scraper] Parsed {} valid video listings", videos.len());
    Ok(VideoListResponse { videos, total_pages })
}

pub async fn search_videos(
    opts: &FetchOptions<'_>,
) -> Result<VideoListResponse, Box<dyn Error>> {
    let encoded_keyword = url::form_urlencoded::byte_serialize(opts.target.as_bytes()).collect::<String>();
    let clean_lang = opts.lang.trim().trim_matches('/');
    let base = if clean_lang.is_empty() {
        "https://supjav.com/".to_string()
    } else {
        format!("https://supjav.com/{}/", clean_lang)
    };
    let url = format!("{}?s={}", base, encoded_keyword);
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

pub async fn discover_working_domain(client: &wreq::Client) -> String {
    let mirrors = crate::scraper::get_mirror_domains(crate::scraper::Site::Supjav);
    crate::scraper::discover_working_domain(client, mirrors).await
}
