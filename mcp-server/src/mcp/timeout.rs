use super::{McpCallResponse, McpContent};
use serde_json::json;
use std::time::Duration;

pub fn timeout_call_response(tool_name: &str, timeout: Duration) -> McpCallResponse {
    let body = json!({
        "status": "timeout",
        "tool_name": tool_name,
        "timeout_seconds": timeout.as_secs(),
        "message": format!(
            "Tool execution exceeded the hard timeout of {} seconds and was cancelled to prevent a stuck MCP session.",
            timeout.as_secs()
        ),
        "retryable": true,
        "suggested_action": "Retry with a narrower scope or raise CORTEX_SCOUT_TOOL_TIMEOUT_SECS[_TOOL] if this workload legitimately needs more time."
    });

    McpCallResponse {
        content: vec![McpContent {
            content_type: "text".to_string(),
            text: serde_json::to_string_pretty(&body).unwrap_or_else(|_| body.to_string()),
        }],
        is_error: true,
    }
}
