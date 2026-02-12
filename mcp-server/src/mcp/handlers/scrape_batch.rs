use crate::mcp::{McpCallResponse, McpContent};
use crate::types::ErrorResponse;
use crate::{batch_scrape, AppState};
use axum::http::StatusCode;
use axum::response::Json;
use serde_json::Value;
use std::sync::Arc;
use tracing::error;

pub async fn handle(
    state: Arc<AppState>,
    arguments: &Value,
) -> Result<Json<McpCallResponse>, (StatusCode, Json<ErrorResponse>)> {
    let urls = arguments
        .get("urls")
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "Missing required parameter: urls (must be array)".to_string(),
                }),
            )
        })?
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect::<Vec<_>>();

    if urls.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "urls array cannot be empty".to_string(),
            }),
        ));
    }

    let max_concurrent = arguments
        .get("max_concurrent")
        .and_then(|v| v.as_u64())
        .map(|n| n as usize)
        .unwrap_or(10);

    let max_chars = arguments
        .get("max_chars")
        .and_then(|v| v.as_u64())
        .map(|n| n as usize);

    let use_proxy = arguments
        .get("use_proxy")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    match batch_scrape::scrape_batch(&state, urls, max_concurrent, max_chars, use_proxy).await {
        Ok(response) => {
            let json_str = serde_json::to_string_pretty(&response)
                .unwrap_or_else(|e| format!(r#"{{"error": "Failed to serialize: {}"}}"#, e));
            Ok(Json(McpCallResponse {
                content: vec![McpContent {
                    content_type: "text".to_string(),
                    text: json_str,
                }],
                is_error: false,
            }))
        }
        Err(e) => {
            error!("Batch scrape tool error: {}", e);
            Ok(Json(McpCallResponse {
                content: vec![McpContent {
                    content_type: "text".to_string(),
                    text: format!("Batch scrape failed: {}", e),
                }],
                is_error: true,
            }))
        }
    }
}