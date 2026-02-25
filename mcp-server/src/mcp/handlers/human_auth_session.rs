/// `human_auth_session` — the Auth Specialist.
///
/// This is the Auth-Gatekeeper's escalation endpoint.  It extends the core HITL
/// browser tool (`non_robot_search`) with auth-specific parameters:
///
/// * `instruction_message` — displayed in the browser overlay so the user knows
///   exactly which service to log in to and why.
/// * `keep_open` — leaves the browser window open after content is extracted so
///   the user can keep browsing if needed.
/// * Automatic session-cookie persistence: after the user completes auth, cookies
///   are saved to `~/.cortex-scout/sessions/{domain}.json` so future requests to
///   the same domain can reuse the session without another HITL interruption.
///
/// In the **Autonomous Auth-Handling Protocol** this tool is invoked in Step 3
/// (HITL Phase) only after `web_fetch` returned `auth_risk_score >= 0.4` and
/// `visual_scout` confirmed the presence of a login page.
use super::common::parse_quality_mode;
use crate::mcp::{McpCallResponse, McpContent};
use crate::types::ErrorResponse;
use axum::http::StatusCode;
use axum::response::Json;
use serde_json::Value;
use std::sync::Arc;

use crate::AppState;

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

    let output_format = arguments
        .get("output_format")
        .and_then(|v| v.as_str())
        .unwrap_or("json");

    let max_chars = arguments
        .get("max_chars")
        .and_then(|v| v.as_u64())
        .map(|n| n as usize)
        .or_else(|| {
            std::env::var("MAX_CONTENT_CHARS")
                .ok()
                .and_then(|s| s.parse().ok())
        })
        .unwrap_or(10000);

    let use_proxy = arguments
        .get("use_proxy")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let quality_mode = parse_quality_mode(arguments)?;

    let captcha_grace_seconds = arguments
        .get("captcha_grace_seconds")
        .and_then(|v| v.as_u64())
        .unwrap_or(5);

    let human_timeout_seconds = arguments
        .get("human_timeout_seconds")
        .and_then(|v| v.as_u64())
        .unwrap_or(1200);

    #[cfg(feature = "non_robot_search")]
    {
        let user_profile_path = arguments
            .get("user_profile_path")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let auto_scroll = arguments
            .get("auto_scroll")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let wait_for_selector = arguments
            .get("wait_for_selector")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let keep_open = arguments
            .get("keep_open")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let instruction_message = arguments
            .get("instruction_message")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let cfg = crate::features::non_robot_search::NonRobotSearchConfig {
            url: url.to_string(),
            max_chars,
            use_proxy,
            quality_mode,
            captcha_grace: std::time::Duration::from_secs(captcha_grace_seconds),
            human_timeout: std::time::Duration::from_secs(human_timeout_seconds),
            user_profile_path,
            auto_scroll,
            wait_for_selector,
            keep_open,
            instruction_message,
        };

        match crate::features::non_robot_search::execute_manual_auth_flow(&state, cfg).await {
            Ok(mut content) => {
                crate::content_quality::apply_scrape_content_limit(&mut content, max_chars, false);

                if output_format == "text" {
                    return Ok(Json(McpCallResponse {
                        content: vec![McpContent {
                            content_type: "text".to_string(),
                            text: content.clean_content,
                        }],
                        is_error: false,
                    }));
                }

                let json_str = serde_json::to_string_pretty(&content).unwrap_or_else(|e| {
                    format!(r#"{{\"error\": \"Failed to serialize: {}\"}}"#, e)
                });
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
                    text: format!("human_auth_session failed: {}", e),
                }],
                is_error: true,
            })),
        }
    }

    #[cfg(not(feature = "non_robot_search"))]
    {
        let _ = (
            state,
            url,
            output_format,
            max_chars,
            use_proxy,
            quality_mode,
            captcha_grace_seconds,
            human_timeout_seconds,
        );
        Ok(Json(McpCallResponse {
            content: vec![McpContent {
                content_type: "text".to_string(),
                text: "human_auth_session is not enabled in this running binary (feature flag: `non_robot_search`). Rebuild and restart using a build with the `non_robot_search` feature, for example: `cd mcp-server && cargo build --release --features non_robot_search --bin cortex-scout --bin cortex-scout-mcp`. If you're using VS Code MCP stdio, restart the MCP server after rebuilding.".to_string(),
            }],
            is_error: true,
        }))
    }
}
