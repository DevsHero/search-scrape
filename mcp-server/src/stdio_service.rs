use rmcp::{model::*, ServiceExt};
use std::env;
use std::sync::Arc;
use tracing::{error, info};
use std::borrow::Cow;
use crate::{search, scrape, AppState};

#[derive(Clone, Debug)]
pub struct McpService {
    pub state: Arc<AppState>,
}

impl McpService {
    pub fn new() -> anyhow::Result<Self> {
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .init();

        let searxng_url = env::var("SEARXNG_URL")
            .unwrap_or_else(|_| "http://localhost:8888".to_string());
        
        info!("Starting MCP Service");
        info!("SearXNG URL: {}", searxng_url);

        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        let state = Arc::new(AppState {
            searxng_url,
            http_client,
            search_cache: moka::future::Cache::builder().max_capacity(10_000).time_to_live(std::time::Duration::from_secs(60 * 10)).build(),
            scrape_cache: moka::future::Cache::builder().max_capacity(10_000).time_to_live(std::time::Duration::from_secs(60 * 30)).build(),
            outbound_limit: Arc::new(tokio::sync::Semaphore::new(32)),
        });

        Ok(Self { state })
    }
}

impl rmcp::ServerHandler for McpService {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::LATEST,
            server_info: Implementation {
                name: "search-scrape".to_string(),
                version: "1.0.0".to_string(),
            },
            instructions: Some(
                "A pure Rust web search and scraping service using SearXNG for federated search and a native Rust scraper for content extraction.".to_string(),
            ),
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            ..Default::default()
        }
    }

    async fn list_tools(
        &self,
        _page: Option<PaginatedRequestParam>,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let tools = vec![
            Tool {
                name: Cow::Borrowed("search_web"),
                description: Some(Cow::Borrowed("Search the web using SearXNG federated search. AGENT GUIDANCE: (1) Set max_results=5-10 for quick lookups, 20-50 for comprehensive research. (2) Use time_range='week' or 'month' for recent topics. (3) Use categories='it' for tech, 'news' for current events, 'science' for research. (4) Check 'answers' field for instant facts before reading snippets. (5) If you see 'Did you mean' corrections, retry with the suggested spelling. (6) If unresponsive_engines > 3, consider retrying.")),
                input_schema: match serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {"type": "string", "description": "Search query. TIP: Use specific terms and quotes for exact phrases. Example: 'rust async' instead of just 'rust'"},
                        "engines": {"type": "string", "description": "Comma-separated engines (e.g., 'google,bing'). TIP: Omit for default. Use 'google,bing' for English content, add 'duckduckgo' for privacy-focused results"},
                        "categories": {"type": "string", "description": "Comma-separated categories. WHEN TO USE: 'it' for programming/tech, 'news' for current events, 'science' for research papers, 'general' for mixed. Omit for all categories"},
                        "language": {"type": "string", "description": "Language code (e.g., 'en', 'es', 'fr'). TIP: Use 'en' for English-only results, omit for multilingual"},
                        "safesearch": {"type": "integer", "minimum": 0, "maximum": 2, "description": "Safe search: 0=off, 1=moderate (recommended), 2=strict. Default env setting usually sufficient"},
                        "time_range": {"type": "string", "description": "Filter by recency. WHEN TO USE: 'day' for breaking news, 'week' for current events, 'month' for recent tech/trends, 'year' for last 12 months. Omit for all-time results"},
                        "pageno": {"type": "integer", "minimum": 1, "description": "Page number for pagination. TIP: Start with page 1, use page 2+ only if initial results insufficient"},
                        "max_results": {"type": "integer", "minimum": 1, "maximum": 100, "default": 10, "description": "Max results to return. GUIDANCE: 5-10 for quick facts, 15-25 for balanced research, 30-50 for comprehensive surveys. Default 10 is good for most queries. Higher = more tokens"}
                    },
                    "required": ["query"]
                }) {
                    serde_json::Value::Object(map) => std::sync::Arc::new(map),
                    _ => std::sync::Arc::new(serde_json::Map::new()),
                },
                output_schema: None,
                annotations: None,
            },
            Tool {
                name: Cow::Borrowed("scrape_url"),
                description: Some(Cow::Borrowed("Scrape and extract clean content from URLs. AGENT GUIDANCE: (1) Set max_chars=3000-5000 for summaries, 10000-20000 for full articles, 30000+ for documentation. (2) Keep content_links_only=true (default) to get only relevant links. (3) Check word_count - if <50, page may be JS-heavy or paywalled. (4) Use [N] citation markers in content to reference specific sources. (5) For docs sites, increase max_chars to capture full tutorials.")),
                input_schema: match serde_json::json!({
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
                        }
                    },
                    "required": ["url"]
                }) {
                    serde_json::Value::Object(map) => std::sync::Arc::new(map),
                    _ => std::sync::Arc::new(serde_json::Map::new()),
                },
                output_schema: None,
                annotations: None,
            },
        ];

        Ok(ListToolsResult {
            tools,
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParam,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        info!("MCP tool call: {} with args: {:?}", request.name, request.arguments);
        match request.name.as_ref() {
            "search_web" => {
                let args = request.arguments.as_ref().ok_or_else(|| ErrorData::new(
                    ErrorCode::INVALID_PARAMS,
                    "Missing required arguments object",
                    None,
                ))?;
                let query = args
                    .get("query")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ErrorData::new(
                        ErrorCode::INVALID_PARAMS,
                        "Missing required parameter: query",
                        None,
                    ))?;
                
                let engines = args.get("engines").and_then(|v| v.as_str()).map(|s| s.to_string());
                let categories = args.get("categories").and_then(|v| v.as_str()).map(|s| s.to_string());
                let language = args.get("language").and_then(|v| v.as_str()).map(|s| s.to_string());
                let time_range = args.get("time_range").and_then(|v| v.as_str()).map(|s| s.to_string());
                let safesearch = args.get("safesearch").and_then(|v| v.as_i64()).and_then(|n| if (0..=2).contains(&n) { Some(n as u8) } else { None });
                let pageno = args.get("pageno").and_then(|v| v.as_u64()).map(|n| n as u32);

                let max_results = args.get("max_results").and_then(|v| v.as_u64()).map(|n| n as usize).unwrap_or(10);
                let overrides = crate::search::SearchParamOverrides { engines, categories, language, safesearch, time_range, pageno };

                match search::search_web_with_params(&self.state, query, Some(overrides)).await {
                    Ok((results, extras)) => {
                        let content_text = if results.is_empty() {
                            let mut text = format!("No search results found for query: '{}'\n\n", query);
                            
                            // Show suggestions/corrections to help user refine query
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
                            let _showing = result_count.min(max_results);
                            
                            let mut text = format!("Found {} search results for '{}':", result_count, query);
                            if result_count > max_results {
                                text.push_str(&format!(" (showing top {})\n", max_results));
                            }
                            text.push_str("\n\n");
                            
                            // Show instant answers first if available
                            if !extras.answers.is_empty() {
                                text.push_str("**Instant Answers:**\n");
                                for answer in &extras.answers {
                                    text.push_str(&format!("📌 {}\n\n", answer));
                                }
                            }
                            
                            // Show search results
                            for (i, result) in limited_results.enumerate() {
                                text.push_str(&format!(
                                    "{}. **{}**\n   URL: {}\n   Snippet: {}\n\n",
                                    i + 1,
                                    result.title,
                                    result.url,
                                    result.content.chars().take(200).collect::<String>()
                                ));
                            }
                            
                            // Show helpful metadata at the end
                            if !extras.suggestions.is_empty() {
                                text.push_str(&format!("\n**Related searches:** {}\n", extras.suggestions.join(", ")));
                            }
                            if !extras.unresponsive_engines.is_empty() {
                                text.push_str(&format!("\n⚠️ **Note:** {} engine(s) did not respond (may affect completeness)\n", extras.unresponsive_engines.len()));
                            }
                            
                            text
                        };
                        
                        Ok(CallToolResult::success(vec![Content::text(content_text)]))
                    }
                    Err(e) => {
                        error!("Search tool error: {}", e);
                        Ok(CallToolResult::success(vec![Content::text(format!("Search failed: {}", e))]))
                    }
                }
            }
            "scrape_url" => {
                let args = request.arguments.as_ref().ok_or_else(|| ErrorData::new(
                    ErrorCode::INVALID_PARAMS,
                    "Missing required arguments object",
                    None,
                ))?;
                let url = args
                    .get("url")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ErrorData::new(
                        ErrorCode::INVALID_PARAMS,
                        "Missing required parameter: url",
                        None,
                    ))?;
                
                self.state.scrape_cache.invalidate(url).await;
                
                match scrape::scrape_url(&self.state, url).await {
                    Ok(content) => {
                        info!("Scraped content: {} words, {} chars clean_content", content.word_count, content.clean_content.len());
                        
                        let max_chars = args
                            .get("max_chars")
                            .and_then(|v| v.as_u64())
                            .map(|n| n as usize)
                            .or_else(|| std::env::var("MAX_CONTENT_CHARS").ok().and_then(|s| s.parse().ok()))
                            .unwrap_or(10000);
                        
                        let content_preview = if content.clean_content.is_empty() {
                            let msg = "[No content extracted]\n\n**Possible reasons:**\n\
                            • Page is JavaScript-heavy (requires browser execution)\n\
                            • Content is behind authentication/paywall\n\
                            • Site blocks automated access\n\n\
                            **Suggestion:** For JS-heavy sites, try using the Playwright MCP tool instead.";
                            msg.to_string()
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
                        
                        // Build Sources section from links
                        let sources_section = if content.links.is_empty() {
                            String::new()
                        } else {
                            let mut sources = String::from("\n\n**Sources:**\n");
                            // Get max_links from args or env var or default
                            let max_sources = args
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
                        
                        let content_text = format!(
                            "**{}**\n\nURL: {}\nWord Count: {}\nLanguage: {}\n\n**Content:**\n{}\n\n**Metadata:**\n- Description: {}\n- Keywords: {}\n\n**Headings:**\n{}\n\n**Links Found:** {}\n**Images Found:** {}{}",
                            content.title,
                            content.url,
                            content.word_count,
                            content.language,
                            content_preview,
                            content.meta_description,
                            content.meta_keywords,
                            content.headings.iter()
                                .map(|h| format!("- {} {}", h.level.to_uppercase(), h.text))
                                .collect::<Vec<_>>()
                                .join("\n"),
                            content.links.len(),
                            content.images.len(),
                            sources_section
                        );
                        
                        Ok(CallToolResult::success(vec![Content::text(content_text)]))
                    }
                    Err(e) => {
                        error!("Scrape tool error: {}", e);
                        Ok(CallToolResult::success(vec![Content::text(format!("Scraping failed: {}", e))]))
                    }
                }
            }
            _ => Err(ErrorData::new(
                ErrorCode::METHOD_NOT_FOUND,
                format!("Unknown tool: {}", request.name),
                None,
            )),
        }
    }
}

pub async fn run() -> anyhow::Result<()> {
    let service = McpService::new()?;
    let server = service.serve(rmcp::transport::stdio()).await?;
    info!("MCP stdio server running");
    let _quit_reason = server.waiting().await?;
    Ok(())
}