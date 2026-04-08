use super::common::parse_quality_mode;
use crate::deep_research::{deep_research, DeepResearchConfig};
use crate::mcp::{McpCallResponse, McpContent};
use crate::mcp::tooling::deep_research_enabled;
use crate::types::ErrorResponse;
use crate::AppState;
use axum::http::StatusCode;
use axum::response::Json;
use serde_json::Value;
use std::sync::Arc;
use tracing::error;

fn parse_request(
    arguments: &Value,
) -> Result<(String, DeepResearchConfig), (StatusCode, Json<ErrorResponse>)> {
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
        .map(|n| n.max(1) as usize)
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

    Ok((
        query,
        DeepResearchConfig {
            depth,
            max_sources_per_hop,
            max_chars_per_source,
            max_concurrent,
            use_proxy,
            quality_mode: Some(quality_mode),
            relevance_threshold,
        },
    ))
}

pub async fn handle(
    state: Arc<AppState>,
    arguments: &Value,
) -> Result<Json<McpCallResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Runtime gate (belt-and-suspenders: catalog already filters this entry when disabled,
    // but direct HTTP calls bypass tool discovery).
    if !deep_research_enabled() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "deep_research is disabled. \
                    Set DEEP_RESEARCH_ENABLED=1 (or unset) to enable at runtime. \
                    For a build without this tool: cargo build --no-default-features."
                    .to_string(),
            }),
        ));
    }

    let (query, config) = parse_request(arguments)?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_request_clamps_and_preserves_all_parameters() {
        let (query, config) = parse_request(&json!({
            "query": "rust model context protocol",
            "depth": 99,
            "max_sources": 999,
            "max_chars_per_source": 0,
            "max_concurrent": 999,
            "use_proxy": true,
            "relevance_threshold": 2.0,
            "quality_mode": "aggressive"
        }))
        .expect("request should parse");

        assert_eq!(query, "rust model context protocol");
        assert_eq!(config.depth, 3);
        assert_eq!(config.max_sources_per_hop, 20);
        assert_eq!(config.max_chars_per_source, 1);
        assert_eq!(config.max_concurrent, 10);
        assert!(config.use_proxy);
        assert_eq!(config.relevance_threshold, Some(1.0));
        assert_eq!(config.quality_mode.map(|mode| mode.as_str()), Some("aggressive"));
    }

    #[test]
    fn parse_request_rejects_empty_query() {
        let result = parse_request(&json!({"query": "   "}));
        assert!(result.is_err(), "empty query should fail");
        let err = result.err().expect("error should be present");
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
        assert_eq!(err.1.error, "query must not be empty");
    }

    #[test]
    fn parse_request_rejects_invalid_quality_mode() {
        let result = parse_request(&json!({
            "query": "rust model context protocol",
            "quality_mode": "turbo"
        }));
        assert!(result.is_err(), "invalid quality mode should fail");
        let err = result.err().expect("error should be present");

        assert_eq!(err.0, StatusCode::BAD_REQUEST);
        assert_eq!(
            err.1.error,
            "Invalid quality_mode. Allowed values: balanced, aggressive, high"
        );
    }
}
