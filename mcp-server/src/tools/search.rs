use crate::types::*;
use crate::AppState;
use crate::query_rewriter::{QueryRewriter, QueryRewriteResult};
use crate::rerank::Reranker;
use anyhow::{anyhow, Result};
use backoff::future::retry;
use backoff::ExponentialBackoffBuilder;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};

#[derive(Debug, Default, Clone)]
pub struct SearchParamOverrides {
    pub engines: Option<String>,       // comma-separated list
    pub categories: Option<String>,    // comma-separated list
    pub language: Option<String>,      // e.g., "en" or "en-US"
    pub safesearch: Option<u8>,        // 0,1,2
    pub time_range: Option<String>,    // e.g., day, week, month, year
    pub pageno: Option<u32>,           // 1..N
}

#[derive(Debug, Default, Clone)]
pub struct SearchExtras {
    pub answers: Vec<String>,
    pub suggestions: Vec<String>,
    pub corrections: Vec<String>,
    pub unresponsive_engines: Vec<String>,
    pub query_rewrite: Option<QueryRewriteResult>,
    pub duplicate_warning: Option<String>,
}

pub async fn search_web(state: &Arc<AppState>, query: &str) -> Result<(Vec<SearchResult>, SearchExtras)> {
    search_web_with_params(state, query, None).await
}

