use crate::scraper::{Category, VideoInfo, VideoListResponse};
use dom_query::Document;
use std::error::Error;
use wreq::Client;
use url::Url;

pub fn with_lang(url: &str, lang: &str) -> String {
    let clean_lang = lang.trim().trim_matches('/');
    if clean_lang.is_empty() {
        return url.to_string();
    }
    let root = "https://supjav.com";
    let prefix = "https://supjav.com/";
    if url == root || url == prefix {
        return format!("{}/{}/", root, clean_lang);
    }
    if url.starts_with(prefix) {
        return format!("{}/{}/{}", root, clean_lang, &url[prefix.len()..]);
    }
    url.to_string()
}

pub async fn get_categories(lang: &str) -> Vec<Category> {
    let raw_cats = vec![
        ("最近更新", "https://supjav.com/"),
        ("本週熱門", "https://supjav.com/popular?sort=week"),
        ("本月熱門", "https://supjav.com/popular?sort=month"),
        ("無碼", "https://supjav.com/category/uncensored-jav"),
        ("有碼", "https://supjav.com/category/censored-jav"),
        ("素人", "https://supjav.com/category/amateur"),
        ("中文字幕", "https://supjav.com/category/chinese-subtitles"),
        ("英文字幕", "https://supjav.com/category/english-subtitles"),
        ("破壞版", "https://supjav.com/category/reducing-mosaic"),
    ];

    let mut list = Vec::new();
    for (name, url) in raw_cats {
        let mapped_url = with_lang(url, lang);
        list.push(Category {
            name: name.to_string(),
            url: mapped_url,
        });
    }
    list
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
    client: &Client,
    base_url: &str,
    page: usize,
    lang: &str,
    cf_clearance: &str,
    user_agent: &str,
) -> Result<VideoListResponse, Box<dyn Error>> {
    println!("[SupJav Scraper] Fetching list from: {}", base_url);

    let final_url = build_supjav_page_url(base_url, page);
    println!("[SupJav Scraper] Fetching list from final URL: {}", final_url);

    let accept_lang = crate::scraper::get_accept_language_header(lang);
    let mut req = client.get(&final_url)
        .header("accept", "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8")
        .header("accept-language", accept_lang)
        .header("referer", "https://supjav.com/");
        
    req = crate::scraper::apply_cf_headers(req, cf_clearance, user_agent);
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
    
    if total_pages < page {
        total_pages = page;
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
    client: &Client,
    keyword: &str,
    page: usize,
    lang: &str,
    cf_clearance: &str,
    user_agent: &str,
) -> Result<VideoListResponse, Box<dyn Error>> {
    let encoded_keyword = url::form_urlencoded::byte_serialize(keyword.as_bytes()).collect::<String>();
    let base = with_lang("https://supjav.com/", lang);
    let url = format!("{}?s={}", base, encoded_keyword);
    fetch_list(client, &url, page, lang, cf_clearance, user_agent).await
}
