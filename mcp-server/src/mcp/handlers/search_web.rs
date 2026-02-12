use crate::mcp::{McpCallResponse, McpContent};
use crate::types::ErrorResponse;
use crate::{search, AppState};
use axum::http::StatusCode;
use axum::response::Json;
use serde_json::Value;
use std::sync::Arc;

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

    let max_results = arguments
        .get("max_results")
        .and_then(|v| v.as_u64())
        .map(|n| n as usize)
        .unwrap_or(10);

    let overrides = search::SearchParamOverrides {
        engines: arguments
            .get("engines")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        categories: arguments
            .get("categories")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        language: arguments
            .get("language")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        safesearch: arguments
            .get("safesearch")
            .and_then(|v| v.as_i64())
            .and_then(|n| if (0..=2).contains(&n) { Some(n as u8) } else { None }),
        time_range: arguments
            .get("time_range")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        pageno: arguments
            .get("pageno")
            .and_then(|v| v.as_u64())
            .and_then(|n| if n >= 1 { Some(n as u32) } else { None }),
    };

    let has_overrides = overrides.engines.is_some()
        || overrides.categories.is_some()
        || overrides.language.is_some()
        || overrides.safesearch.is_some()
        || overrides.time_range.is_some()
        || overrides.pageno.is_some();

    let (results, extras) = search::search_web_with_params(
        &state,
        query,
        if has_overrides { Some(overrides) } else { None },
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Search failed: {}", e),
            }),
        )
    })?;

    let content_text = if results.is_empty() {
        let mut text = format!("No search results found for query: '{}'\n\n", query);

        if !extras.suggestions.is_empty() {
            text.push_str(&format!("**Suggestions:** {}\n", extras.suggestions.join(", ")));
        }
        if !extras.corrections.is_empty() {
            text.push_str(&format!("**Did you mean:** {}\n", extras.corrections.join(", ")));
        }
        if !extras.unresponsive_engines.is_empty() {
            text.push_str(&format!(
                "\n**Note:** {} search engine(s) did not respond. Try different engines or retry.\n",
                extras.unresponsive_engines.len()
            ));
        }
        text
    } else {
        let limited_results = results.iter().take(max_results);
        let result_count = results.len();

        let mut text = format!("Found {} search results for '{}':", result_count, query);
        if result_count > max_results {
            text.push_str(&format!(" (showing top {})\n", max_results));
        }
        text.push_str("\n\n");

        if !extras.answers.is_empty() {
            text.push_str("**Instant Answers:**\n");
            for answer in &extras.answers {
                text.push_str(&format!("📌 {}\n\n", answer));
            }
        }

        for (i, result) in limited_results.enumerate() {
            text.push_str(&format!(
                "{}. **{}**\n   URL: {}\n   Snippet: {}\n\n",
                i + 1,
                result.title,
                result.url,
                result.content.chars().take(200).collect::<String>()
            ));
        }

        if !extras.suggestions.is_empty() {
            text.push_str(&format!("\n**Related searches:** {}\n", extras.suggestions.join(", ")));
        }
        if !extras.unresponsive_engines.is_empty() {
            text.push_str(&format!(
                "\n⚠️ **Note:** {} engine(s) did not respond (may affect completeness)\n",
                extras.unresponsive_engines.len()
            ));
        }

        text
    };

    Ok(Json(McpCallResponse {
        content: vec![McpContent {
            content_type: "text".to_string(),
            text: content_text,
        }],
        is_error: false,
    }))
}