pub async fn search_web_with_params(
    state: &Arc<AppState>,
    query: &str,
    overrides: Option<SearchParamOverrides>,
) -> Result<(Vec<SearchResult>, SearchExtras)> {
    info!("Searching for: {}", query);
    
    // Phase 2: Check for recent duplicates if memory enabled
    let mut duplicate_warning = None;
    if let Some(memory) = &state.memory {
        match memory.find_recent_duplicate(query, 6).await {
            Ok(Some((entry, score))) => {
                let time_ago = chrono::Utc::now().signed_duration_since(entry.timestamp);
                let hours = time_ago.num_hours();
                let minutes = time_ago.num_minutes();
                
                let time_str = if hours > 0 {
                    format!("{} hour{} ago", hours, if hours == 1 { "" } else { "s" })
                } else {
                    format!("{} minute{} ago", minutes, if minutes == 1 { "" } else { "s" })
                };
                
                duplicate_warning = Some(format!(
                    "⚠️ Similar search found from {} (similarity: {:.2}). Consider checking history first.",
                    time_str, score
                ));
                warn!("Duplicate search detected: {} ({} ago)", entry.query, time_str);
            }
            Ok(None) => {}
            Err(e) => warn!("Failed to check for duplicates: {}", e),
        }
    }
    
    // Phase 2: Query rewriting for developer queries
    let rewriter = QueryRewriter::new();
    let rewrite_result = rewriter.rewrite_query(query);
    
    let effective_query = if rewrite_result.was_rewritten() {
        info!("Query rewritten: '{}' -> '{}'", query, rewrite_result.best_query());
        rewrite_result.best_query()
    } else {
        query
    };
    
    let cache_key = if let Some(ref ov) = overrides {
        format!(
            "q={}|eng={}|cat={}|lang={}|safe={}|time={}|page={}",
            query,
            ov.engines.clone().unwrap_or_default(),
            ov.categories.clone().unwrap_or_default(),
            ov.language.clone().unwrap_or_default(),
            ov.safesearch.map(|v| v.to_string()).unwrap_or_default(),
            ov.time_range.clone().unwrap_or_default(),
            ov.pageno.map(|v| v.to_string()).unwrap_or_else(|| "1".into())
        )
    } else {
        format!("q={}|default", query)
    };

    // Note: We don't cache extras, only results, to keep cache simple
    // Extras are usually lightweight and context-dependent
    if let Some(cached) = state.search_cache.get(&cache_key).await {
        debug!("search cache hit for query");
        // Return cached results with current extras (rewrite + duplicate check)
        let cached_extras = SearchExtras {
            query_rewrite: Some(rewrite_result),
            duplicate_warning,
            ..Default::default()
        };
        return Ok((cached, cached_extras));
    }

    let _permit = state.outbound_limit.acquire().await.expect("semaphore closed");
    let mut params: HashMap<String, String> = HashMap::new();
    let mut engines = std::env::var("SEARXNG_ENGINES").unwrap_or_else(|_| "google,brave,reddit,hackernews".to_string());
    let mut categories = "general".to_string();

    // Engine forcing for GitHub/StackOverflow queries
    let query_lower = effective_query.to_lowercase();
    if query_lower.contains("github") || query_lower.contains("repo") || query_lower.contains("repository") {
        engines = "github,google,brave,startpage".to_string();
        categories = "it".to_string();
    } else if query_lower.contains("stackoverflow") || query_lower.contains("stack overflow") {
        engines = "google,brave,startpage".to_string();
        categories = "it".to_string();
    }
    
    // Use effective query (rewritten or original)
    params.insert("q".into(), effective_query.to_string());
    params.insert("format".into(), "json".into());
    params.insert("engines".into(), engines);
    params.insert("categories".into(), categories);
    params.insert("time_range".into(), "".into());
    params.insert("language".into(), "en".into());
    params.insert("safesearch".into(), "0".into());
    params.insert("pageno".into(), "1".into());

    if let Some(ov) = overrides {
    if let Some(v) = ov.engines { if !v.is_empty() { params.insert("engines".into(), v); } }
    if let Some(v) = ov.categories { if !v.is_empty() { params.insert("categories".into(), v); } }
    if let Some(v) = ov.language { if !v.is_empty() { params.insert("language".into(), v); } }
    if let Some(v) = ov.time_range { params.insert("time_range".into(), v); }
    if let Some(v) = ov.safesearch { params.insert("safesearch".into(), match v { 0 => "0".into(), 1 => "1".into(), 2 => "2".into(), _ => "0".into() }); }
    if let Some(v) = ov.pageno { params.insert("pageno".into(), v.to_string()); }
    }
    
    let search_url = format!("{}/search", state.searxng_url);
    debug!("Search URL: {}", search_url);

    let client = state.http_client.clone();
    let client_ip = std::env::var("SEARXNG_CLIENT_IP").unwrap_or_else(|_| "127.0.0.1".to_string());

    async fn fetch(
        client: reqwest::Client,
        search_url: String,
        client_ip: String,
        params: HashMap<String, String>,
    ) -> Result<SearxngResponse> {
        let params_cloned = params.clone();
        retry(
            ExponentialBackoffBuilder::new()
                .with_initial_interval(std::time::Duration::from_millis(200))
                .with_max_interval(std::time::Duration::from_secs(2))
                .with_max_elapsed_time(Some(std::time::Duration::from_secs(4)))
                .build(),
            || async {
                let mut url = reqwest::Url::parse(&search_url)
                    .map_err(|e| backoff::Error::permanent(anyhow!("Invalid URL: {}", e)))?;
                {
                    let mut query_pairs = url.query_pairs_mut();
                    for (k, v) in &params_cloned {
                        query_pairs.append_pair(k, v);
                    }
                }
                let resp = client
                    .get(url)
                    .header("User-Agent", "SearchScrape/1.0")
                    .header("Accept", "application/json")
                    .header("X-Forwarded-For", &client_ip)
                    .header("X-Real-IP", &client_ip)
                    .send()
                    .await
                    .map_err(|e| backoff::Error::transient(anyhow!("Failed to send request to SearXNG: {}", e)))?;
                if !resp.status().is_success() {
                    let status = resp.status();
                    let text = resp.text().await.unwrap_or_else(|_| "".into());
                    let err = anyhow!("SearXNG request failed with status {}: {}", status, text);
                    if status.is_server_error() {
                        return Err(backoff::Error::transient(err));
                    }
                    return Err(backoff::Error::permanent(err));
                }
                match resp.json::<SearxngResponse>().await {
                    Ok(parsed) => Ok(parsed),
                    Err(e) => Err(backoff::Error::transient(anyhow!("Failed to parse SearXNG response: {}", e))),
                }
            },
        )
        .await
    }

    let searxng_response: SearxngResponse = fetch(
        client.clone(),
        search_url.clone(),
        client_ip.clone(),
        params.clone(),
    )
    .await?;

    let mut community_response: Option<SearxngResponse> = None;
    if std::env::var("SEARCH_COMMUNITY_SOURCES")
        .unwrap_or_else(|_| "true".to_string())
        .to_lowercase()
        == "true"
    {
        let mut community_params = params.clone();
        community_params.insert("engines".into(), "reddit,hackernews".to_string());
        community_params.insert("categories".into(), "general".to_string());
        community_params.insert(
            "q".into(),
            format!("{} (site:reddit.com OR site:news.ycombinator.com)", effective_query),
        );
        if let Ok(response) = fetch(
            client.clone(),
            search_url.clone(),
            client_ip.clone(),
            community_params,
        )
        .await
        {
            community_response = Some(response);
        }
    }
    
    info!("SearXNG returned {} results", searxng_response.results.len());
    
    // Extract extras from SearXNG response
    let extras = SearchExtras {
        answers: searxng_response.answers
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect(),
        suggestions: searxng_response.suggestions
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect(),
        corrections: searxng_response.corrections
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect(),
        unresponsive_engines: searxng_response.unresponsive_engines
            .and_then(|v| v.as_object().cloned())
            .map(|obj| obj.keys().cloned().collect())
            .unwrap_or_default(),
        query_rewrite: Some(rewrite_result),
        duplicate_warning,
    };
    
    // Convert to our format with enhanced metadata (Priority 2)
    let mut seen = std::collections::HashSet::new();
    let mut results: Vec<SearchResult> = Vec::new();
    for result in searxng_response.results.into_iter() {
        if seen.insert(result.url.clone()) {
            let (domain, source_type) = classify_search_result(&result.url);
            results.push(SearchResult {
                url: result.url,
                title: result.title,
                content: result.content,
                engine: Some(result.engine),
                score: result.score,
                domain,
                source_type: Some(source_type),
            });
        }
    }

    if let Some(community) = community_response {
        for result in community.results.into_iter() {
            if seen.insert(result.url.clone()) {
                let (domain, source_type) = classify_search_result(&result.url);
                results.push(SearchResult {
                    url: result.url,
                    title: result.title,
                    content: result.content,
                    engine: Some(result.engine),
                    score: result.score,
                    domain,
                    source_type: Some(source_type),
                });
            }
        }
    }
    
    debug!("Converted {} results", results.len());
    
    // PROFESSIONAL UPGRADE: Enhanced semantic reranking with keyword boosting
    // Boost results that contain query keywords in the first 200 chars
    let reranker = Reranker::new(query);
    let boosted_results = boost_by_early_keywords(&results, query);
    let reranked_results = reranker.rerank_top(boosted_results, 50); // Keep top 50 reranked results
    
    info!("Reranked {} results by relevance (with keyword boosting)", reranked_results.len());
    
    // Fill cache with reranked results
    state.search_cache.insert(cache_key, reranked_results.clone()).await;
    
    // Auto-log to history if memory is enabled (Phase 1)
    if let Some(memory) = &state.memory {
        let result_json = serde_json::to_value(&reranked_results).unwrap_or_default();
        
        if let Err(e) = memory.log_search(query.to_string(), &result_json, reranked_results.len()).await {
            tracing::warn!("Failed to log search to history: {}", e);
        }
    }
    
    Ok((reranked_results, extras))
}

