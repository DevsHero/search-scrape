use super::common::parse_quality_mode;
use crate::deep_research::{deep_research, DeepResearchConfig};
use crate::mcp::{McpCallResponse, McpContent};
use crate::types::ErrorResponse;
use crate::AppState;
use axum::http::StatusCode;
use axum::response::Json;
use serde_json::Value;
use std::sync::Arc;
use tracing::error;

pub async fn handle(
    state: Arc<AppState>,
    arguments: &Value,
) -> Result<Json<McpCallResponse>, (StatusCode, Json<ErrorResponse>)> {
    // ── Required parameter ────────────────────────────────────────────────
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
        })?
        .to_string();

    if query.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "query must not be empty".to_string(),
            }),
        ));
    }

    // ── Optional parameters (with safe defaults) ──────────────────────────
    let depth = arguments
        .get("depth")
        .and_then(|v| v.as_u64())
        .map(|n| n.clamp(1, 3) as u8)
        .unwrap_or(1);

    let max_sources_per_hop = arguments
        .get("max_sources")
        .and_then(|v| v.as_u64())
        .map(|n| n.clamp(1, 20) as usize)
        .unwrap_or(10);

    let max_chars_per_source = arguments
        .get("max_chars_per_source")
        .and_then(|v| v.as_u64())
        .map(|n| n as usize)
        .unwrap_or(20_000);

    let max_concurrent = arguments
        .get("max_concurrent")
        .and_then(|v| v.as_u64())
        .map(|n| n.clamp(1, 10) as usize)
        .unwrap_or(3);

    let use_proxy = arguments
        .get("use_proxy")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let relevance_threshold = arguments
        .get("relevance_threshold")
        .and_then(|v| v.as_f64())
        .map(|f| f.clamp(0.0, 1.0) as f32);

    let quality_mode = parse_quality_mode(arguments)?;

    let config = DeepResearchConfig {
        depth,
        max_sources_per_hop,
        max_chars_per_source,
        max_concurrent,
        use_proxy,
        quality_mode: Some(quality_mode),
        relevance_threshold,
    };

    // ── Execute pipeline ──────────────────────────────────────────────────
    match deep_research(state, query, config).await {
        Ok(result) => {
            let json_str = serde_json::to_string_pretty(&result).unwrap_or_else(|e| {
                format!(r#"{{"error": "Failed to serialize result: {}"}}"#, e)
            });

            Ok(Json(McpCallResponse {
                content: vec![McpContent {
                    content_type: "text".to_string(),
                    text: json_str,
                }],
                is_error: false,
            }))
        }
        Err(e) => {
            error!("deep_research tool error: {}", e);
            Ok(Json(McpCallResponse {
                content: vec![McpContent {
                    content_type: "text".to_string(),
                    text: format!("Deep research failed: {}", e),
                }],
                is_error: true,
            }))
        }
    }
}
