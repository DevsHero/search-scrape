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
            description: "Search the web using SearXNG federated search. AGENT GUIDANCE: (1) Set max_results=5-10 for quick lookups, 20-50 for comprehensive research. (2) Use time_range='week' or 'month' for recent topics. (3) Use categories='it' for tech, 'news' for current events, 'science' for research. (4) Check 'answers' field for instant facts before reading snippets. (5) If you see 'Did you mean' corrections, retry with the suggested spelling. (6) If unresponsive_engines > 3, consider retrying.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query. TIP: Use specific terms and quotes for exact phrases. Example: 'rust async' instead of just 'rust'"
                    },
                    "engines": {
                        "type": "string",
                        "description": "Comma-separated engines (e.g., 'google,bing'). TIP: Omit for default. Use 'google,bing' for English content, add 'duckduckgo' for privacy-focused results"
                    },
                    "categories": {
                        "type": "string",
                        "description": "Comma-separated categories. WHEN TO USE: 'it' for programming/tech, 'news' for current events, 'science' for research papers, 'general' for mixed. Omit for all categories"
                    },
                    "language": {
                        "type": "string",
                        "description": "Language code (e.g., 'en', 'es', 'fr'). TIP: Use 'en' for English-only results, omit for multilingual"
                    },
                    "safesearch": {
                        "type": "integer",
                        "minimum": 0,
                        "maximum": 2,
                        "description": "Safe search: 0=off, 1=moderate (recommended), 2=strict. Default env setting usually sufficient"
                    },
                    "time_range": {
                        "type": "string",
                        "description": "Filter by recency. WHEN TO USE: 'day' for breaking news, 'week' for current events, 'month' for recent tech/trends, 'year' for last 12 months. Omit for all-time results"
                    },
                    "pageno": {
                        "type": "integer",
                        "minimum": 1,
                        "description": "Page number for pagination. TIP: Start with page 1, use page 2+ only if initial results insufficient"
                    },
                    "max_results": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 100,
                        "default": 10,
                        "description": "Max results to return. GUIDANCE: 5-10 for quick facts, 15-25 for balanced research, 30-50 for comprehensive surveys. Default 10 is good for most queries. Higher = more tokens"
                    }
                },
                "required": ["query"]
            }),
        },
        McpTool {
            name: "scrape_url".to_string(),
            description: "Scrape and extract clean content from URLs. AGENT GUIDANCE: (1) Set max_chars=3000-5000 for summaries, 10000-20000 for full articles, 30000+ for documentation. (2) Keep content_links_only=true (default) to get only relevant links. (3) Check word_count - if <50, page may be JS-heavy or paywalled. (4) Use [N] citation markers in content to reference specific sources. (5) For docs sites, increase max_chars to capture full tutorials.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "Full URL to scrape. TIP: Works best with article/blog/docs pages. May have limited content for JS-heavy sites or paywalls"
                    },
                    "content_links_only": {
                        "type": "boolean",
                        "description": "Extract main content links only (true, default) or all page links (false). GUIDANCE: Keep true for articles/blogs to avoid nav clutter. Set false only when you need site-wide links like sitemaps",
                        "default": true
                    },
                    "max_links": {
                        "type": "integer",
                        "description": "Max links in Sources section. GUIDANCE: 20-30 for focused articles, 50-100 (default) for comprehensive pages, 200+ for navigation-heavy docs. Lower = faster response",
                        "minimum": 1,
                        "maximum": 500,
                        "default": 100
                    },
                    "max_chars": {
                        "type": "integer",
                        "description": "Max content length. WHEN TO ADJUST: 3000-5000 for quick summaries, 10000 (default) for standard articles, 20000-30000 for long-form content, 40000+ for full documentation pages. Truncated content shows a warning",
                        "minimum": 100,
                        "maximum": 50000,
                        "default": 10000
                    },
                    "output_format": {
                        "type": "string",
                        "enum": ["text", "json"],
                        "description": "Output format. 'text' (default) returns formatted markdown for humans. 'json' returns structured JSON for agents/parsing. AGENT TIP: Use 'json' to get extraction_score, truncated flag, code_blocks array, and all metadata as machine-readable fields",
                        "default": "text"
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
                Ok(mut content) => {
                    let max_chars = request.arguments
                        .get("max_chars")
                        .and_then(|v| v.as_u64())
                        .map(|n| n as usize)
                        .or_else(|| std::env::var("MAX_CONTENT_CHARS").ok().and_then(|s| s.parse().ok()))
                        .unwrap_or(10000);
                    
                    // Set truncation metadata (Priority 1)
                    content.actual_chars = content.clean_content.len();
                    content.max_chars_limit = Some(max_chars);
                    content.truncated = content.clean_content.len() > max_chars;
                    
                    if content.truncated {
                        content.warnings.push("content_truncated".to_string());
                    }
                    if content.word_count < 50 {
                        content.warnings.push("short_content".to_string());
                    }
                    if content.extraction_score.map(|s| s < 0.4).unwrap_or(false) {
                        content.warnings.push("low_extraction_score".to_string());
                    }
                    
                    // Check for output_format parameter (Priority 1)
                    let output_format = request.arguments
                        .get("output_format")
                        .and_then(|v| v.as_str())
                        .unwrap_or("text");
                    
                    if output_format == "json" {
                        // Return JSON format directly as text
                        let json_str = serde_json::to_string_pretty(&content)
                            .unwrap_or_else(|e| format!(r#"{{"error": "Failed to serialize: {}"}}"#, e));
                        return Ok(Json(McpCallResponse {
                            content: vec![McpContent {
                                content_type: "text".to_string(),
                                text: json_str,
                            }],
                            is_error: false,
                        }));
                    }
                    
                    // Otherwise return formatted text (backward compatible)
                    let content_text = {
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