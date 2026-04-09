use super::common::parse_quality_mode;
use crate::mcp::{McpCallResponse, McpContent};
use crate::types::ErrorResponse;
use crate::{scrape, search, AppState};
use axum::http::StatusCode;
use axum::response::Json;
use serde_json::Value;
use std::sync::Arc;
use tracing::warn;

pub async fn handle(
    state: Arc<AppState>,
    arguments: &Value,
) -> Result<Json<McpCallResponse>, (StatusCode, Json<ErrorResponse>)> {
    let query = arguments
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "Missing required parameter: query".to_string(),
                }),
            )
        })?;

    let top_n = arguments
        .get("top_n")
        .and_then(|v| v.as_u64())
        .map(|n| n as usize)
        .unwrap_or(3);

    let use_proxy = arguments
        .get("use_proxy")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let quality_mode = parse_quality_mode(arguments)?;

    let (results, extras) = search::search_web(&state, query).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Search failed: {}", e),
            }),
        )
    })?;

    let to_scrape: Vec<String> = results.iter().take(top_n).map(|r| r.url.clone()).collect();
    let mut scraped_content = Vec::new();
    let mut tasks = Vec::new();

    for url in to_scrape {
        let state_cloned = Arc::clone(&state);
        let quality_mode_cloned = quality_mode;
        tasks.push(tokio::spawn(async move {
            (
                url.clone(),
                scrape::scrape_url_with_options(
                    &state_cloned,
                    &url,
                    use_proxy,
                    Some(quality_mode_cloned),
                )
                .await,
            )
        }));
    }

    for task in tasks {
        match task.await {
            Ok((_, Ok(content))) => scraped_content.push(content),
            Ok((url, Err(e))) => warn!("Structured scrape failed for {}: {}", url, e),
            Err(e) => warn!("Structured scrape task join error: {}", e),
        }
    }

    let mut text = format!("Found {} results for '{}'\n\n", results.len(), query);
    text.push_str(&format!(
        "Structured scrapes: {}\n\n",
        scraped_content.len()
    ));
    if !extras.degraded_engines.is_empty() {
        text.push_str(&format!(
            "Degraded engines: {}\n",
            extras.degraded_engines.join(", ")
        ));
    }
    if !extras.skipped_engines.is_empty() {
        text.push_str(&format!(
            "Skipped engines: {}\n\n",
            extras.skipped_engines.join(", ")
        ));
    }
    for (i, item) in scraped_content.iter().enumerate() {
        text.push_str(&format!(
            "{}. {} ({} words)\nURL: {}\n",
            i + 1,
            item.title,
            item.word_count,
            item.url
        ));
        // Include truncated content so agents get actual page summaries.
        let content = if !item.clean_content.is_empty() {
            item.clean_content.as_str()
        } else {
            item.content.as_str()
        };
        if !content.is_empty() {
            let preview: String = content.chars().take(1500).collect();
            text.push_str(&format!("Content: {}\n", preview));
            if content.len() > 1500 {
                text.push_str("... [truncated — use web_fetch for full content]\n");
            }
        }
        text.push('\n');
    }

    Ok(Json(McpCallResponse {
        content: vec![McpContent {
            content_type: "text".to_string(),
            text,
        }],
        is_error: false,
    }))
}
