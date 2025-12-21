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
                description: Some(Cow::Borrowed("Search the web using SearXNG federated search engine. Supports optional parameters: engines, categories, language, safesearch, time_range, pageno.")),
                input_schema: match serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {"type": "string", "description": "The search query to execute"},
                        "engines": {"type": "string", "description": "Comma-separated list of engines (overrides env SEARXNG_ENGINES)"},
                        "categories": {"type": "string", "description": "Comma-separated categories (e.g., general, news, it)"},
                        "language": {"type": "string", "description": "Language code (e.g., en, en-US)"},
                        "safesearch": {"type": "integer", "minimum": 0, "maximum": 2, "description": "0=off, 1=moderate, 2=strict"},
                        "time_range": {"type": "string", "description": "Filter by time (e.g., day, week, month, year)"},
                        "pageno": {"type": "integer", "minimum": 1, "description": "Page number (1..N)"}
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
                description: Some(Cow::Borrowed("Scrape content from a URL with intelligent extraction. Returns cleaned text, metadata, structured data, and clickable source citations. Automatically filters noise (ads, nav, footers) and extracts main content links.")),
                input_schema: match serde_json::json!({
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

                let overrides = crate::search::SearchParamOverrides { engines, categories, language, safesearch, time_range, pageno };

                match search::search_web_with_params(&self.state, query, Some(overrides)).await {
                    Ok(results) => {
                        let content_text = if results.is_empty() {
                            format!("No search results found for query: {}", query)
                        } else {
                            let mut text = format!("Found {} search results for '{}':\n\n", results.len(), query);
                            for (i, result) in results.iter().enumerate() {
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
                        
                        let content_preview = if content.clean_content.is_empty() {
                            "[No content extracted - this may indicate a parsing issue]".to_string()
                        } else {
                            content.clean_content.chars().take(2000).collect::<String>()
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