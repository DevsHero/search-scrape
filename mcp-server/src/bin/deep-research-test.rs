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
        eprintln!("  IP_LIST_PATH=ip.txt (optional, enables proxy manager)");
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

    let ip_list_path = std::env::var("IP_LIST_PATH").unwrap_or_else(|_| "ip.txt".to_string());
    if tokio::fs::metadata(&ip_list_path).await.is_ok() {
        match shadowcrawl::proxy_manager::ProxyManager::new(&ip_list_path).await {
            Ok(pm) => state = state.with_proxy_manager(Arc::new(pm)),
            Err(e) => eprintln!("WARN: failed to init proxy manager: {e}"),
        }
    }

    let config = DeepResearchConfig::default();
    let result = deep_research::deep_research(Arc::new(state), query, config).await?;

    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
