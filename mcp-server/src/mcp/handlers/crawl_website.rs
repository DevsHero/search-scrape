use super::common::parse_quality_mode;
use crate::crawl::CrawlConfig;
use crate::mcp::{McpCallResponse, McpContent};
use crate::types::{AuthWallBlocked, ErrorResponse};
use crate::{crawl, AppState};
use axum::http::StatusCode;
use axum::response::Json;
use serde_json::Value;
use std::sync::Arc;
use tracing::error;

pub async fn handle(
    state: Arc<AppState>,
    arguments: &Value,
) -> Result<Json<McpCallResponse>, (StatusCode, Json<ErrorResponse>)> {
    let url = arguments
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "Missing required parameter: url".to_string(),
                }),
            )
        })?;

    let quality_mode = parse_quality_mode(arguments)?;

    let config = CrawlConfig {
        max_depth: arguments
            .get("max_depth")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize)
            .unwrap_or(3),
        max_pages: arguments
            .get("max_pages")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize)
            .unwrap_or(50),
        max_concurrent: arguments
            .get("max_concurrent")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize)
            .unwrap_or(5),
        same_domain_only: arguments
            .get("same_domain_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(true),
        include_patterns: arguments
            .get("include_patterns")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|s| s.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default(),
        exclude_patterns: arguments
            .get("exclude_patterns")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|s| s.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default(),
        max_chars_per_page: arguments
            .get("max_chars_per_page")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize)
            .unwrap_or(5000),
        quality_mode: quality_mode.as_str().to_string(),
    };

    let use_proxy = arguments
        .get("use_proxy")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // BUG-9: cap total JSON output size to prevent large workspace storage spill.
    // Agent can tune via `max_chars` (default 10_000).
    let max_chars = arguments
        .get("max_chars")
        .and_then(|v| v.as_u64())
        .map(|n| n as usize)
        .unwrap_or(10_000);

    match crawl::crawl_website(&state, url, config, use_proxy).await {
        Ok(response) => {
            let mut json_str = serde_json::to_string_pretty(&response)
                .unwrap_or_else(|e| format!(r#"{{"error": "Failed to serialize: {}"}}"#, e));
            if json_str.len() > max_chars {
                json_str.truncate(max_chars);
                json_str.push_str("\n... [truncated: increase max_chars to see more results]");
            }
            Ok(Json(McpCallResponse {
                content: vec![McpContent {
                    content_type: "text".to_string(),
                    text: json_str,
                }],
                is_error: false,
            }))
        }
        Err(e) => {
            error!("Crawl tool error: {}", e);

            let msg = e.to_string();
            if msg.starts_with("NEED_HITL:") {
                let blocked = AuthWallBlocked {
                    status: "NEED_HITL".to_string(),
                    reason: msg,
                    url: url.to_string(),
                    suggested_action: "non_robot_search".to_string(),
                    github_raw_url: None,
                };
                let json_str = serde_json::to_string_pretty(&blocked)
                    .unwrap_or_else(|e| format!(r#"{{"error": "Failed to serialize: {}"}}"#, e));
                return Ok(Json(McpCallResponse {
                    content: vec![McpContent {
                        content_type: "text".to_string(),
                        text: json_str,
                    }],
                    is_error: true,
                }));
            }

            Ok(Json(McpCallResponse {
                content: vec![McpContent {
                    content_type: "text".to_string(),
                    text: format!("Crawl failed: {}", e),
                }],
                is_error: true,
            }))
        }
    }
}
