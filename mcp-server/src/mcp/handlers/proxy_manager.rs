use crate::mcp::{McpCallResponse, McpContent};
use crate::mcp_tooling::format_proxy_display;
use crate::proxy_grabber;
use crate::types::ErrorResponse;
use crate::AppState;
use axum::http::StatusCode;
use axum::response::Json;
use serde_json::Value;
use std::collections::HashSet;
use std::sync::Arc;
use tracing::{error, warn};

pub async fn handle(
    state: Arc<AppState>,
    arguments: &Value,
) -> Result<Json<McpCallResponse>, (StatusCode, Json<ErrorResponse>)> {
    let action = arguments
        .get("action")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "Missing required parameter: action".to_string(),
                }),
            )
        })?;

    match action {
        "grab" => {
            let params = proxy_grabber::GrabParams {
                limit: arguments
                    .get("limit")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize),
                proxy_type: arguments
                    .get("proxy_type")
                    .and_then(|v| v.as_str())
                    .map(|v| v.to_string()),
                random: arguments
                    .get("random")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                store_ip_txt: arguments
                    .get("store_ip_txt")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                clear_ip_txt: arguments
                    .get("clear_ip_txt")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                append: arguments
                    .get("append")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
            };

            match proxy_grabber::grab_proxies(&state, params).await {
                Ok(result) => {
                    let json_str = serde_json::to_string_pretty(&result).unwrap_or_else(|e| {
                        format!(r#"{{"error": "Failed to serialize: {}"}}"#, e)
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
                    error!("Proxy grab tool error: {}", e);
                    Ok(Json(McpCallResponse {
                        content: vec![McpContent {
                            content_type: "text".to_string(),
                            text: format!("proxy_manager grab failed: {}", e),
                        }],
                        is_error: true,
                    }))
                }
            }
        }
        "list" => {
            let params = proxy_grabber::ListParams {
                limit: arguments
                    .get("limit")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize),
                proxy_type: arguments
                    .get("proxy_type")
                    .and_then(|v| v.as_str())
                    .map(|v| v.to_string()),
                random: arguments
                    .get("random")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                show_type: arguments
                    .get("show_proxy_type")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true),
            };

            match proxy_grabber::list_proxies(params).await {
                Ok(result) => {
                    let json_str = serde_json::to_string_pretty(&result).unwrap_or_else(|e| {
                        format!(r#"{{"error": "Failed to serialize: {}"}}"#, e)
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
                    error!("Proxy list tool error: {}", e);
                    Ok(Json(McpCallResponse {
                        content: vec![McpContent {
                            content_type: "text".to_string(),
                            text: format!("proxy_manager list failed: {}", e),
                        }],
                        is_error: true,
                    }))
                }
            }
        }
        "status" => {
            if let Some(proxy_manager) = &state.proxy_manager {
                match proxy_manager.get_status().await {
                    Ok(status) => {
                        let status_json = serde_json::json!({
                            "total_proxies": status.total_proxies,
                            "enabled_proxies": status.enabled_proxies,
                            "current_proxy": status.current_proxy.as_ref().map(|url| format_proxy_display(url)),
                            "best_proxy": status.best_proxy.as_ref().map(|proxy| {
                                serde_json::json!({
                                    "provider": proxy.provider,
                                    "priority": proxy.priority,
                                    "latency_ms": proxy.latency_ms,
                                    "last_success": proxy.last_success_timestamp,
                                    "failure_count": proxy.failure_count
                                })
                            }),
                            "current_ip": status.current_ip
                        });

                        Ok(Json(McpCallResponse {
                            content: vec![McpContent {
                                content_type: "text".to_string(),
                                text: format!(
                                    "Proxy Status:\n{}",
                                    serde_json::to_string_pretty(&status_json).unwrap()
                                ),
                            }],
                            is_error: false,
                        }))
                    }
                    Err(e) => {
                        error!("Get proxy status error: {}", e);
                        Ok(Json(McpCallResponse {
                            content: vec![McpContent {
                                content_type: "text".to_string(),
                                text: format!("proxy_manager status failed: {}", e),
                            }],
                            is_error: true,
                        }))
                    }
                }
            } else {
                Ok(Json(McpCallResponse {
                    content: vec![McpContent {
                        content_type: "text".to_string(),
                        text: "Proxy manager not available. Provide IP_LIST_PATH (default: ip.txt) to enable proxy support."
                            .to_string(),
                    }],
                    is_error: true,
                }))
            }
        }
        "switch" => {
            if let Some(proxy_manager) = &state.proxy_manager {
                let force_new = arguments
                    .get("force_new")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                if !force_new && proxy_manager.should_use_sticky_proxy().await {
                    if let Some(current) = proxy_manager.get_current_proxy().await {
                        return Ok(Json(McpCallResponse {
                            content: vec![McpContent {
                                content_type: "text".to_string(),
                                text: format!(
                                    "Using sticky session proxy (still valid): {}",
                                    format_proxy_display(&current)
                                ),
                            }],
                            is_error: false,
                        }));
                    }
                }

                match proxy_manager.switch_to_best_proxy().await {
                    Ok(proxy_url) => {
                        let masked_url = format_proxy_display(&proxy_url);

                        Ok(Json(McpCallResponse {
                            content: vec![McpContent {
                                content_type: "text".to_string(),
                                text: format!(
                                    "Switched to proxy: {}\n\nUse this proxy for next scrape by passing proxy parameter.",
                                    masked_url
                                ),
                            }],
                            is_error: false,
                        }))
                    }
                    Err(e) => {
                        error!("Switch proxy error: {}", e);
                        Ok(Json(McpCallResponse {
                            content: vec![McpContent {
                                content_type: "text".to_string(),
                                text: format!("proxy_manager switch failed: {}", e),
                            }],
                            is_error: true,
                        }))
                    }
                }
            } else {
                Ok(Json(McpCallResponse {
                    content: vec![McpContent {
                        content_type: "text".to_string(),
                        text: "Proxy manager not available. Provide IP_LIST_PATH (default: ip.txt) to enable proxy support."
                            .to_string(),
                    }],
                    is_error: true,
                }))
            }
        }
        "test" => {
            if let Some(proxy_manager) = &state.proxy_manager {
                let strict_proxy_health = arguments
                    .get("strict_proxy_health")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let target_url = arguments
                    .get("target_url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("http://httpbin.org/ip");

                let explicit_proxy = arguments
                    .get("proxy_url")
                    .and_then(|v| v.as_str())
                    .filter(|v| !v.trim().is_empty())
                    .map(|v| v.to_string());

                let mut candidates = Vec::new();
                let mut seen = HashSet::new();

                if let Some(proxy) = explicit_proxy {
                    if seen.insert(proxy.clone()) {
                        candidates.push(proxy);
                    }
                } else {
                    if let Some(current) = proxy_manager.get_current_proxy().await {
                        if seen.insert(current.clone()) {
                            candidates.push(current);
                        }
                    }

                    if let Ok(best) = proxy_manager.switch_to_best_proxy().await {
                        if seen.insert(best.clone()) {
                            candidates.push(best);
                        }
                    }

                    for proxy in proxy_manager.list_proxies(true).await.into_iter().take(5) {
                        if seen.insert(proxy.url.clone()) {
                            candidates.push(proxy.url);
                        }
                    }
                }

                if candidates.is_empty() {
                    return Ok(Json(McpCallResponse {
                        content: vec![McpContent {
                            content_type: "text".to_string(),
                            text: "No proxy available for testing. Add proxies via proxy_manager grab or provide proxy_url."
                                .to_string(),
                        }],
                        is_error: strict_proxy_health,
                    }));
                }

                let mut failures = Vec::new();

                for proxy_url in candidates {
                    match proxy_manager
                        .test_proxy_connection(&proxy_url, target_url)
                        .await
                    {
                        Ok(latency_ms) => {
                            if let Err(e) = proxy_manager
                                .record_proxy_result(&proxy_url, true, Some(latency_ms))
                                .await
                            {
                                warn!("Failed to record proxy test result: {}", e);
                            }

                            return Ok(Json(McpCallResponse {
                                content: vec![McpContent {
                                    content_type: "text".to_string(),
                                    text: format!(
                                        "✅ Proxy connection successful!\n\nProxy: {}\nLatency: {}ms\nTarget: {}\n\nProxy is ready for use.",
                                        format_proxy_display(&proxy_url),
                                        latency_ms,
                                        target_url
                                    ),
                                }],
                                is_error: false,
                            }));
                        }
                        Err(e) => {
                            if let Err(e2) = proxy_manager
                                .record_proxy_result(&proxy_url, false, None)
                                .await
                            {
                                warn!("Failed to record proxy failure: {}", e2);
                            }
                            failures.push(format!("{} -> {}", format_proxy_display(&proxy_url), e));
                        }
                    }
                }

                error!("Test proxy connection error: all attempts failed");
                Ok(Json(McpCallResponse {
                    content: vec![McpContent {
                        content_type: "text".to_string(),
                        text: format!(
                            "❌ Proxy connection failed for {} candidate(s)\nTarget: {}\nStrict proxy health: {}\n\n{}\n\nThis usually means the current proxy pool is offline/blocked. Try proxy_manager grab then retest.",
                            failures.len(),
                            target_url,
                            if strict_proxy_health { "enabled" } else { "disabled" },
                            failures.join("\n")
                        ),
                    }],
                    is_error: strict_proxy_health,
                }))
            } else {
                let strict_proxy_health = arguments
                    .get("strict_proxy_health")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                Ok(Json(McpCallResponse {
                    content: vec![McpContent {
                        content_type: "text".to_string(),
                        text: "Proxy manager not available. Provide IP_LIST_PATH (default: ip.txt) to enable proxy support."
                            .to_string(),
                    }],
                    is_error: strict_proxy_health,
                }))
            }
        }
        _ => Ok(Json(McpCallResponse {
            content: vec![McpContent {
                content_type: "text".to_string(),
                text: format!(
                    "Invalid action: {}. Use grab, list, status, switch, or test.",
                    action
                ),
            }],
            is_error: true,
        })),
    }
}
