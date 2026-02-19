use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use std::env;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::{error, info, warn};

use shadowcrawl::{mcp, scrape, search, types::*, AppState};

fn parse_port_from_args() -> Option<u16> {
    let mut args = std::env::args().peekable();
    while let Some(a) = args.next() {
        if a == "--port" {
            if let Some(v) = args.next() {
                if let Ok(p) = v.parse::<u16>() {
                    return Some(p);
                }
            }
        } else if let Some(rest) = a.strip_prefix("--port=") {
            if let Ok(p) = rest.parse::<u16>() {
                return Some(p);
            }
        }
    }
    None
}

fn port_from_env() -> Option<u16> {
    for k in ["SHADOWCRAWL_PORT", "PORT"] {
        if let Ok(v) = std::env::var(k) {
            if let Ok(p) = v.trim().parse::<u16>() {
                return Some(p);
            }
        }
    }
    None
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,tower_http=warn"));
    tracing_subscriber::fmt().with_env_filter(env_filter).init();

    // Handle setup-only mode
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--setup") {
        let opts = shadowcrawl::setup::SetupOptions {
            mode: shadowcrawl::setup::SetupRunMode::SetupFlag,
            ..Default::default()
        };
        let report = shadowcrawl::setup::check_all(opts).await;
        println!("{}", report);
        report.print_action_required_blocks();
        if report.has_failures() {
            std::process::exit(2);
        }
        return Ok(());
    }

    info!("Starting MCP Server");

    // Pre-flight checklist (non-interactive) at startup
    let report = shadowcrawl::setup::check_all(shadowcrawl::setup::SetupOptions::default()).await;
    info!("{}", report.summarize_for_logs());
    if report.has_failures() {
        warn!("shadow-setup: startup checklist found failures; run with --setup for guided remediation");
        report.print_action_required_blocks();
    }

    // Create HTTP client
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

    // Create application state
    let mut state = AppState::new(http_client);

    // Initialize semantic memory if LANCEDB_URI is set
    if let Some(lancedb_uri) = shadowcrawl::core::config::lancedb_uri() {
        info!("Initializing memory with LanceDB at: {}", lancedb_uri);
        match shadowcrawl::history::MemoryManager::new(&lancedb_uri).await {
            Ok(memory) => {
                state = state.with_memory(Arc::new(memory));
                info!("Memory initialized successfully");
            }
            Err(e) => {
                warn!(
                    "Failed to initialize memory: {}. Continuing without memory feature.",
                    e
                );
            }
        }
    } else {
        info!("LANCEDB_URI not set. Memory feature disabled.");
    }

    // Initialize proxy manager if ip.txt exists
    let ip_list_path = env::var("IP_LIST_PATH").unwrap_or_else(|_| "ip.txt".to_string());

    if tokio::fs::metadata(&ip_list_path).await.is_ok() {
        info!("Loading proxy manager from IP list: {}", ip_list_path);
        match shadowcrawl::proxy_manager::ProxyManager::new(&ip_list_path).await {
            Ok(proxy_manager) => {
                let status = proxy_manager.get_status().await?;
                state = state.with_proxy_manager(Arc::new(proxy_manager));
                info!(
                    "Proxy manager initialized: {} total proxies, {} enabled",
                    status.total_proxies, status.enabled_proxies
                );
            }
            Err(e) => {
                warn!(
                    "Failed to initialize proxy manager: {}. Continuing without proxy support.",
                    e
                );
            }
        }
    } else {
        info!(
            "IP list not found at {}. Proxy feature disabled.",
            ip_list_path
        );
    }

    let state = Arc::new(state);

    // Build router
    let app = Router::new()
        .route("/", get(health_check))
        .route("/health", get(health_check))
        .route("/.well-known/mcp/server-card.json", get(server_card))
        .route("/mcp", post(mcp_rpc_handler))
        .route("/search", post(search_web_handler))
        .route("/search_structured", post(search_structured_handler))
        .route("/scrape", post(scrape_url_handler))
        .route("/chat", post(chat_handler))
        .route("/mcp/tools", get(mcp::list_tools))
        .route("/mcp/call", post(mcp::call_tool))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state.clone());

    // Start server
    let port: u16 = parse_port_from_args()
        .or_else(port_from_env)
        .unwrap_or(5000);
    let bind_addr = format!("0.0.0.0:{}", port);
    let listener = match tokio::net::TcpListener::bind(&bind_addr).await {
        Ok(l) => l,
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            anyhow::bail!(
                "Address already in use: {}. Stop the existing process or run with --port {} (or set PORT/SHADOWCRAWL_PORT).",
                bind_addr,
                port.saturating_add(1)
            )
        }
        Err(e) => return Err(e.into()),
    };
    info!("MCP Server listening on http://{}", bind_addr);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(state.clone()))
        .await?;

    Ok(())
}

async fn shutdown_signal(state: Arc<AppState>) {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigterm = signal(SignalKind::terminate()).ok();
        let mut sigint = signal(SignalKind::interrupt()).ok();

        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = async {
                if let Some(ref mut s) = sigterm {
                    s.recv().await;
                } else {
                    futures::future::pending::<()>().await;
                }
            } => {},
            _ = async {
                if let Some(ref mut s) = sigint {
                    s.recv().await;
                } else {
                    futures::future::pending::<()>().await;
                }
            } => {},
        }
    }

    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }

    if let Some(pool) = state.browser_pool.as_ref() {
        pool.shutdown().await;
    }
}

async fn health_check() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "healthy",
        "service": "shadowcrawl",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

