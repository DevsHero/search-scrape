use crate::mcp::{McpCallResponse, McpContent};
use crate::types::ErrorResponse;
use crate::{batch_scrape, AppState};
use super::common::parse_quality_mode;
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

    let output_format = arguments
        .get("output_format")
        .and_then(|v| v.as_str())
        .unwrap_or("json");

    let quality_mode = parse_quality_mode(arguments)?;

    match batch_scrape::scrape_batch(
        &state,
        urls,
        max_concurrent,
        max_chars,
        use_proxy,
        Some(quality_mode),
    )
    .await
    {
        Ok(response) => {
            if output_format == "text" {
                let mut text = format!(
                    "Batch scrape summary\nTotal: {}\nSuccessful: {}\nFailed: {}\nDuration: {}ms\n\n",
                    response.total,
                    response.successful,
                    response.failed,
                    response.total_duration_ms
                );

                for (index, item) in response.results.iter().enumerate() {
                    if item.success {
                        if let Some(data) = &item.data {
                            text.push_str(&format!(
                                "{}. ✅ {}\n   Title: {}\n   Words: {}\n   Truncated: {}\n\n",
                                index + 1,
                                item.url,
                                data.title,
                                data.word_count,
                                data.truncated
                            ));
                        }
                    } else {
                        text.push_str(&format!(
                            "{}. ❌ {}\n   Error: {}\n\n",
                            index + 1,
                            item.url,
                            item.error.as_deref().unwrap_or("unknown error")
                        ));
                    }
                }

                return Ok(Json(McpCallResponse {
                    content: vec![McpContent {
                        content_type: "text".to_string(),
                        text,
                    }],
                    is_error: false,
                }));
            }

            let normalized_results: Vec<serde_json::Value> = response
                .results
                .iter()
                .map(|item| {
                    if let Some(data) = &item.data {
                        serde_json::json!({
                            "url": item.url,
                            "success": item.success,
                            "duration_ms": item.duration_ms,
                            "data": {
                                "metadata": {
                                    "title": data.title,
                                    "canonical_url": data.canonical_url,
                                    "site_name": data.site_name,
                                    "author": data.author,
                                    "published_at": data.published_at,
                                    "language": data.language,
                                    "status_code": data.status_code,
                                    "content_type": data.content_type,
                                    "word_count": data.word_count,
                                    "reading_time_minutes": data.reading_time_minutes,
                                    "extraction_score": data.extraction_score,
                                    "warnings": data.warnings,
                                    "truncated": data.truncated,
                                    "actual_chars": data.actual_chars,
                                    "max_chars_limit": data.max_chars_limit,
                                    "og_image": data.og_image,
                                    "meta_description": data.meta_description
                                },
                                "markdown_content": data.clean_content,
                                "headings": data.headings,
                                "links": data.links,
                                "images": data.images,
                                "code_blocks": data.code_blocks
                            }
                        })
                    } else {
                        serde_json::json!({
                            "url": item.url,
                            "success": item.success,
                            "duration_ms": item.duration_ms,
                            "error": item.error
                        })
                    }
                })
                .collect();

            let normalized = serde_json::json!({
                "total": response.total,
                "successful": response.successful,
                "failed": response.failed,
                "total_duration_ms": response.total_duration_ms,
                "results": normalized_results
            });

            let json_str = serde_json::to_string_pretty(&normalized)
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