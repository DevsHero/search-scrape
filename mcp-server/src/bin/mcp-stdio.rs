use search_scrape::stdio_service;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    // VS Code MCP host may probe stdio servers with `--version`/`--help`.
    // If we ignore args and start JSON-RPC transport instead, the host can
    // fail compatibility detection and cancel the session.
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        tracing::info!("version={}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        tracing::info!("search-scrape-mcp (MCP stdio server); usage: search-scrape-mcp [--version|--help]");
        return Ok(());
    }
    stdio_service::run().await
}