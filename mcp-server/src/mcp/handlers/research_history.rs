use crate::history::EntryType;
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

    let limit = arguments
        .get("limit")
        .and_then(|v| v.as_u64())
        .map(|n| n as usize)
        .unwrap_or(10);

    let threshold = arguments
        .get("threshold")
        .and_then(|v| v.as_f64())
        .map(|n| n as f32)
        .unwrap_or(0.5);

    let entry_type = arguments.get("entry_type").and_then(|v| v.as_str());

    if let Some(memory) = &state.memory {
        let entry_type_filter = entry_type.map(|s| {
            if s == "search" {
                EntryType::Search
            } else {
                EntryType::Scrape
            }
        });

        match memory
            .search_history(query, limit, threshold, entry_type_filter)
            .await
        {
            Ok(results) => {
                let formatted_results = results
                    .iter()
                    .map(|(entry, score)| {
                        let match_quality = if *score >= 0.9 {
                            "Exact Match"
                        } else if *score >= 0.7 {
                            "High Match"
                        } else if *score >= 0.5 {
                            "Partial Match"
                        } else {
                            "Low Match"
                        };

                        serde_json::json!({
                            "query": entry.query,
                            "entry_type": format!("{:?}", entry.entry_type),
                            "similarity_score": score,
                            "match_quality": match_quality,
                            "timestamp": entry.timestamp.to_rfc3339(),
                            "domain": entry.domain,
                            "summary": entry.summary
                        })
                    })
                    .collect::<Vec<_>>();

                let result_json = serde_json::json!({
                    "query": query,
                    "total_results": formatted_results.len(),
                    "threshold": threshold,
                    "results": formatted_results
                });

                Ok(Json(McpCallResponse {
                    content: vec![McpContent {
                        content_type: "text".to_string(),
                        text: serde_json::to_string_pretty(&result_json)
                            .unwrap_or_else(|e| format!(r#"{{"error": "Serialization failed: {}"}}"#, e)),
                    }],
                    is_error: false,
                }))
            }
            Err(e) => {
                error!("Research history error: {}", e);
                Ok(Json(McpCallResponse {
                    content: vec![McpContent {
                        content_type: "text".to_string(),
                        text: format!("Research history search failed: {}", e),
                    }],
                    is_error: true,
                }))
            }
        }
    } else {
        let result_json = serde_json::json!({
            "query": query,
            "total_results": 0,
            "threshold": threshold,
            "results": [],
            "warnings": ["research_history_unavailable_memory_not_initialized"]
        });

        Ok(Json(McpCallResponse {
            content: vec![McpContent {
                content_type: "text".to_string(),
                text: serde_json::to_string_pretty(&result_json)
                    .unwrap_or_else(|e| format!(r#"{{"error": "Serialization failed: {}"}}"#, e)),
            }],
            is_error: false,
        }))
    }
}