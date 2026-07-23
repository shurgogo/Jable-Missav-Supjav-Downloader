use crate::scraper::{Category, TagItem, VideoInfo, VideoListResponse};
use dom_query::Document;
use std::collections::HashMap;
use std::error::Error;
use wreq::Client;

pub fn with_lang(url: &str, lang: &str) -> String {
    if lang.is_empty() {
        return url.to_string();
    }
    if url.contains("lang=") {
        return url.to_string();
    }
    let mut url_parsed = match url::Url::parse(url) {
        Ok(u) => u,
        Err(_) => return url.to_string(),
    };
    url_parsed.query_pairs_mut().append_pair("lang", lang);
    url_parsed.to_string()
}

// Fetch dynamic categories merged with homepage sections
pub async fn get_categories(client: &Client, lang: &str) -> Vec<Category> {
    let mut cats = vec![
        Category {
            name: "最近更新".to_string(),
            url: with_lang("https://jable.tv/latest-updates/", lang),
        },
        Category {
            name: "熱門影片".to_string(),
            url: with_lang("https://jable.tv/hot/", lang),
        },
        Category {
            name: "新片上架".to_string(),
            url: with_lang("https://jable.tv/new-release/", lang),
        },
    ];

    let fetch_url = with_lang("https://jable.tv/categories/", lang);
    println!("[Scraper] Fetching dynamic categories from: {}", fetch_url);
    let resp = match client.get(&fetch_url)
        .header("accept", "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8")
        .header("accept-language", "zh-TW,zh;q=0.9,en-US;q=0.8,en;q=0.7")
        .header("referer", "https://jable.tv/")
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            println!("[Scraper] Failed to fetch dynamic categories: {}", e);
            return cats;
        }
    };

    if !resp.status().is_success() {
        return cats;
    }

    let html = match resp.text().await {
        Ok(t) => t,
        Err(_) => return cats,
    };

    let doc = Document::from(html.as_str());
    let anchors = doc.select("a[href*=\"/categories/\"]");
    let re_count = regex::Regex::new(r#"\d[\d,]*\s*部影片"#).unwrap();

    for node in anchors.iter() {
        let href = node.attr("href").map(|v| v.to_string()).unwrap_or_default();
        let text = node.text().to_string();
        let text_trimmed = text.trim();

        if href.contains("/categories/")
            && href != "https://jable.tv/categories/"
            && !text_trimmed.is_empty()
        {
            let name_clean = re_count.replace_all(text_trimmed, "").trim().to_string();
            if !name_clean.is_empty() {
                cats.push(Category {
                    name: name_clean,
                    url: with_lang(&href, lang),
                });
            }
        }
    }

    println!("[Scraper] Loaded {} categories", cats.len());
    cats
}

