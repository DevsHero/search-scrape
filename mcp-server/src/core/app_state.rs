use std::env;

#[derive(Clone)]
pub struct AppState {
    pub searxng_url: String,
    pub http_client: reqwest::Client,
    pub tool_registry: std::sync::Arc<crate::core::tools_registry::ToolRegistry>,
    // Caches for performance
    pub search_cache: moka::future::Cache<String, Vec<super::types::SearchResult>>, // key: query
    pub scrape_cache: moka::future::Cache<String, super::types::ScrapeResponse>,    // key: url
    // Concurrency control for external calls
    pub outbound_limit: std::sync::Arc<tokio::sync::Semaphore>,
    // Memory manager for research history (optional)
    pub memory: Option<std::sync::Arc<crate::history::MemoryManager>>,
    // Proxy manager for dynamic IP rotation (optional)
    pub proxy_manager: Option<std::sync::Arc<crate::proxy_manager::ProxyManager>>,

    // Serialize high-fidelity browser sessions to avoid Chromium profile lock conflicts.
    pub non_robot_search_lock: std::sync::Arc<tokio::sync::Mutex<()>>,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("searxng_url", &self.searxng_url)
            .field("memory_enabled", &self.memory.is_some())
            .field("proxy_manager_enabled", &self.proxy_manager.is_some())
            .finish()
    }
}

impl AppState {
    pub fn new(searxng_url: String, http_client: reqwest::Client) -> Self {
        let outbound_limit = env::var("OUTBOUND_LIMIT")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(32);

        let tool_registry = std::sync::Arc::new(crate::core::tools_registry::ToolRegistry::load());
        Self {
            searxng_url,
            http_client,
            tool_registry,
            search_cache: moka::future::Cache::builder()
                .max_capacity(10_000)
                .time_to_live(std::time::Duration::from_secs(60 * 10))
                .build(),
            scrape_cache: moka::future::Cache::builder()
                .max_capacity(10_000)
                .time_to_live(std::time::Duration::from_secs(60 * 30))
                .build(),
            outbound_limit: std::sync::Arc::new(tokio::sync::Semaphore::new(outbound_limit)),
            memory: None,        // Will be initialized if QDRANT_URL is set
            proxy_manager: None, // Will be initialized if IP_LIST_PATH exists
            non_robot_search_lock: std::sync::Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    pub fn with_memory(mut self, memory: std::sync::Arc<crate::history::MemoryManager>) -> Self {
        self.memory = Some(memory);
        self
    }

    pub fn with_proxy_manager(
        mut self,
        proxy_manager: std::sync::Arc<crate::proxy_manager::ProxyManager>,
    ) -> Self {
        self.proxy_manager = Some(proxy_manager);
        self
    }
}
