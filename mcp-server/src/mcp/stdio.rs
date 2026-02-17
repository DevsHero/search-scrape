use crate::mcp::http::{list_tools_for_state, McpCallRequest};
use crate::{history, AppState};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Json;
use serde_json::{json, Value};
use std::env;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{info, warn};

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

fn jsonrpc_error(id: &Value, code: i64, message: impl Into<String>) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message.into()
        }
    })
}

fn jsonrpc_result(id: &Value, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    })
}

async fn handle_tools_list(service: &McpService, id: &Value) -> Value {
    let tools = list_tools_for_state(service.state.as_ref());
    match serde_json::to_value(tools) {
        Ok(v) => jsonrpc_result(id, v),
        Err(e) => jsonrpc_error(id, -32603, format!("failed to serialize tools: {}", e)),
    }
}

async fn handle_tools_call(service: &McpService, id: &Value, params: &Value) -> Value {
    let name = params.get("name").and_then(|v| v.as_str());
    let arguments = params.get("arguments");

    let Some(name) = name else {
        return jsonrpc_error(id, -32602, "Missing required field: params.name");
    };
    let Some(arguments) = arguments else {
        return jsonrpc_error(id, -32602, "Missing required field: params.arguments");
    };

    let request = McpCallRequest {
        name: name.to_string(),
        arguments: arguments.clone(),
    };

    let result = crate::mcp::call_tool(State(Arc::clone(&service.state)), Json(request)).await;
    match result {
        Ok(Json(response)) => match serde_json::to_value(response) {
            Ok(v) => jsonrpc_result(id, v),
            Err(e) => jsonrpc_error(id, -32603, format!("failed to serialize result: {}", e)),
        },
        Err((status, Json(err))) => {
            // Map HTTP-ish errors to JSON-RPC codes.
            let code = match status {
                StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY => -32602,
                StatusCode::NOT_FOUND => -32601,
                _ => -32603,
            };
            jsonrpc_error(id, code, err.error)
        }
    }
}

pub async fn run() -> anyhow::Result<()> {
    let service = McpService::new().await?;
    info!("MCP stdio server initialized; waiting for client session");

    let stdin = tokio::io::stdin();
    let mut lines = BufReader::new(stdin).lines();
    let mut stdout = tokio::io::stdout();

    let mut has_initialize = false;
    let mut is_initialized = false;
    let mut shutdown_requested = false;

    while let Some(line) = lines.next_line().await? {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let Ok(msg) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };

        let method = msg.get("method").and_then(|v| v.as_str()).unwrap_or("");
        let id = msg.get("id").cloned().unwrap_or(Value::Null);
        let is_request = msg.get("id").is_some();
        let params = msg.get("params").cloned().unwrap_or(Value::Null);

        // Notifications
        if !is_request {
            match method {
                // MCP: client sends this after it receives initialize response.
                // Some clients send params omitted or {}, accept both.
                "initialized" => {
                    has_initialize = true;
                    is_initialized = true;
                    continue;
                }
                "exit" => {
                    if shutdown_requested {
                        break;
                    }
                    continue;
                }
                _ => continue,
            }
        }

        // Requests
        let response = match method {
            "initialize" => {
                has_initialize = true;
                // Do not mark initialized until we get the notification.
                let server_info = json!({
                    "name": "shadowcrawl",
                    "title": "Search & Sync MCP",
                    "version": env!("CARGO_PKG_VERSION")
                });
                jsonrpc_result(
                    &id,
                    json!({
                        "protocolVersion": "2024-11-05",
                        "capabilities": {"tools": {}},
                        "serverInfo": server_info
                    }),
                )
            }
            "shutdown" => {
                shutdown_requested = true;
                jsonrpc_result(&id, Value::Null)
            }
            "tools/list" => {
                if !has_initialize || !is_initialized {
                    jsonrpc_error(&id, -32002, "Server not initialized")
                } else {
                    handle_tools_list(&service, &id).await
                }
            }
            "tools/call" => {
                if !has_initialize || !is_initialized {
                    jsonrpc_error(&id, -32002, "Server not initialized")
                } else {
                    handle_tools_call(&service, &id, &params).await
                }
            }
            _ => jsonrpc_error(&id, -32601, format!("Method not found: {}", method)),
        };

        let out = serde_json::to_string(&response).unwrap_or_else(|e| {
            serde_json::to_string(&jsonrpc_error(&id, -32603, format!("serialize error: {}", e)))
                .unwrap_or_else(|_| "{\"jsonrpc\":\"2.0\",\"id\":null,\"error\":{\"code\":-32603,\"message\":\"serialize error\"}}".to_string())
        });

        stdout.write_all(out.as_bytes()).await?;
        stdout.write_all(b"\n").await?;
        stdout.flush().await?;
    }

    warn!("MCP stdio server stopped");
    Ok(())
}
