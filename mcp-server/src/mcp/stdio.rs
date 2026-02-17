use super::handlers;
use super::tooling::schema_to_object_map;
use crate::mcp::McpCallResponse;
use crate::types::ErrorResponse;
use crate::{history, AppState};
use axum::http::StatusCode;
use axum::response::Json;
use rmcp::{model::*, ServiceExt};
use serde_json::Value;
use std::borrow::Cow;
use std::env;
use std::sync::Arc;
use tracing::{info, warn};

fn status_code_to_error_code(status: StatusCode) -> ErrorCode {
    match status {
        StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY => ErrorCode::INVALID_PARAMS,
        StatusCode::NOT_FOUND => ErrorCode::METHOD_NOT_FOUND,
        _ => ErrorCode::INTERNAL_ERROR,
    }
}

fn mcp_call_response_to_stdio_result(response: McpCallResponse) -> CallToolResult {
    let content = response
        .content
        .into_iter()
        .map(|item| Content::text(item.text))
        .collect();

    if response.is_error {
        CallToolResult::error(content)
    } else {
        CallToolResult::success(content)
    }
}

fn convert_http_handler_result(
    result: Result<Json<McpCallResponse>, (StatusCode, Json<ErrorResponse>)>,
) -> Result<CallToolResult, ErrorData> {
    match result {
        Ok(Json(response)) => Ok(mcp_call_response_to_stdio_result(response)),
        Err((status, Json(err))) => Err(ErrorData::new(
            status_code_to_error_code(status),
            err.error,
            None,
        )),
    }
}

#[derive(Clone, Debug)]
pub struct McpService {
    pub state: Arc<AppState>,
}

impl McpService {
    pub async fn new() -> anyhow::Result<Self> {
        tracing_subscriber::fmt()
            .with_writer(std::io::stderr)
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init()
            .ok();

        let searxng_url =
            env::var("SEARXNG_URL").unwrap_or_else(|_| "http://localhost:8888".to_string());

        // Pre-flight checklist (non-interactive) at startup
        let report = crate::setup::check_all(crate::setup::SetupOptions::default()).await;
        info!("{}", report.summarize_for_logs());
        if report.has_failures() {
            warn!("shadow-setup: startup checklist found failures; run shadowcrawl-mcp --setup for guided remediation");
            report.print_action_required_blocks();
        }

        info!("Starting MCP Service");
        info!("SearXNG URL: {}", searxng_url);

        let http_timeout = env::var("HTTP_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(30);
        let connect_timeout = env::var("HTTP_CONNECT_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(10);

        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(http_timeout))
            .connect_timeout(std::time::Duration::from_secs(connect_timeout))
            .build()?;

        let mut state = AppState::new(searxng_url, http_client);

        if let Ok(lancedb_uri) = env::var("LANCEDB_URI") {
            info!("Initializing memory with LanceDB at: {}", lancedb_uri);
            match history::MemoryManager::new(&lancedb_uri).await {
                Ok(memory) => {
                    state = state.with_memory(Arc::new(memory));
                    info!("Memory initialized successfully");
                }
                Err(e) => warn!(
                    "Failed to initialize memory: {}. Continuing without memory.",
                    e
                ),
            }
        } else {
            info!("LANCEDB_URI not set. Memory feature disabled.");
        }

        let ip_list_path = env::var("IP_LIST_PATH").unwrap_or_else(|_| "ip.txt".to_string());
        if tokio::fs::metadata(&ip_list_path).await.is_ok() {
            info!("Loading proxy manager from IP list: {}", ip_list_path);
            match crate::proxy_manager::ProxyManager::new(&ip_list_path).await {
                Ok(proxy_manager) => {
                    let status = proxy_manager.get_status().await?;
                    state = state.with_proxy_manager(Arc::new(proxy_manager));
                    info!(
                        "Proxy manager initialized: {} total proxies, {} enabled",
                        status.total_proxies, status.enabled_proxies
                    );
                }
                Err(e) => warn!("Failed to initialize proxy manager: {}", e),
            }
        } else {
            info!(
                "IP list not found at {}. Proxy feature disabled.",
                ip_list_path
            );
        }

        Ok(Self {
            state: Arc::new(state),
        })
    }
}

impl rmcp::ServerHandler for McpService {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::LATEST,
            server_info: Implementation {
                title: Some("Search & Sync MCP".to_string()),
                description: Some(
                    "A pure Rust web research service using federated search plus high-integrity content synchronization for consistent downstream analysis."
                        .to_string(),
                ),
                ..Implementation::from_build_env()
            },
            instructions: Some(
                "Use these tools to discover sources and synchronize web content into consistent, analysis-ready outputs."
                    .to_string(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
        }
    }

