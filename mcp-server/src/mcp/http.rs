use super::handlers;
use crate::types::*;
use crate::AppState;
use axum::{extract::State, http::StatusCode, response::Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub title: String,
    pub description: String,
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
    pub is_error: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct McpContent {
    #[serde(rename = "type")]
    pub content_type: String,
    pub text: String,
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

pub async fn call_tool(
    State(state): State<Arc<AppState>>,
    Json(request): Json<McpCallRequest>,
) -> Result<Json<McpCallResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!(
        "MCP tool call: {} with args: {:?}",
        request.name, request.arguments
    );

    let internal_name = state
        .tool_registry
        .resolve_incoming_tool_name(&request.name)
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("Unknown tool: {}", request.name),
                }),
            )
        })?;

    let internal_args = state
        .tool_registry
        .map_public_arguments_to_internal(&internal_name, request.arguments);

    match internal_name.as_str() {
        "search_web" => handlers::search_web::handle(state, &internal_args).await,
        "search_structured" => handlers::search_structured::handle(state, &internal_args).await,
        "scrape_url" => handlers::scrape_url::handle(state, &internal_args).await,
        "crawl_website" => handlers::crawl_website::handle(state, &internal_args).await,
        "scrape_batch" => handlers::scrape_batch::handle(state, &internal_args).await,
        "extract_structured" => handlers::extract_structured::handle(state, &internal_args).await,
        "fetch_then_extract" => handlers::fetch_then_extract::handle(state, &internal_args).await,
        "research_history" => handlers::research_history::handle(state, &internal_args).await,
        "proxy_manager" => handlers::proxy_manager::handle(state, &internal_args).await,
        "non_robot_search" => handlers::non_robot_search::handle(state, &internal_args).await,
        "visual_scout" => handlers::visual_scout::handle(state, &internal_args).await,
        "human_auth_session" => handlers::human_auth_session::handle(state, &internal_args).await,
        _ => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!("Unknown tool: {}", request.name),
            }),
        )),
    }
}
