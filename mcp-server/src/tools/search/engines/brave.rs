use crate::types::SearchResult;
use scraper::{ElementRef, Html, Selector};

use super::{fetch_serp_html, EngineError};

fn normalize_href(href: &str) -> Option<String> {
    let href = href.trim();
    if href.is_empty() {
        return None;
    }

    if href.starts_with("http://") || href.starts_with("https://") {
        return Some(href.to_string());
    }

    None
}

fn extract_snippet(container: &ElementRef<'_>) -> String {
    let candidates = [
        "p.snippet-description",
        "div.snippet-description",
        "p",
        "div",
    ];
    for css in candidates {
        if let Ok(sel) = Selector::parse(css) {
            if let Some(n) = container.select(&sel).next() {
                let txt = n.text().collect::<Vec<_>>().join(" ");
                let trimmed = txt.split_whitespace().collect::<Vec<_>>().join(" ");
                if trimmed.len() >= 20 {
                    return trimmed;
                }
            }
        }
    }
    String::new()
}

pub fn parse_results(html: &str, max_results: usize) -> Vec<SearchResult> {
    let doc = Html::parse_document(html);

    // Brave SERP markup changes; prefer semantic patterns: anchors wrapping h3 under main.
    let main_sel = Selector::parse("main").unwrap();
    let a_sel = Selector::parse("a").unwrap();
    let h3_sel = Selector::parse("h3").unwrap();

    let mut out = Vec::new();
    let Some(main) = doc.select(&main_sel).next() else {
        return out;
    };

    // Attempt 1: anchors containing h3.
    for a in main.select(&a_sel) {
        if out.len() >= max_results {
            break;
        }
        if a.select(&h3_sel).next().is_none() {
            continue;
        }

        let href = a.value().attr("href").unwrap_or("");
        let Some(url) = normalize_href(href) else {
            continue;
        };

        let title = a
            .select(&h3_sel)
            .next()
            .map(|h| h.text().collect::<Vec<_>>().join(" "))
            .unwrap_or_default();
        let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
        if title.is_empty() {
            continue;
        }

        // Snippet: use parent container heuristics.
        let snippet_raw = extract_snippet(&a);
        let (published_prefix, snippet) = crate::tools::search::split_date_prefix(&snippet_raw);
        let published_at = published_prefix
            .or_else(|| crate::tools::search::extract_published_at_from_text(&snippet_raw));
        let breadcrumbs = crate::tools::search::breadcrumbs_from_url(&url);
        let (domain, source_type) = crate::tools::search::classify_search_result(&url);

        out.push(SearchResult {
            url,
            title,
            content: snippet,
            engine: Some("brave".to_string()),
            engine_source: Some("brave".to_string()),
            engine_sources: vec!["brave".to_string()],
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
    let mut url = reqwest::Url::parse("https://search.brave.com/search")
        .map_err(|e| EngineError::Fatal(e.to_string()))?;
    url.query_pairs_mut().append_pair("q", query);

    let (_status, body) = fetch_serp_html(client, url, "brave").await?;

    Ok(parse_results(&body, max_results))
}
