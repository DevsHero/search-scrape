use shadowcrawl::{deep_research, AppState};
use shadowcrawl::deep_research::DeepResearchConfig;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init()
        .ok();

    let query = std::env::args().skip(1).collect::<Vec<_>>().join(" ");
    if query.trim().is_empty() {
        eprintln!("Usage: deep-research-test <query>");
        eprintln!("\nEnv:");
        eprintln!("  LANCEDB_URI=... (optional, enables semantic shave)");
        eprintln!("  IP_LIST_PATH=... (optional, enables proxy manager; default: ../ip.txt)");
        eprintln!("  DEEP_RESEARCH_USE_PROXY=1 (optional, route scraping through proxy)");
        eprintln!("  DEEP_RESEARCH_DEPTH=2 (optional)");
        eprintln!("  DEEP_RESEARCH_MAX_SOURCES=10 (optional)");
        eprintln!("  OPENAI_API_KEY=... (optional, enables real LLM synthesis)");
        eprintln!("  DEEP_RESEARCH_LLM_MODEL=gpt-4o-mini (optional)");
        std::process::exit(2);
    }

    let http_timeout = std::env::var("HTTP_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(45);
    let connect_timeout = std::env::var("HTTP_CONNECT_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(10);

    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(http_timeout))
        .connect_timeout(std::time::Duration::from_secs(connect_timeout))
        .build()?;

    let mut state = AppState::new(http_client);

    if let Some(lancedb_uri) = shadowcrawl::core::config::lancedb_uri() {
        if !lancedb_uri.contains("://") {
            let _ = tokio::fs::create_dir_all(&lancedb_uri).await;
        }
        match shadowcrawl::history::MemoryManager::new(&lancedb_uri).await {
            Ok(memory) => state = state.with_memory(Arc::new(memory)),
            Err(e) => eprintln!("WARN: failed to init memory: {e}"),
        }
    }

    // Prefer repo-root ip.txt when running from `mcp-server/`.
    let default_ip_list = {
        let manifest_dir = env!("CARGO_MANIFEST_DIR"); // .../ShadowCrawl/mcp-server
        let path = std::path::Path::new(manifest_dir).join("..").join("ip.txt");
        path.to_string_lossy().to_string()
    };
    let ip_list_path = std::env::var("IP_LIST_PATH").unwrap_or(default_ip_list);
    if tokio::fs::metadata(&ip_list_path).await.is_ok() {
        match shadowcrawl::proxy_manager::ProxyManager::new(&ip_list_path).await {
            Ok(pm) => state = state.with_proxy_manager(Arc::new(pm)),
            Err(e) => eprintln!("WARN: failed to init proxy manager: {e}"),
        }
    }

    let mut config = DeepResearchConfig::default();
    config.use_proxy = std::env::var("DEEP_RESEARCH_USE_PROXY")
        .ok()
        .is_some_and(|v| v.trim() == "1" || v.trim().eq_ignore_ascii_case("true"));

    if let Ok(v) = std::env::var("DEEP_RESEARCH_DEPTH") {
        if let Ok(n) = v.parse::<u8>() {
            config.depth = n;
        }
    }
    if let Ok(v) = std::env::var("DEEP_RESEARCH_MAX_SOURCES") {
        if let Ok(n) = v.parse::<usize>() {
            config.max_sources_per_hop = n;
        }
    }

    let result = deep_research::deep_research(Arc::new(state), query, config).await?;

    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
