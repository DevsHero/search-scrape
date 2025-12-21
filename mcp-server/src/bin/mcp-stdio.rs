use mcp_server::stdio_service;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    stdio_service::run().await
}