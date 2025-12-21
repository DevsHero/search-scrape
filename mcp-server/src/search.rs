use crate::types::*;
use crate::AppState;
use anyhow::{anyhow, Result};
use backoff::future::retry;
use backoff::ExponentialBackoffBuilder;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info};

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
        return Ok((cached, SearchExtras::default()));
    }

    let _permit = state.outbound_limit.acquire().await.expect("semaphore closed");
    let mut params: HashMap<String, String> = HashMap::new();
    let engines = std::env::var("SEARXNG_ENGINES").unwrap_or_else(|_| "duckduckgo,google,bing".to_string());
    
    params.insert("q".into(), query.to_string());
    params.insert("format".into(), "json".into());
    params.insert("engines".into(), engines);
    params.insert("categories".into(), "general".into());
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
    let search_url_owned = search_url.clone();
    let params_cloned = params.clone();
    let searxng_response: SearxngResponse = retry(
        ExponentialBackoffBuilder::new()
            .with_initial_interval(std::time::Duration::from_millis(200))
            .with_max_interval(std::time::Duration::from_secs(2))
            .with_max_elapsed_time(Some(std::time::Duration::from_secs(4)))
            .build(),
        || async {
            let resp = client
                .get(&search_url_owned)
                .query(&params_cloned)
                .header("User-Agent", "MCP-Server/1.0")
                .header("Accept", "application/json")
                .send()
                .await
                .map_err(|e| backoff::Error::transient(anyhow!("Failed to send request to SearXNG: {}", e)))?;
            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_else(|_| "".into());
                let err = anyhow!("SearXNG request failed with status {}: {}", status, text);
                // 5xx transient, others permanent
                if status.is_server_error() {
                    return Err(backoff::Error::transient(err));
                } else {
                    return Err(backoff::Error::permanent(err));
                }
            }
            match resp.json::<SearxngResponse>().await {
                Ok(parsed) => Ok(parsed),
                Err(e) => Err(backoff::Error::transient(anyhow!("Failed to parse SearXNG response: {}", e))),
            }
        },
    )
    .await?;
    
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
    };
    
    // Convert to our format
    let mut seen = std::collections::HashSet::new();
    let mut results: Vec<SearchResult> = Vec::new();
    for result in searxng_response.results.into_iter() {
        if seen.insert(result.url.clone()) {
            results.push(SearchResult {
                url: result.url,
                title: result.title,
                content: result.content,
                engine: Some(result.engine),
                score: result.score,
            });
        }
    }
    
    debug!("Converted {} results", results.len());
    // Fill cache with composite key
    state.search_cache.insert(cache_key, results.clone()).await;
    Ok((results, extras))
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
            Ok(results) => {
                assert!(!results.is_empty(), "Should return some results");
                for result in &results {
                    assert!(!result.url.is_empty(), "URL should not be empty");
                    assert!(!result.title.is_empty(), "Title should not be empty");
                }
            }
            Err(e) => {
                // If SearXNG is not running, this is expected
                println!("Search test failed (expected if SearXNG not running): {}", e);
            }
        }
    }
}