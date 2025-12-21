pub mod search;
pub mod scrape;
pub mod types;
pub mod mcp;
pub mod rust_scraper;
pub mod stdio_service;

#[derive(Clone, Debug)]
pub struct AppState {
    pub searxng_url: String,
    pub http_client: reqwest::Client,
    // Caches for performance
    pub search_cache: moka::future::Cache<String, Vec<types::SearchResult>>, // key: query
    pub scrape_cache: moka::future::Cache<String, types::ScrapeResponse>,     // key: url
    // Concurrency control for external calls
    pub outbound_limit: std::sync::Arc<tokio::sync::Semaphore>,
}

// Re-export AppState for easy access
pub use types::*;

impl AppState {
    pub fn new(searxng_url: String, http_client: reqwest::Client) -> Self {
        Self {
            searxng_url,
            http_client,
            search_cache: moka::future::Cache::builder()
                .max_capacity(10_000)
                .time_to_live(std::time::Duration::from_secs(60 * 10))
                .build(),
            scrape_cache: moka::future::Cache::builder()
                .max_capacity(10_000)
                .time_to_live(std::time::Duration::from_secs(60 * 30))
                .build(),
            outbound_limit: std::sync::Arc::new(tokio::sync::Semaphore::new(32)),
        }
    }
}