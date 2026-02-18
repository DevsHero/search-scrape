use crate::types::SearchResult;
use scraper::{Html, Selector};

use super::{fetch_serp_html, EngineError};

fn normalize_ddg_href(href: &str) -> Option<String> {
    let href = href.trim();
    if href.is_empty() {
        return None;
    }

    // Protocol-relative URLs.
    let candidate = if href.starts_with("//") {
        format!("https:{}", href)
    } else if href.starts_with('/') {
        format!("https://duckduckgo.com{}", href)
    } else {
        href.to_string()
    };

    // If it's a DuckDuckGo redirect link, extract the real destination.
    if let Ok(url) = url::Url::parse(&candidate) {
        if matches!(url.host_str(), Some("duckduckgo.com")) && url.path().starts_with("/l/") {
            for (k, v) in url.query_pairs() {
                if k == "uddg" && !v.trim().is_empty() {
                    return Some(v.to_string());
                }
            }
        }
    }

    // Otherwise, accept absolute http(s) only.
    if candidate.starts_with("http://") || candidate.starts_with("https://") {
        return Some(candidate);
    }

    None
}

pub fn parse_results(html: &str, max_results: usize) -> Vec<SearchResult> {
    let doc = Html::parse_document(html);
    let sel_item = Selector::parse("div.results_links").unwrap();
    let sel_link = Selector::parse("a.result__a").unwrap();
    let sel_snip = Selector::parse("a.result__snippet, div.result__snippet").unwrap();

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
        let Some(href) = normalize_ddg_href(&href_raw) else {
            continue;
        };
        let title = link.text().collect::<Vec<_>>().join(" ");
        let title = title.split_whitespace().collect::<Vec<_>>().join(" ");

        let snippet_raw = item
            .select(&sel_snip)
            .next()
            .map(|n| n.text().collect::<Vec<_>>().join(" "))
            .unwrap_or_default();
        let snippet_raw = snippet_raw.split_whitespace().collect::<Vec<_>>().join(" ");
        let (published_prefix, snippet) = crate::tools::search::split_date_prefix(&snippet_raw);

        let published_at = published_prefix
            .or_else(|| crate::tools::search::extract_published_at_from_text(&snippet_raw));
        let breadcrumbs = crate::tools::search::breadcrumbs_from_url(&href);

        let (domain, source_type) = crate::tools::search::classify_search_result(&href);
        out.push(SearchResult {
            url: href,
            title,
            content: snippet,
            engine: Some("duckduckgo".to_string()),
            engine_source: Some("duckduckgo".to_string()),
            engine_sources: vec!["duckduckgo".to_string()],
            score: None,
            published_at,
            breadcrumbs,
            rich_snippet: None,
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
    let mut url = reqwest::Url::parse("https://duckduckgo.com/html/")
        .map_err(|e| EngineError::Fatal(e.to_string()))?;
    url.query_pairs_mut().append_pair("q", query);

    let (_status, body) = fetch_serp_html(client, url, "duckduckgo").await?;

    Ok(parse_results(&body, max_results))
}