// Sidebar tags helper from Python source definition
pub fn get_sidebar_tags() -> HashMap<String, Vec<TagItem>> {
    let mut groups = HashMap::new();

    let raw_tags = vec![
        (
            "衣著",
            vec![
                ("黑絲", "black-pantyhose"),
                ("過膝襪", "knee-socks"),
                ("運動裝", "sportswear"),
                ("肉絲", "flesh-toned-pantyhose"),
                ("絲襪", "pantyhose"),
                ("眼鏡娘", "glasses"),
                ("獸耳", "kemonomimi"),
                ("漁網", "fishnets"),
                ("水着", "swimsuit"),
                ("校服", "school-uniform"),
                ("旗袍", "cheongsam"),
                ("婚紗", "wedding-dress"),
                ("女僕", "maid"),
                ("和服", "kimono"),
                ("吊帶襪", "stockings"),
                ("兔女郎", "bunny-girl"),
                ("Cosplay", "Cosplay"),
            ],
        ),
        (
            "身材",
            vec![
                ("黑肉", "suntan"),
                ("長身", "tall"),
                ("軟體", "flexible-body"),
                ("貧乳", "small-tits"),
                ("美腿", "beautiful-leg"),
                ("美尻", "beautiful-butt"),
                ("紋身", "tattoo"),
                ("短髮", "short-hair"),
                ("白虎", "hairless-pussy"),
                ("熟女", "mature-woman"),
                ("巨乳", "big-tits"),
                ("少女", "girl"),
                ("嬌小", "dainty"),
            ],
        ),
        (
            "交合",
            vec![
                ("顏射", "facial"),
                ("腳交", "footjob"),
                ("肛交", "anal-sex"),
                ("痙攣", "spasms"),
                ("潮吹", "squirting"),
                ("深喉", "deep-throat"),
                ("接吻", "kiss"),
                ("口爆", "cum-in-mouth"),
                ("口交", "blowjob"),
                ("乳交", "tit-wank"),
                ("中出", "creampie"),
            ],
        ),
        (
            "玩法",
            vec![
                ("露出", "outdoor"),
                ("集團進犯", "gang-intrusion"),
                ("進犯", "intrusion"),
                ("調教", "tune"),
                ("綑綁", "bondage"),
                ("瞬間插入", "quickie"),
                ("痴漢", "chikan"),
                ("痴女", "chizyo"),
                ("男M", "masochism-guy"),
                ("泥醉", "crapulence"),
                ("泡姬", "soapland"),
                ("母乳", "breast-milk"),
                ("放尿", "piss"),
                ("按摩", "massage"),
                ("多P", "groupsex"),
                ("刑具", "grip"),
                ("凌辱", "insult"),
                ("一日十回", "10-times-a-day"),
                ("3P", "3p"),
            ],
        ),
        (
            "劇情",
            vec![
                ("黑人", "black"),
                ("醜男", "ugly-man"),
                ("誘惑", "temptation"),
                ("親屬", "kinship"),
                ("童貞", "virginity"),
                ("時間停止", "time-stop"),
                ("復仇", "avenge"),
                ("年齡差", "age-difference"),
                ("巨漢", "giant"),
                ("媚藥", "love-potion"),
                ("夫目前犯", "sex-beside-husband"),
                ("出軌", "affair"),
                ("催眠", "hypnosis"),
                ("偷拍", "private-cam"),
                ("下雨天", "rainy-day"),
                ("NTR", "ntr"),
            ],
        ),
        (
            "角色",
            vec![
                ("風俗娘", "club-hostess-and-sex-worker"),
                ("醫生", "doctor"),
                ("逃犯", "fugitive"),
                ("護士", "nurse"),
                ("老師", "teacher"),
                ("空姐", "flight-attendant"),
                ("球隊經理", "team-manager"),
                ("未亡人", "widow"),
                ("搜查官", "detective"),
                ("情侶", "couple"),
                ("家政婦", "housewife"),
                ("家庭教師", "private-teacher"),
                ("偶像", "idol"),
                ("人妻", "wife"),
                ("主播", "female-anchor"),
                ("OL", "ol"),
            ],
        ),
        (
            "地點",
            vec![
                ("魔鏡號", "magic-mirror"),
                ("電車", "tram"),
                ("處女", "first-night"),
                ("監獄", "prison"),
                ("溫泉", "hot-spring"),
                ("洗浴場", "bathing-place"),
                ("泳池", "swimming-pool"),
                ("汽車", "car"),
                ("廁所", "toilet"),
                ("學校", "school"),
                ("圖書館", "library"),
                ("健身房", "gym-room"),
                ("便利店", "store"),
            ],
        ),
        (
            "雜項",
            vec![
                ("錄像", "video-recording"),
                ("處女作/引退作", "debut-retires"),
                ("綜藝", "variety-show"),
                ("節日主題", "festival"),
                ("感謝祭", "thanksgiving"),
                ("4小時以上", "more-than-4-hours"),
            ],
        ),
    ];

    for (group_name, tags) in raw_tags {
        let tag_items = tags
            .into_iter()
            .map(|(name, slug)| TagItem {
                name: name.to_string(),
                slug: slug.to_string(),
                url: format!("https://jable.tv/tags/{}/", slug),
            })
            .collect();
        groups.insert(group_name.to_string(), tag_items);
    }

    groups
}

