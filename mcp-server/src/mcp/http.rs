use super::handlers;
use super::tooling::tool_catalog;
use crate::types::*;
use crate::AppState;
use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
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

pub async fn list_tools() -> Json<McpToolsResponse> {
    let tools = tool_catalog()
        .into_iter()
        .map(|tool| McpTool {
            name: tool.name.to_string(),
            description: tool.description.to_string(),
            input_schema: tool.input_schema,
        })
        .collect();
    Json(McpToolsResponse { tools })
}

pub async fn call_tool(
    State(state): State<Arc<AppState>>,
    Json(request): Json<McpCallRequest>,
) -> Result<Json<McpCallResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("MCP tool call: {} with args: {:?}", request.name, request.arguments);

    match request.name.as_str() {
        "search_web" => handlers::search_web::handle(state, &request.arguments).await,
        "search_structured" => handlers::search_structured::handle(state, &request.arguments).await,
        "scrape_url" => handlers::scrape_url::handle(state, &request.arguments).await,
        "crawl_website" => handlers::crawl_website::handle(state, &request.arguments).await,
        "scrape_batch" => handlers::scrape_batch::handle(state, &request.arguments).await,
        "extract_structured" => handlers::extract_structured::handle(state, &request.arguments).await,
        "research_history" => handlers::research_history::handle(state, &request.arguments).await,
        "proxy_manager" => handlers::proxy_manager::handle(state, &request.arguments).await,
        _ => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!("Unknown tool: {}", request.name),
            }),
        )),
    }
}