/// Classify search result by domain and source type (Priority 2)
/// Returns (domain, source_type)
fn classify_search_result(url_str: &str) -> (Option<String>, String) {
    let domain = url::Url::parse(url_str)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_string()));
    
    let source_type = if let Some(ref d) = domain {
        let d_lower = d.to_lowercase();
        
        // Documentation sites
        if d_lower.ends_with(".github.io") 
            || d_lower.contains("docs.rs")
            || d_lower.contains("readthedocs")
            || d_lower.contains("rust-lang.org")
            || d_lower.contains("doc.rust-lang")
            || d_lower.contains("developer.mozilla.org")
            || d_lower.contains("learn.microsoft.com")
            || d_lower.contains("man7.org")
            || d_lower.contains("devdocs.io")
        {
            "docs".to_string()
        }
        // Repository hosting
        else if d_lower.contains("github.com")
            || d_lower.contains("gitlab.com")
            || d_lower.contains("bitbucket.org")
            || d_lower.contains("codeberg.org")
        {
            "repo".to_string()
        }
        // News sites
        else if d_lower.contains("news")
            || d_lower.contains("blog")
            || d_lower.contains("medium.com")
            || d_lower.contains("dev.to")
            || d_lower.contains("hackernews")
            || d_lower.contains("reddit.com")
            || d_lower.contains("thenewstack.io")
        {
            "blog".to_string()
        }
        // Video platforms
        else if d_lower.contains("youtube.com")
            || d_lower.contains("vimeo.com")
        {
            "video".to_string()
        }
        // Q&A sites
        else if d_lower.contains("stackoverflow.com")
            || d_lower.contains("stackexchange.com")
        {
            "qa".to_string()
        }
        // Package registries
        else if d_lower.contains("crates.io")
            || d_lower.contains("npmjs.com")
            || d_lower.contains("pypi.org")
        {
            "package".to_string()
        }
        // Gaming/unrelated (noise filtering)
        else if d_lower.contains("steam")
            || d_lower.contains("facepunch")
            || d_lower.contains("game")
        {
            "gaming".to_string()
        }
        else {
            "other".to_string()
        }
    } else {
        "other".to_string()
    };
    
    (domain, source_type)
}

