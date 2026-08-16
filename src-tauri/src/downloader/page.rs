use url::Url;
use wreq::Client;

use crate::scraper::Site;

use super::parsers::{parse_missav_page, parse_supjav_page};

/// Title + stream URL extracted from a video page.
pub struct PageInfo {
    pub title: String,
    pub m3u8_url: String,
}

impl From<super::parsers::JableVideoPageInfo> for PageInfo {
    fn from(info: super::parsers::JableVideoPageInfo) -> Self {
        PageInfo {
            title: info.title,
            m3u8_url: info.m3u8_url,
        }
    }
}

/// Fetch the video page for `site` and extract the stream URL + title.
/// The returned error string is already user-facing ("Failed to request …").
pub async fn fetch_page_info(
    client: &Client,
    site: Site,
    url: &str,
    cf_clearance: &str,
    user_agent: &str,
) -> Result<PageInfo, String> {
    match site {
        Site::Missav => fetch_missav_page(client, url, cf_clearance, user_agent).await,
        Site::Supjav => parse_supjav_page(client, url, cf_clearance, user_agent)
            .await
            .map(PageInfo::from)
            .map_err(|e| format!("Failed to parse SupJav page: {}", e)),
        Site::Jable => fetch_jable_page(client, url, cf_clearance, user_agent).await,
    }
}

async fn fetch_missav_page(
    client: &Client,
    url: &str,
    cf_clearance: &str,
    user_agent: &str,
) -> Result<PageInfo, String> {
    let referer_url = if let Ok(parsed_url) = Url::parse(url) {
        format!(
            "{}://{}/",
            parsed_url.scheme(),
            parsed_url.host_str().unwrap_or("missav.ai")
        )
    } else {
        "https://missav.ai/".to_string()
    };

    let mut req = client
        .get(url)
        .header(
            "accept",
            "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8",
        )
        .header("accept-language", "zh-TW,zh;q=0.9,en-US;q=0.8,en;q=0.7")
        .header("referer", &referer_url);
    req = crate::scraper::apply_cf_headers(req, cf_clearance, user_agent);

    let resp = req
        .send()
        .await
        .map_err(|e| format!("Failed to request MissAV page: {}", e))?;
    let text = resp
        .text()
        .await
        .map_err(|e| format!("Failed to read MissAV body: {}", e))?;
    parse_missav_page(&text)
        .map(PageInfo::from)
        .map_err(|e| format!("Failed to parse MissAV page: {}", e))
}

async fn fetch_jable_page(
    client: &Client,
    url: &str,
    cf_clearance: &str,
    user_agent: &str,
) -> Result<PageInfo, String> {
    let mut req = client.get(url).header("referer", "https://jable.tv/");
    req = crate::scraper::apply_cf_headers(req, cf_clearance, user_agent);

    let resp = req
        .send()
        .await
        .map_err(|e| format!("Failed to request JableTV page: {}", e))?;
    let html = resp
        .text()
        .await
        .map_err(|e| format!("Failed to read JableTV body: {}", e))?;

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
        return Err("Could not find JableTV M3U8 link in scripts".to_string());
    }

    Ok(PageInfo { title, m3u8_url })
}
