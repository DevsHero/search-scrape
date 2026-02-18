use crate::types::SearchResult;
use scraper::{Html, Selector};

use super::{detect_block_reason, fetch_html, EngineError};

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
        let href = link.value().attr("href").unwrap_or("").to_string();
        if href.is_empty() {
            continue;
        }
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

    let (status, body) = fetch_html(client, url)
        .await
        .map_err(|e| EngineError::Transient(e.to_string()))?;

    if let Some(reason) = detect_block_reason(status, &body) {
        return Err(EngineError::Blocked { reason });
    }

    Ok(parse_results(&body, max_results))
}
