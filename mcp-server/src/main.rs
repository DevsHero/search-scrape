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
use tracing::{info, warn, error};

use shadowcrawl::{search, scrape, types::*, mcp, AppState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    // Get configuration from environment
    let searxng_url = env::var("SEARXNG_URL")
        .unwrap_or_else(|_| "http://localhost:8888".to_string());
    
    info!("Starting MCP Server");
    info!("SearXNG URL: {}", searxng_url);

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
    let mut state = AppState::new(searxng_url, http_client);

    // Initialize memory if QDRANT_URL is set
    if let Ok(qdrant_url) = env::var("QDRANT_URL") {
        info!("Initializing memory with Qdrant at: {}", qdrant_url);
        match shadowcrawl::history::MemoryManager::new(&qdrant_url).await {
            Ok(memory) => {
                state = state.with_memory(Arc::new(memory));
                info!("Memory initialized successfully");
            }
            Err(e) => {
                warn!("Failed to initialize memory: {}. Continuing without memory feature.", e);
            }
        }
    } else {
        info!("QDRANT_URL not set. Memory feature disabled.");
    }

    // Initialize proxy manager if ip.txt exists
    let ip_list_path = env::var("IP_LIST_PATH").unwrap_or_else(|_| "ip.txt".to_string());

    if tokio::fs::metadata(&ip_list_path).await.is_ok() {
        info!("Loading proxy manager from IP list: {}", ip_list_path);
        match shadowcrawl::proxy_manager::ProxyManager::new(&ip_list_path).await {
            Ok(proxy_manager) => {
                let status = proxy_manager.get_status().await?;
                state = state.with_proxy_manager(Arc::new(proxy_manager));
                info!("Proxy manager initialized: {} total proxies, {} enabled", 
                      status.total_proxies, status.enabled_proxies);
            }
            Err(e) => {
                warn!("Failed to initialize proxy manager: {}. Continuing without proxy support.", e);
            }
        }
    } else {
        info!("IP list not found at {}. Proxy feature disabled.", ip_list_path);
    }

    let state = Arc::new(state);

    // Build router
    let app = Router::new()
        .route("/", get(health_check))
        .route("/health", get(health_check))
        .route("/search", post(search_web_handler))
        .route("/search_structured", post(search_structured_handler))
        .route("/scrape", post(scrape_url_handler))
        .route("/chat", post(chat_handler))
        .route("/mcp/tools", get(mcp::list_tools))
        .route("/mcp/call", post(mcp::call_tool))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    // Start server
    let listener = tokio::net::TcpListener::bind("0.0.0.0:5000").await?;
    info!("MCP Server listening on http://0.0.0.0:5000");
    
    axum::serve(listener, app).await?;
    
    Ok(())
}

async fn health_check() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "healthy",
        "service": "shadowcrawl",
        "version": env!("CARGO_PKG_VERSION")
    }))
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
    let top_n = std::env::var("CHAT_SCRAPE_TOP_N").ok().and_then(|v| v.parse::<usize>().ok()).unwrap_or(5);
    let to_scrape: Vec<String> = search_results.iter().take(top_n).map(|r| r.url.clone()).collect();
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
        let content_summary = scraped_content.iter()
            .map(|c| format!(
                "• {} ({} words, {}m)\n  {}\n  URL: {}\n",
                c.title,
                c.word_count,
                c.reading_time_minutes.unwrap_or(((c.word_count as f64 / 200.0).ceil() as u32).max(1)),
                c.meta_description,
                c.canonical_url.as_ref().unwrap_or(&c.url)
            ))
            .collect::<Vec<_>>()
            .join("\n---\n");
        
        format!("Based on my search for '{}', I found the following information:\n\n{}", 
            request.query, content_summary)
    };
    
    Ok(Json(ChatResponse {
        response: response_text,
        search_results,
        scraped_content,
    }))
}