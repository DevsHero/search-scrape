use cortex_scout::AppState;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init()
        .ok();

    let target_min: usize = std::env::args()
        .nth(1)
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(150);

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

    let state = Arc::new(AppState::new(http_client));

    // This uses the same logic as the MCP tool `proxy_control` action=grab.
    let result = cortex_scout::features::proxy_grabber::grab_proxies(
        &state,
        cortex_scout::features::proxy_grabber::GrabParams {
            limit: Some(target_min),
            proxy_type: Some("http".to_string()),
            random: false,
            store_ip_txt: true,
            clear_ip_txt: false,
            append: true,
        },
    )
    .await?;

    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
