pub mod search;
pub mod scrape;
pub mod crawl;
pub mod extract;
pub mod batch_scrape;
pub mod types;
pub mod mcp;
pub mod rust_scraper;
pub mod stdio_service;
pub mod history;
pub mod query_rewriter;

#[derive(Clone)]
pub struct AppState {
    pub searxng_url: String,
    pub http_client: reqwest::Client,
    // Caches for performance
    pub search_cache: moka::future::Cache<String, Vec<types::SearchResult>>, // key: query
    pub scrape_cache: moka::future::Cache<String, types::ScrapeResponse>,     // key: url
    // Concurrency control for external calls
    pub outbound_limit: std::sync::Arc<tokio::sync::Semaphore>,
    // Memory manager for research history (optional)
    pub memory: Option<std::sync::Arc<history::MemoryManager>>,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("searxng_url", &self.searxng_url)
            .field("memory_enabled", &self.memory.is_some())
            .finish()
    }
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
            memory: None, // Will be initialized if QDRANT_URL is set
        }
    }

    pub fn with_memory(mut self, memory: std::sync::Arc<history::MemoryManager>) -> Self {
        self.memory = Some(memory);
        self
    }
}