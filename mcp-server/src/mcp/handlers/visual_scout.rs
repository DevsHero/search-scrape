use crate::mcp::{McpCallResponse, McpContent};
use crate::types::ErrorResponse;
use axum::http::StatusCode;
use axum::response::Json;
use serde_json::Value;
use std::sync::Arc;

use crate::AppState;

pub async fn handle(
    _state: Arc<AppState>,
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

    let proxy_url = arguments
        .get("proxy_url")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let width = arguments
        .get("width")
        .and_then(|v| v.as_u64())
        .map(|n| n as u32);

    let height = arguments
        .get("height")
        .and_then(|v| v.as_u64())
        .map(|n| n as u32);

    let output_format = arguments
        .get("output_format")
        .and_then(|v| v.as_str())
        .unwrap_or("json");

    match crate::visual_scout::take_screenshot(url, proxy_url.as_deref(), width, height).await {
        Ok(result) => {
            if output_format == "text" {
                let text = format!(
                    "📸 **Visual Scout — {}**\n\nPage title: {}\nFinal URL: {}\n{}\n\nScreenshot saved: {} bytes\nPath: {}",
                    url,
                    result.page_title,
                    result.url,
                    result.hint,
                    result.screenshot_bytes,
                    result.screenshot_path,
                );
                return Ok(Json(McpCallResponse {
                    content: vec![McpContent {
                        content_type: "text".to_string(),
                        text,
                    }],
                    is_error: false,
                }));
            }

            let json_str = serde_json::to_string_pretty(&result)
                .unwrap_or_else(|e| format!(r#"{{"error": "Failed to serialize: {}"}}"#, e));

            Ok(Json(McpCallResponse {
                content: vec![McpContent {
                    content_type: "text".to_string(),
                    text: json_str,
                }],
                is_error: false,
            }))
        }
        Err(e) => Ok(Json(McpCallResponse {
            content: vec![McpContent {
                content_type: "text".to_string(),
                text: format!("visual_scout failed: {}", e),
            }],
            is_error: true,
        })),
    }
}
