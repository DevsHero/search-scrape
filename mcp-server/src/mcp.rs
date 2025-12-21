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
            description: "Search the web using SearXNG federated search engine. Returns results with answers, suggestions, and spelling corrections to help refine queries. Supports engines, categories, language, safesearch, time_range, pageno, and max_results.".to_string(),
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
                    },
                    "max_results": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 100,
                        "default": 10,
                        "description": "Maximum number of results to return (default: 10, max: 100)"
                    }
                },
                "required": ["query"]
            }),
        },
        McpTool {
            name: "scrape_url".to_string(),
            description: "Scrape content from a URL with intelligent extraction. Returns cleaned text, metadata, structured data, and clickable source citations. Automatically filters noise (ads, nav, footers) and extracts main content links.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "The URL to scrape content from"
                    },
                    "content_links_only": {
                        "type": "boolean",
                        "description": "If true, only extract links from main content area (article/main). If false, extract all document links. Default: true (smart filtering)",
                        "default": true
                    },
                    "max_links": {
                        "type": "integer",
                        "description": "Maximum number of links to return in Sources section. Default: 100",
                        "minimum": 1,
                        "maximum": 500,
                        "default": 100
                    },
                    "max_chars": {
                        "type": "integer",
                        "description": "Maximum characters to return in content preview. Useful to control token usage. Default: 10000",
                        "minimum": 100,
                        "maximum": 50000,
                        "default": 10000
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
            
            let max_results = request.arguments
                .get("max_results")
                .and_then(|v| v.as_u64())
                .map(|n| n as usize)
                .unwrap_or(10);
            
            // Perform search
            let ov_opt = Some(overrides);
            match search::search_web_with_params(&state, query, ov_opt).await {
                Ok((results, extras)) => {
                    let content_text = if results.is_empty() {
                        let mut text = format!("No search results found for query: '{}'\n\n", query);
                        
                        if !extras.suggestions.is_empty() {
                            text.push_str(&format!("**Suggestions:** {}\n", extras.suggestions.join(", ")));
                        }
                        if !extras.corrections.is_empty() {
                            text.push_str(&format!("**Did you mean:** {}\n", extras.corrections.join(", ")));
                        }
                        if !extras.unresponsive_engines.is_empty() {
                            text.push_str(&format!("\n**Note:** {} search engine(s) did not respond. Try different engines or retry.\n", extras.unresponsive_engines.len()));
                        }
                        text
                    } else {
                        let limited_results = results.iter().take(max_results);
                        let result_count = results.len();
                        
                        let mut text = format!("Found {} search results for '{}':", result_count, query);
                        if result_count > max_results {
                            text.push_str(&format!(" (showing top {})\n", max_results));
                        }
                        text.push_str("\n\n");
                        
                        if !extras.answers.is_empty() {
                            text.push_str("**Instant Answers:**\n");
                            for answer in &extras.answers {
                                text.push_str(&format!("📌 {}\n\n", answer));
                            }
                        }
                        
                        for (i, result) in limited_results.enumerate() {
                            text.push_str(&format!(
                                "{}. **{}**\n   URL: {}\n   Snippet: {}\n\n",
                                i + 1,
                                result.title,
                                result.url,
                                result.content.chars().take(200).collect::<String>()
                            ));
                        }
                        
                        if !extras.suggestions.is_empty() {
                            text.push_str(&format!("\n**Related searches:** {}\n", extras.suggestions.join(", ")));
                        }
                        if !extras.unresponsive_engines.is_empty() {
                            text.push_str(&format!("\n⚠️ **Note:** {} engine(s) did not respond (may affect completeness)\n", extras.unresponsive_engines.len()));
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
                        let max_chars = request.arguments
                            .get("max_chars")
                            .and_then(|v| v.as_u64())
                            .map(|n| n as usize)
                            .or_else(|| std::env::var("MAX_CONTENT_CHARS").ok().and_then(|s| s.parse().ok()))
                            .unwrap_or(10000);
                        
                        let content_preview = if content.clean_content.is_empty() {
                            "[No content extracted]\n\n**Possible reasons:**\n\
                            • Page is JavaScript-heavy (requires browser execution)\n\
                            • Content is behind authentication/paywall\n\
                            • Site blocks automated access\n\n\
                            **Suggestion:** For JS-heavy sites, try using the Playwright MCP tool instead.".to_string()
                        } else if content.word_count < 10 {
                            format!("{}\n\n⚠️ **Very short content** ({} words). Page may be mostly dynamic/JS-based.", 
                                content.clean_content.chars().take(max_chars).collect::<String>(),
                                content.word_count)
                        } else {
                            let preview = content.clean_content.chars().take(max_chars).collect::<String>();
                            if content.clean_content.len() > max_chars {
                                format!("{}\n\n[Content truncated: {}/{} chars shown. Increase max_chars parameter to see more]",
                                    preview, max_chars, content.clean_content.len())
                            } else {
                                preview
                            }
                        };
                        
                        let headings = content.headings.iter()
                            .take(10)
                            .map(|h| format!("- {} {}", h.level.to_uppercase(), h.text))
                            .collect::<Vec<_>>()
                            .join("\n");
                        
                        // Build Sources section from links
                        let sources_section = if content.links.is_empty() {
                            String::new()
                        } else {
                            let mut sources = String::from("\n\nSources:\n");
                            // Get max_links from args or env var or default
                            let max_sources = request.arguments
                                .get("max_links")
                                .and_then(|v| v.as_u64())
                                .map(|n| n as usize)
                                .or_else(|| std::env::var("MAX_LINKS").ok().and_then(|s| s.parse().ok()))
                                .unwrap_or(100);
                            let link_count = content.links.len();
                            for (i, link) in content.links.iter().take(max_sources).enumerate() {
                                if !link.text.is_empty() {
                                    sources.push_str(&format!("[{}]: {} ({})", i + 1, link.url, link.text));
                                } else {
                                    sources.push_str(&format!("[{}]: {}", i + 1, link.url));
                                }
                                sources.push('\n');
                            }
                            if link_count > max_sources {
                                sources.push_str(&format!("\n(Showing {} of {} total links)\n", max_sources, link_count));
                            }
                            sources
                        };
                        
                        format!(
                            "{}\nURL: {}\nCanonical: {}\nWord Count: {} ({}m)\nLanguage: {}\nSite: {}\nAuthor: {}\nPublished: {}\n\nDescription: {}\nOG Image: {}\n\nHeadings:\n{}\n\nLinks: {}  Images: {}\n\nPreview:\n{}{}",
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
                            content_preview,
                            sources_section
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