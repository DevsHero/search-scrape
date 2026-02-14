pub mod core;
pub mod features;
pub mod mcp;
pub mod nlp;
pub mod scraping;
pub mod setup;
pub mod tools;

// --- Primary core exports ---
pub use core::content_quality;
pub use core::types;
pub use core::types::*;
pub use core::AppState;

// --- Backwards-compatible module paths ---
pub use features::{antibot, history, non_robot_search, proxy_grabber, proxy_manager};
pub use mcp::handlers as mcp_handlers;
pub use mcp::stdio as stdio_service;
pub use mcp::tooling as mcp_tooling;
pub use nlp::{query_rewriter, rerank};
pub use scraping::rust_scraper;
pub use setup as shadow_setup;
pub use tools::{batch_scrape, crawl, extract, scrape, search};