// Fetch list with pagination and sorting
pub async fn fetch_list(
    client: &Client,
    base_url: &str,
    page: usize,
    sort_by: Option<String>,
    lang: &str,
    cf_clearance: &str,
    user_agent: &str,
) -> Result<VideoListResponse, Box<dyn Error>> {
    let mut url_parsed = url::Url::parse(base_url)?;

    // Set "from" query parameter (JableTV pagination is 1-indexed)
    url_parsed
        .query_pairs_mut()
        .append_pair("from", &page.to_string());

    if let Some(ref sort) = sort_by {
        if !sort.is_empty() {
            url_parsed.query_pairs_mut().append_pair("sort_by", sort);
        }
    }

    let final_url = url_parsed.to_string();
    println!("[Scraper] Fetching list from final URL: {}", final_url);

    let mut req = client.get(&final_url)
        .header("accept", "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8")
        .header("accept-language", "zh-TW,zh;q=0.9,en-US;q=0.8,en;q=0.7")
        .header("referer", "https://jable.tv/");

    req = crate::scraper::apply_cf_headers(req, cf_clearance, user_agent);
    let resp = req.send().await?;

    let status = resp.status();
    println!("[Scraper] Response status: {}", status);

    if !status.is_success() {
        return Err(format!("HTTP Error: {} - Failed to access site", status).into());
    }

    let html = resp.text().await?;

    if html.contains("Just a moment...")
        || html.contains("cf-challenge")
        || (html.contains("cloudflare") && html.contains("checking your browser"))
    {
        println!("[Scraper] Blocked by Cloudflare (Challenge Page detected)");
        return Err(
            "遭到 Cloudflare 安全驗證阻擋，請嘗試使用 VPN 或在瀏覽器中開啟一次該網站。".into(),
        );
    }

    let doc = Document::from(html.as_str());

    // Parse video count and total pages
    let mut total_pages = 1;
    let title_box = doc.select("div.title-box");
    if title_box.length() > 0 {
        let span_text = title_box.select("span").text().to_string();
        let re_num = regex::Regex::new(r#"([\d,]+)"#).unwrap();
        if let Some(caps) = re_num.captures(&span_text) {
            if let Some(m) = caps.get(1) {
                let num_str = m.as_str().replace(",", "");
                if let Ok(total_links) = num_str.parse::<usize>() {
                    total_pages = (total_links + 23) / 24;
                    println!(
                        "[Scraper] Total videos: {}, parsed total pages: {}",
                        total_links, total_pages
                    );
                }
            }
        }
    } else {
        // Fallback: parse pagination buttons
        let mut max_p = 1;
        let pagination_links = doc.select("ul.pagination a.page-link");
        let re_from = regex::Regex::new(r#"from=(\d+)"#).unwrap();
        for node in pagination_links.iter() {
            let href = node.attr("href").unwrap_or_default();
            if let Some(caps) = re_from.captures(&href) {
                if let Some(m) = caps.get(1) {
                    if let Ok(p) = m.as_str().parse::<usize>() {
                        if p > max_p {
                            max_p = p;
                        }
                    }
                }
            }
            let text_val = node.text().to_string();
            if let Ok(p) = text_val.trim().parse::<usize>() {
                if p > max_p {
                    max_p = p;
                }
            }
        }
        total_pages = max_p;
        println!(
            "[Scraper] Title box not found. Parsed total pages from pagination links: {}",
            total_pages
        );
    }

    if total_pages < page {
        total_pages = page;
    }

    let mut videos = Vec::new();
    let cards = doc.select("div.video-img-box");
    println!("[Scraper] Found {} video cards", cards.length());

    for node in cards.iter() {
        let detail = node.select("div.detail");
        let tag_a = detail.select("h6 a");

        let url_val = tag_a
            .attr("href")
            .map(|v| v.to_string())
            .unwrap_or_default();
        let title_val = tag_a.text().to_string().trim().to_string();

        if url_val.is_empty() || title_val.is_empty() {
            continue;
        }

        let img = node.select("img");
        let mut img_url = img
            .attr("data-src")
            .or_else(|| img.attr("src"))
            .map(|v| v.to_string())
            .unwrap_or_default();

        if !img_url.is_empty() && !img_url.starts_with("http") {
            img_url = format!("https://jable.tv{}", img_url);
        }

        let mut preview_url = img
            .attr("data-preview")
            .map(|v| v.to_string())
            .filter(|v| !v.is_empty());

        if let Some(ref mut p_url) = preview_url {
            if p_url.starts_with("//") {
                *p_url = format!("https:{}", p_url);
            } else if p_url.starts_with('/') {
                *p_url = format!("https://jable.tv{}", p_url);
            }
        }

        let duration_text = node
            .select("span.label")
            .text()
            .to_string()
            .trim()
            .to_string();
        let duration_opt = if duration_text.is_empty() {
            None
        } else {
            Some(duration_text)
        };

        videos.push(VideoInfo {
            title: title_val,
            url: with_lang(&url_val, lang),
            image_url: img_url,
            duration: duration_opt,
            preview_url,
        });
    }

    Ok(VideoListResponse {
        videos,
        total_pages,
    })
}

pub async fn search_videos(
    client: &Client,
    keyword: &str,
    page: usize,
    sort_by: Option<String>,
    lang: &str,
    cf_clearance: &str,
    user_agent: &str,
) -> Result<VideoListResponse, Box<dyn Error>> {
    let encoded_keyword =
        url::form_urlencoded::byte_serialize(keyword.as_bytes()).collect::<String>();
    let base = with_lang(
        &format!("https://jable.tv/search/?q={}", encoded_keyword),
        lang,
    );
    fetch_list(client, &base, page, sort_by, lang, cf_clearance, user_agent).await
}