    async fn list_tools(
        &self,
        _page: Option<PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let tools = self
            .state
            .tool_registry
            .public_specs()
            .into_iter()
            .map(|spec| Tool {
                name: Cow::Owned(spec.public_name),
                title: Some(spec.public_title),
                description: Some(Cow::Owned(spec.public_description)),
                input_schema: schema_to_object_map(&spec.public_input_schema),
                output_schema: None,
                annotations: None,
                execution: None,
                icons: None,
                meta: None,
            })
            .collect();

        Ok(ListToolsResult {
            tools,
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        info!(
            "MCP tool call: {} with args: {:?}",
            request.name, request.arguments
        );

        let args_map = request.arguments.as_ref().ok_or_else(|| {
            ErrorData::new(
                ErrorCode::INVALID_PARAMS,
                "Missing required arguments object",
                None,
            )
        })?;

        let internal_name = self
            .state
            .tool_registry
            .resolve_incoming_tool_name(request.name.as_ref())
            .ok_or_else(|| {
                ErrorData::new(
                    ErrorCode::METHOD_NOT_FOUND,
                    format!("Unknown tool: {}", request.name),
                    None,
                )
            })?;

        // rmcp stdio arguments are an object map; mcp_handlers expect a serde_json::Value.
        // Clone is fine here (tool inputs are small) and keeps handler API consistent across transports.
        let public_args = Value::Object(args_map.clone());
        let internal_args = self
            .state
            .tool_registry
            .map_public_arguments_to_internal(&internal_name, public_args);

        match internal_name.as_str() {
            "search_web" => convert_http_handler_result(
                handlers::search_web::handle(Arc::clone(&self.state), &internal_args).await,
            ),
            "search_structured" => convert_http_handler_result(
                handlers::search_structured::handle(Arc::clone(&self.state), &internal_args).await,
            ),
            "scrape_url" => convert_http_handler_result(
                handlers::scrape_url::handle(Arc::clone(&self.state), &internal_args).await,
            ),
            "crawl_website" => convert_http_handler_result(
                handlers::crawl_website::handle(Arc::clone(&self.state), &internal_args).await,
            ),
            "scrape_batch" => convert_http_handler_result(
                handlers::scrape_batch::handle(Arc::clone(&self.state), &internal_args).await,
            ),
            "extract_structured" => convert_http_handler_result(
                handlers::extract_structured::handle(Arc::clone(&self.state), &internal_args).await,
            ),
            "research_history" => convert_http_handler_result(
                handlers::research_history::handle(Arc::clone(&self.state), &internal_args).await,
            ),
            "proxy_manager" => convert_http_handler_result(
                handlers::proxy_manager::handle(Arc::clone(&self.state), &internal_args).await,
            ),
            "non_robot_search" => convert_http_handler_result(
                handlers::non_robot_search::handle(Arc::clone(&self.state), &internal_args).await,
            ),
            _ => Err(ErrorData::new(
                ErrorCode::METHOD_NOT_FOUND,
                format!("Unknown tool: {}", request.name),
                None,
            )),
        }
    }
}

pub async fn run() -> anyhow::Result<()> {
    let service = McpService::new().await?;
    let running = service.serve(rmcp::transport::stdio()).await?;
    info!("MCP stdio server initialized; waiting for client session");
    let quit_reason = running.waiting().await?;
    warn!("MCP stdio server stopped: {:?}", quit_reason);
    Ok(())
}
