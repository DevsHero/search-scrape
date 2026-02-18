use crate::types::SearchResult;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use scraper::{ElementRef, Html, Selector};

use super::{fetch_serp_html, EngineError};

fn normalize_google_href(href: &str) -> Option<String> {
    if href.is_empty() {
        return None;
    }

    if href.starts_with("/url?") {
        if let Ok(url) = reqwest::Url::parse(&format!("https://www.google.com{}", href)) {
            for (k, v) in url.query_pairs() {
                if k == "q" && !v.is_empty() {
                    return Some(v.to_string());
                }
            }
        }
        return None;
    }

    if href.starts_with("http://") || href.starts_with("https://") {
        return Some(href.to_string());
    }

    None
}

fn extract_snippet(container: &ElementRef<'_>) -> String {
    // Google markup changes often. We try a few common patterns.
    let candidates = ["div.VwiC3b", "div.IsZvec", "span.aCOpRe", "div.MUxGbd"];

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

fn extract_top_answer(doc: &Html) -> Option<String> {
    // Best-effort featured snippet / answer box extraction.
    // Markup changes often; try a few common containers.
    let selectors = [
        "div[data-attrid='wa:/description']",
        "div[data-attrid='wa:/short_answer']",
        "div.V3FYCf",
        "div.kno-rdesc",
    ];

    for css in selectors {
        if let Ok(sel) = Selector::parse(css) {
            if let Some(n) = doc.select(&sel).next() {
                let txt = n.text().collect::<Vec<_>>().join(" ");
                let cleaned = txt.split_whitespace().collect::<Vec<_>>().join(" ");
                if cleaned.len() >= 40 {
                    return Some(cleaned);
                }
            }
        }
    }

    None
}

pub fn parse_results(html: &str, max_results: usize) -> Vec<SearchResult> {
    let doc = Html::parse_document(html);

    let top_answer = extract_top_answer(&doc);

    let container_selectors = ["div#search div.MjjYud", "div#search div.g"];
    let link_sel = Selector::parse("a").unwrap();
    let h3_sel = Selector::parse("h3").unwrap();

    let mut out = Vec::new();
    'outer: for css in container_selectors {
        let Ok(container_sel) = Selector::parse(css) else {
            continue;
        };

        for container in doc.select(&container_sel) {
            if out.len() >= max_results {
                break 'outer;
            }

            let mut chosen: Option<(String, String)> = None;
            for a in container.select(&link_sel) {
                if a.select(&h3_sel).next().is_some() {
                    let href = a.value().attr("href").unwrap_or("");
                    let url = match normalize_google_href(href) {
                        Some(u) => u,
                        None => continue,
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
                    chosen = Some((url, title));
                    break;
                }
            }

            let Some((url, title)) = chosen else {
                continue;
            };

            if url.contains("google.com") {
                continue;
            }

            let snippet_raw = extract_snippet(&container);
            let (published_prefix, snippet) = crate::tools::search::split_date_prefix(&snippet_raw);
            let published_at = published_prefix
                .or_else(|| crate::tools::search::extract_published_at_from_text(&snippet_raw));
            let breadcrumbs = crate::tools::search::breadcrumbs_from_url(&url);
            let (domain, source_type) = crate::tools::search::classify_search_result(&url);

            // Attach top_answer only to the first organic result (best-effort).
            let top_answer_for_this = if out.is_empty() {
                top_answer.clone()
            } else {
                None
            };

            out.push(SearchResult {
                url,
                title,
                content: snippet,
                engine: Some("google".to_string()),
                engine_source: Some("google".to_string()),
                engine_sources: vec!["google".to_string()],
                score: None,
                published_at,
                breadcrumbs,
                rich_snippet: None,
                top_answer: top_answer_for_this,
                domain,
                source_type: Some(source_type),
            });
        }

        if !out.is_empty() {
            break;
        }
    }

    out
}

pub async fn search(
    client: &reqwest::Client,
    query: &str,
    max_results: usize,
) -> Result<Vec<SearchResult>, EngineError> {
    // Use a conservative, widely supported endpoint.
    let encoded = utf8_percent_encode(query, NON_ALPHANUMERIC).to_string();
    let url = reqwest::Url::parse(&format!(
        "https://www.google.com/search?q={}&hl=en&num={}",
        encoded,
        max_results.min(10).max(5)
    ))
    .map_err(|e| EngineError::Fatal(e.to_string()))?;

    let (_status, body) = fetch_serp_html(client, url, "google").await?;

    Ok(parse_results(&body, max_results))
}