async fn server_card(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let tools: Vec<serde_json::Value> = state
        .tool_registry
        .public_specs()
        .into_iter()
        .map(|spec| {
            serde_json::json!({
                "name": spec.public_name,
                "description": spec.public_description
            })
        })
        .collect();

    Json(serde_json::json!({
        "serverInfo": {
            "name": "ShadowCrawl",
            "version": env!("CARGO_PKG_VERSION")
        },
        "tools": tools,
        "resources": [],
        "prompts": []
    }))
}

async fn mcp_rpc_handler(
    State(state): State<Arc<AppState>>,
    Json(request): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let id = request
        .get("id")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let method = request
        .get("method")
        .and_then(|m| m.as_str())
        .unwrap_or_default();

    match method {
        "initialize" => Json(serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "protocolVersion": "2025-11-05",
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": "ShadowCrawl",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }
        })),
        "tools/list" => {
            let tools = mcp::http::list_tools_for_state(state.as_ref());
            Json(serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": tools
            }))
        }
        _ => Json(serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": -32601,
                "message": "Method not found"
            }
        })),
    }
}

async fn search_web_handler(
    State(state): State<Arc<AppState>>,
    Json(request): Json<SearchRequest>,
) -> Result<Json<SearchResponse>, (StatusCode, Json<ErrorResponse>)> {
    match search::search_web(&state, &request.query).await {
        Ok((results, _extras)) => Ok(Json(SearchResponse { results })),
        Err(e) => {
            error!("Search error: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: e.to_string(),
                }),
            ))
        }
    }
}

async fn scrape_url_handler(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ScrapeRequest>,
) -> Result<Json<ScrapeResponse>, (StatusCode, Json<ErrorResponse>)> {
    match scrape::scrape_url(&state, &request.url).await {
        Ok(content) => Ok(Json(content)),
        Err(e) => {
            error!("Scrape error: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: e.to_string(),
                }),
            ))
        }
    }
}

async fn search_structured_handler(
    State(state): State<Arc<AppState>>,
    Json(request): Json<SearchStructuredRequest>,
) -> Result<Json<SearchStructuredResponse>, (StatusCode, Json<ErrorResponse>)> {
    let (results, _extras) = search::search_web(&state, &request.query)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Search failed: {}", e),
                }),
            )
        })?;

    let top_n = request.top_n.unwrap_or(3);
    let to_scrape: Vec<String> = results.iter().take(top_n).map(|r| r.url.clone()).collect();

    let mut scraped_content = Vec::new();
    let mut tasks = Vec::new();
    for url in to_scrape {
        let state_cloned = Arc::clone(&state);
        tasks.push(tokio::spawn(async move {
            (url.clone(), scrape::scrape_url(&state_cloned, &url).await)
        }));
    }

    for task in tasks {
        match task.await {
            Ok((_, Ok(content))) => scraped_content.push(content),
            Ok((url, Err(e))) => warn!("Structured scrape failed for {}: {}", url, e),
            Err(e) => warn!("Structured scrape task join error: {}", e),
        }
    }

    Ok(Json(SearchStructuredResponse {
        results,
        scraped_content,
    }))
}

async fn chat_handler(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ChatRequest>,
) -> Result<Json<ChatResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Processing chat request: {}", request.query);

    let search_results = match search::search_web(&state, &request.query).await {
        Ok((results, _extras)) => results,
        Err(e) => {
            error!("Search failed: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Search failed: {}", e),
                }),
            ));
        }
    };

    info!("Found {} search results", search_results.len());

    // Step 2: Scrape top results concurrently (limit to 5)
    let top_n = std::env::var("CHAT_SCRAPE_TOP_N")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(5);
    let to_scrape: Vec<String> = search_results
        .iter()
        .take(top_n)
        .map(|r| r.url.clone())
        .collect();
    let mut scraped_content = Vec::new();
    let mut tasks = Vec::new();
    for url in to_scrape {
        let state_cloned = Arc::clone(&state);
        tasks.push(tokio::spawn(async move {
            (url.clone(), scrape::scrape_url(&state_cloned, &url).await)
        }));
    }
    for task in tasks {
        match task.await {
            Ok((url, Ok(content))) => {
                info!("Successfully scraped: {}", url);
                scraped_content.push(content);
            }
            Ok((url, Err(e))) => {
                warn!("Failed to scrape {}: {}", url, e);
            }
            Err(e) => warn!("Scrape task join error: {}", e),
        }
    }

    // Step 3: Generate response based on scraped content
    let response_text = if scraped_content.is_empty() {
        format!("I found {} search results for '{}', but couldn't scrape any content. Here are the URLs:\n{}", 
            search_results.len(),
            request.query,
            search_results.iter().map(|r| format!("- {} ({})", r.title, r.url)).collect::<Vec<_>>().join("\n")
        )
    } else {
        let content_summary = scraped_content
            .iter()
            .map(|c| {
                format!(
                    "• {} ({} words, {}m)\n  {}\n  URL: {}\n",
                    c.title,
                    c.word_count,
                    c.reading_time_minutes
                        .unwrap_or(((c.word_count as f64 / 200.0).ceil() as u32).max(1)),
                    c.meta_description,
                    c.canonical_url.as_ref().unwrap_or(&c.url)
                )
            })
            .collect::<Vec<_>>()
            .join("\n---\n");

        format!(
            "Based on my search for '{}', I found the following information:\n\n{}",
            request.query, content_summary
        )
    };

    Ok(Json(ChatResponse {
        response: response_text,
        search_results,
        scraped_content,
    }))
}