/// PROFESSIONAL UPGRADE: Boost results with query keywords in first 200 chars
/// This increases relevance by prioritizing early keyword matches
fn boost_by_early_keywords(results: &[SearchResult], query: &str) -> Vec<SearchResult> {
    let query_tokens: Vec<String> = query
        .to_lowercase()
        .split_whitespace()
        .filter(|s| s.len() > 2)
        .map(|s| s.to_string())
        .collect();
    
    if query_tokens.is_empty() {
        return results.to_vec();
    }
    
    let mut boosted_results: Vec<(SearchResult, f64)> = results
        .iter()
        .map(|result| {
            let mut boost_score = result.score.unwrap_or(1.0);
            
            // Check first 200 chars of content for query keywords
            let content_preview: String = result.content.chars().take(200).collect();
            let content_lower = content_preview.to_lowercase();
            
            let mut early_matches = 0;
            for token in &query_tokens {
                if content_lower.contains(token) {
                    early_matches += 1;
                }
            }
            
            // Boost by 20% if >50% of keywords appear early
            let match_ratio = early_matches as f64 / query_tokens.len() as f64;
            if match_ratio > 0.5 {
                boost_score *= 1.2;
            }
            
            (result.clone(), boost_score)
        })
        .collect();
    
    // Sort by boosted score
    boosted_results.sort_by(|(_, a), (_, b)| {
        b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal)
    });
    
    // Update scores and return
    boosted_results
        .into_iter()
        .map(|(mut result, boosted_score)| {
            result.score = Some(boosted_score);
            result
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    
    #[tokio::test]
    async fn test_search_web() {
        // This test requires a running SearXNG instance
        // Skip in CI/CD environments
        if std::env::var("CI").is_ok() {
            return;
        }
        
        let state = Arc::new(AppState::new(
            "http://localhost:8888".to_string(),
            reqwest::Client::new(),
        ));
        
        let results = search_web(&state, "rust programming language").await;
        
        match results {
            Ok((results, _extras)) => {
                assert!(!results.is_empty(), "Should return some results");
                for result in &results {
                    assert!(!result.url.is_empty(), "URL should not be empty");
                    assert!(!result.title.is_empty(), "Title should not be empty");
                }
            }
            Err(e) => {
                // If SearXNG is not running, this is expected
                tracing::warn!("Search test failed (expected if SearXNG not running): {}", e);
            }
        }
    }
}