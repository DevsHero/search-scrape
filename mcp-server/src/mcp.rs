use crate::types::*;
use crate::{search, scrape, AppState};
use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, error};

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
    let tools = vec![
        McpTool {
            name: "search_web".to_string(),
            description: "Search the web using SearXNG federated search engine. Supports engines, categories, language, safesearch, time_range, and pageno. Returns a list of relevant URLs with titles and snippets.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The search query to execute"
                    },
                    "engines": {
                        "type": "string",
                        "description": "Comma-separated list of engines (e.g., 'google,bing,duckduckgo')"
                    },
                    "categories": {
                        "type": "string",
                        "description": "Comma-separated list of categories (e.g., 'general,news,it,science')"
                    },
                    "language": {
                        "type": "string",
                        "description": "Language code (e.g., 'en', 'en-US')"
                    },
                    "safesearch": {
                        "type": "integer",
                        "minimum": 0,
                        "maximum": 2,
                        "description": "Safe search level: 0 (off), 1 (moderate), 2 (strict)"
                    },
                    "time_range": {
                        "type": "string",
                        "description": "Time filter (e.g., 'day', 'week', 'month', 'year')"
                    },
                    "pageno": {
                        "type": "integer",
                        "minimum": 1,
                        "description": "Page number for pagination"
                    }
                },
                "required": ["query"]
            }),
        },
        McpTool {
            name: "scrape_url".to_string(),
            description: "Scrape content from a specific URL using a Rust-native scraper. Returns cleaned text content, metadata, and structured data.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "The URL to scrape content from"
                    }
                },
                "required": ["url"]
            }),
        },
    ];
    
    Json(McpToolsResponse { tools })
}

pub async fn call_tool(
    State(state): State<Arc<AppState>>,
    Json(request): Json<McpCallRequest>,
) -> Result<Json<McpCallResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("MCP tool call: {} with args: {:?}", request.name, request.arguments);
    
    match request.name.as_str() {
        "search_web" => {
            // Extract query from arguments
            let query = request.arguments
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
            // Optional SearXNG overrides
            let mut overrides = search::SearchParamOverrides::default();
            if let Some(v) = request.arguments.get("engines").and_then(|v| v.as_str()) {
                if !v.is_empty() { overrides.engines = Some(v.to_string()); }
            }
            if let Some(v) = request.arguments.get("categories").and_then(|v| v.as_str()) {
                if !v.is_empty() { overrides.categories = Some(v.to_string()); }
            }
            if let Some(v) = request.arguments.get("language").and_then(|v| v.as_str()) {
                if !v.is_empty() { overrides.language = Some(v.to_string()); }
            }
            if let Some(v) = request.arguments.get("time_range").and_then(|v| v.as_str()) {
                overrides.time_range = Some(v.to_string());
            }
            if let Some(v) = request.arguments.get("safesearch").and_then(|v| v.as_u64()) {
                overrides.safesearch = Some(v as u8);
            }
            if let Some(v) = request.arguments.get("pageno").and_then(|v| v.as_u64()) {
                overrides.pageno = Some(v as u32);
            }
            
            // Perform search
            let ov_opt = Some(overrides);
            match search::search_web_with_params(&state, query, ov_opt).await {
                Ok(results) => {
                    let content_text = if results.is_empty() {
                        format!("No search results found for query: {}", query)
                    } else {
                        let mut text = format!("Found {} search results for '{}':\n\n", results.len(), query);
                        for (i, result) in results.iter().take(10).enumerate() {
                            text.push_str(&format!(
                                "{}. **{}**\n   URL: {}\n   Snippet: {}\n\n",
                                i + 1,
                                result.title,
                                result.url,
                                result.content.chars().take(200).collect::<String>()
                            ));
                        }
                        text
                    };
                    
                    Ok(Json(McpCallResponse {
                        content: vec![McpContent {
                            content_type: "text".to_string(),
                            text: content_text,
                        }],
                        is_error: false,
                    }))
                }
                Err(e) => {
                    error!("Search tool error: {}", e);
                    Ok(Json(McpCallResponse {
                        content: vec![McpContent {
                            content_type: "text".to_string(),
                            text: format!("Search failed: {}", e),
                        }],
                        is_error: true,
                    }))
                }
            }
        }
        "scrape_url" => {
            // Extract URL from arguments
            let url = request.arguments
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
            
            // Perform scraping - only Rust-native path
            match scrape::scrape_url(&state, url).await {
                Ok(content) => {
                    let content_text = {
                        let headings = content.headings.iter()
                            .take(10)
                            .map(|h| format!("- {} {}", h.level.to_uppercase(), h.text))
                            .collect::<Vec<_>>()
                            .join("\n");
                        format!(
                            "{}\nURL: {}\nCanonical: {}\nWord Count: {} ({}m)\nLanguage: {}\nSite: {}\nAuthor: {}\nPublished: {}\n\nDescription: {}\nOG Image: {}\n\nHeadings:\n{}\n\nLinks: {}  Images: {}\n\nPreview:\n{}",
                            content.title,
                            content.url,
                            content.canonical_url.as_deref().unwrap_or("-"),
                            content.word_count,
                            content.reading_time_minutes.unwrap_or(((content.word_count as f64 / 200.0).ceil() as u32).max(1)),
                            content.language,
                            content.site_name.as_deref().unwrap_or("-"),
                            content.author.as_deref().unwrap_or("-"),
                            content.published_at.as_deref().unwrap_or("-"),
                            content.meta_description,
                            content.og_image.as_deref().unwrap_or("-"),
                            headings,
                            content.links.len(),
                            content.images.len(),
                            content.clean_content.chars().take(1200).collect::<String>()
                        )
                    };
                    
                    Ok(Json(McpCallResponse {
                        content: vec![McpContent {
                            content_type: "text".to_string(),
                            text: content_text,
                        }],
                        is_error: false,
                    }))
                }
                Err(e) => {
                    error!("Scrape tool error: {}", e);
                    Ok(Json(McpCallResponse {
                        content: vec![McpContent {
                            content_type: "text".to_string(),
                            text: format!("Scraping failed: {}", e),
                        }],
                        is_error: true,
                    }))
                }
            }
        }
        _ => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!("Unknown tool: {}", request.name),
            }),
        )),
    }
}