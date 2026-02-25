use cortex_scout::stdio_service;

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
        println!("{}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("cortex-scout-mcp (MCP stdio server); usage: cortex-scout-mcp [--version|--help|--setup [--json]]");
        return Ok(());
    }

    if args.iter().any(|a| a == "--setup") {
        let opts = cortex_scout::setup::SetupOptions {
            mode: cortex_scout::setup::SetupRunMode::SetupFlag,
            ..Default::default()
        };
        let report = cortex_scout::setup::check_all(opts).await;
        let is_json = args.iter().any(|a| a == "--json");
        if is_json {
            println!(
                "{}",
                serde_json::to_string_pretty(&report).unwrap_or_else(|e| {
                    format!(r#"{{"error":"failed_to_serialize","details":"{}"}}"#, e)
                })
            );
        } else {
            println!("{}", report);
        }
        if !is_json {
            report.print_action_required_blocks();
        }
        if report.has_failures() {
            std::process::exit(2);
        }
        return Ok(());
    }
    stdio_service::run().await
}
