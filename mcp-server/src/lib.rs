pub mod core;
pub mod features;
pub mod mcp;
pub mod nlp;
pub mod scraping;
pub mod setup;
pub mod tools;

pub fn build_env_filter(default_directives: &str) -> tracing_subscriber::EnvFilter {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(default_directives));

    filter
        .add_directive("chromiumoxide::handler=off".parse().expect("valid directive"))
        .add_directive("chromiumoxide::browser=off".parse().expect("valid directive"))
        .add_directive("html5ever=error".parse().expect("valid directive"))
        .add_directive("lance_index::vector::kmeans=error".parse().expect("valid directive"))
        .add_directive("lance::dataset::scanner=error".parse().expect("valid directive"))
        .add_directive("chromiumoxide=warn".parse().expect("valid directive"))
}

// --- Primary core exports ---
pub use core::content_quality;
pub use core::types;
pub use core::types::*;
pub use core::AppState;

// --- Backwards-compatible module paths ---
pub use features::{
    antibot, history, non_robot_search, proxy_grabber, proxy_manager, visual_scout,
};
pub use mcp::handlers as mcp_handlers;
pub use mcp::stdio as stdio_service;
pub use mcp::tooling as mcp_tooling;
pub use nlp::{query_rewriter, rerank};
pub use scraping::rust_scraper;
pub use setup as shadow_setup;
pub use tools::{batch_scrape, crawl, deep_research, extract, scrape, search};
