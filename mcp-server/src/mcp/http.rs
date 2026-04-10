use super::handlers;
use crate::types::*;
use crate::AppState;
use axum::{extract::State, http::StatusCode, response::Json};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::info;

#[derive(Debug, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub title: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub icons: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct McpToolsResponse {
    pub tools: Vec<McpTool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct McpCallRequest {
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct McpCallResponse {
    pub content: Vec<McpContent>,
    #[serde(rename = "isError")]
    pub is_error: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct McpContent {
    #[serde(rename = "type")]
    pub content_type: String,
    pub text: String,
}

pub(crate) fn instrument_tool_response(
    mut response: McpCallResponse,
    tool_name: &str,
    started_at: Instant,
) -> McpCallResponse {
    let elapsed = started_at.elapsed();
    let metrics_json = json!({
        "tool_name": tool_name,
        "total_duration_ms": elapsed.as_millis() as u64,
        "total_duration_seconds": elapsed.as_secs_f64()
    });

    for item in &mut response.content {
        if item.content_type != "text" {
            continue;
        }

        if let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&item.text) {
            if let Some(obj) = value.as_object_mut() {
                obj.insert("_tool_metrics".to_string(), metrics_json.clone());
                item.text = serde_json::to_string_pretty(&value).unwrap_or_else(|_| item.text.clone());
                continue;
            }
        }

        if !item.text.contains("Tool timing:") {
            item.text.push_str(&format!(
                "\n\nTool timing: {:.3}s ({} ms)",
                elapsed.as_secs_f64(),
                elapsed.as_millis() as u64
            ));
        }
    }

    response
}

pub fn list_tools_for_state(state: &AppState) -> McpToolsResponse {
    let tools = state
        .tool_registry
        .public_specs()
        .into_iter()
        .map(|spec| McpTool {
            name: spec.public_name,
            title: spec.public_title,
            description: spec.public_description,
            input_schema: spec.public_input_schema,
            icons: spec.icons,
        })
        .collect();

    McpToolsResponse { tools }
}

pub async fn list_tools(State(state): State<Arc<AppState>>) -> Json<McpToolsResponse> {
    Json(list_tools_for_state(state.as_ref()))
}

/// Core dispatch logic — called by both the axum route handler and the JSON-RPC handler.
pub async fn call_tool_inner(
    state: Arc<AppState>,
    request: McpCallRequest,
) -> Result<McpCallResponse, (StatusCode, Json<ErrorResponse>)> {
    let tool_start = Instant::now();
    let request_name = request.name.clone();
    info!(
        "MCP tool call: {} with args: {:?}",
        request_name, request.arguments
    );

    let internal_name = state
        .tool_registry
        .resolve_incoming_tool_name(&request_name)
        .or_else(|| match request_name.as_str() {
            "scout_browser_automate" => Some("browser_automate".to_string()),
            "scout_browser_close" => Some("browser_close".to_string()),
            "scout_agent_profile_auth" => Some("agent_profile_auth".to_string()),
            _ => None,
        })
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("Unknown tool: {}", request_name),
                }),
            )
        })?;

    let internal_args = state
        .tool_registry
        .map_public_arguments_to_internal(&internal_name, request.arguments);

    let tool_timeout = Duration::from_secs(crate::core::config::mcp_tool_timeout_secs(&internal_name));
    let dispatch_name = internal_name.clone();
    let state_for_dispatch = Arc::clone(&state);
    let request_name_for_dispatch = request_name.clone();

    let dispatch = async move {
        match dispatch_name.as_str() {
            "search_web" => handlers::search_web::handle(state_for_dispatch, &internal_args).await,
            "search_structured" => handlers::search_structured::handle(state_for_dispatch, &internal_args).await,
            "scrape_url" => handlers::scrape_url::handle(state_for_dispatch, &internal_args).await,
            "crawl_website" => handlers::crawl_website::handle(state_for_dispatch, &internal_args).await,
            "scrape_batch" => handlers::scrape_batch::handle(state_for_dispatch, &internal_args).await,
            "deep_research" => handlers::deep_research::handle(state_for_dispatch, &internal_args).await,
            "extract_structured" => handlers::extract_structured::handle(state_for_dispatch, &internal_args).await,
            "fetch_then_extract" => handlers::fetch_then_extract::handle(state_for_dispatch, &internal_args).await,
            "research_history" => handlers::research_history::handle(state_for_dispatch, &internal_args).await,
            "proxy_manager" => handlers::proxy_manager::handle(state_for_dispatch, &internal_args).await,
            "non_robot_search" => handlers::non_robot_search::handle(state_for_dispatch, &internal_args).await,
            "visual_scout" => handlers::visual_scout::handle(state_for_dispatch, &internal_args).await,
            "human_auth_session" => handlers::human_auth_session::handle(state_for_dispatch, &internal_args).await,
            "browser_automate" | "scout_browser_automate" => {
                handlers::automate::handle(state_for_dispatch, &internal_args).await
            }
            "browser_close" | "scout_browser_close" => {
                handlers::automate::handle_close(state_for_dispatch, &internal_args).await
            }
            "agent_profile_auth" | "scout_agent_profile_auth" => {
                handlers::automate::handle_profile_auth(state_for_dispatch, &internal_args).await
            }
            _ => Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("Unknown tool: {}", request_name_for_dispatch),
                }),
            )),
        }
    };

    match tokio::time::timeout(tool_timeout, dispatch).await {
        Ok(result) => result.map(|Json(r)| instrument_tool_response(r, &request_name, tool_start)),
        Err(_) => Ok(instrument_tool_response(
            super::timeout::timeout_call_response(&request_name, tool_timeout),
            &request_name,
            tool_start,
        )),
    }
}

/// Axum route handler: `POST /mcp/call`
pub async fn call_tool(
    State(state): State<Arc<AppState>>,
    Json(request): Json<McpCallRequest>,
) -> Result<Json<McpCallResponse>, (StatusCode, Json<ErrorResponse>)> {
    call_tool_inner(state, request).await.map(Json)
}
