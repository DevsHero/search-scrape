use crate::types::SearchResult;
use base64::Engine as _;
use scraper::{Html, Selector};

use super::{fetch_serp_html, EngineError};

fn normalize_bing_href(href: &str) -> Option<String> {
    let href = href.trim();
    if href.is_empty() {
        return None;
    }

    if !(href.starts_with("http://") || href.starts_with("https://")) {
        return None;
    }

    let Ok(url) = url::Url::parse(href) else {
        return Some(href.to_string());
    };

    if matches!(url.host_str(), Some("www.bing.com") | Some("bing.com"))
        && url.path().starts_with("/ck/")
    {
        for (k, v) in url.query_pairs() {
            if k == "u" && !v.trim().is_empty() {
                // Observed format: u=a1<base64(url)>
                let mut raw = v.to_string();
                if raw.starts_with("a1") {
                    raw = raw.trim_start_matches("a1").to_string();
                }

                if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(raw) {
                    if let Ok(decoded_str) = String::from_utf8(decoded) {
                        let decoded_str = decoded_str.trim().to_string();
                        if decoded_str.starts_with("http://") || decoded_str.starts_with("https://")
                        {
                            return Some(decoded_str);
                        }
                    }
                }

                // Fall back to original when decoding fails.
                break;
            }
        }
    }

    Some(href.to_string())
}

pub fn parse_results(html: &str, max_results: usize) -> Vec<SearchResult> {
    let doc = Html::parse_document(html);
    let sel_item = Selector::parse("li.b_algo").unwrap();
    let sel_link = Selector::parse("h2 a").unwrap();
    let sel_snip = Selector::parse("div.b_caption p").unwrap();
    let sel_fact = Selector::parse("div.b_factrow, div.b_vlist2col").unwrap();

    let mut out = Vec::new();
    for item in doc.select(&sel_item) {
        if out.len() >= max_results {
            break;
        }
        let link = match item.select(&sel_link).next() {
            Some(l) => l,
            None => continue,
        };
        let href_raw = link.value().attr("href").unwrap_or("").to_string();
        let Some(href) = normalize_bing_href(&href_raw) else {
            continue;
        };
        let title = link.text().collect::<Vec<_>>().join(" ");
        let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
        let snippet_raw = item
            .select(&sel_snip)
            .next()
            .map(|p| p.text().collect::<Vec<_>>().join(" "))
            .unwrap_or_default();
        let snippet_raw = snippet_raw.split_whitespace().collect::<Vec<_>>().join(" ");
        let (published_prefix, snippet) = crate::tools::search::split_date_prefix(&snippet_raw);

        let published_at = published_prefix
            .or_else(|| crate::tools::search::extract_published_at_from_text(&snippet_raw));
        let breadcrumbs = crate::tools::search::breadcrumbs_from_url(&href);
        let rich_snippet = item
            .select(&sel_fact)
            .next()
            .map(|n| n.text().collect::<Vec<_>>().join(" "))
            .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|s| !s.is_empty());

        let (domain, source_type) = crate::tools::search::classify_search_result(&href);
        out.push(SearchResult {
            url: href,
            title,
            content: snippet,
            engine: Some("bing".to_string()),
            engine_source: Some("bing".to_string()),
            engine_sources: vec!["bing".to_string()],
            score: None,
            published_at,
            breadcrumbs,
            rich_snippet,
            top_answer: None,
            domain,
            source_type: Some(source_type),
        });
    }

    out
}

pub async fn search(
    client: &reqwest::Client,
    query: &str,
    max_results: usize,
) -> Result<Vec<SearchResult>, EngineError> {
    let mut url = reqwest::Url::parse("https://www.bing.com/search")
        .map_err(|e| EngineError::Fatal(e.to_string()))?;
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("q", query);
    }

    let (_status, body) = fetch_serp_html(client, url, "bing").await?;

    Ok(parse_results(&body, max_results))
}
