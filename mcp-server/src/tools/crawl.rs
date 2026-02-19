use crate::rust_scraper::QualityMode;
use crate::types::*;
use crate::AppState;
use anyhow::Result;
use futures::stream::{self, StreamExt};
use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
use tracing::{info, warn};
use url::Url;

/// Configuration for website crawling
#[derive(Clone)]
pub struct CrawlConfig {
    pub max_depth: usize,
    pub max_pages: usize,
    pub max_concurrent: usize,
    pub same_domain_only: bool,
    pub include_patterns: Vec<String>,
    pub exclude_patterns: Vec<String>,
    pub max_chars_per_page: usize,
    pub quality_mode: String,
}

impl Default for CrawlConfig {
    fn default() -> Self {
        Self {
            max_depth: 3,
            max_pages: 50,
            max_concurrent: 5,
            same_domain_only: true,
            include_patterns: vec![],
            exclude_patterns: vec![
                // Common non-content patterns
                "/login".to_string(),
                "/logout".to_string(),
                "/signup".to_string(),
                "/register".to_string(),
                "/cart".to_string(),
                "/checkout".to_string(),
                "/admin".to_string(),
                "/api/".to_string(),
                ".pdf".to_string(),
                ".zip".to_string(),
                ".exe".to_string(),
                ".dmg".to_string(),
                ".tar".to_string(),
                ".gz".to_string(),
                ".mp4".to_string(),
                ".mp3".to_string(),
                ".wav".to_string(),
                ".avi".to_string(),
                ".mov".to_string(),
                ".jpg".to_string(),
                ".jpeg".to_string(),
                ".png".to_string(),
                ".gif".to_string(),
                ".svg".to_string(),
                ".webp".to_string(),
            ],
            max_chars_per_page: 5000,
            quality_mode: "balanced".to_string(),
        }
    }
}

/// Crawl a website recursively, discovering and scraping pages
/// Optimized to prevent EOF errors by using chunking for large payloads
pub async fn crawl_website(
    state: &Arc<AppState>,
    start_url: &str,
    config: CrawlConfig,
    use_proxy: bool,
) -> Result<CrawlResponse> {
    let start_time = Instant::now();

    // Parse and validate start URL
    let base_url = Url::parse(start_url)?;
    let base_domain = base_url.host_str().unwrap_or("").to_string();

    info!(
        "Starting crawl of {} (max_depth: {}, max_pages: {})",
        start_url, config.max_depth, config.max_pages
    );

    // Track visited URLs and discovered URLs with their depths
    let visited: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));
    let results: Arc<Mutex<Vec<CrawlPageResult>>> = Arc::new(Mutex::new(Vec::new()));
    let unique_domains: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));

    // BFS queue: (url, depth)
    let queue: Arc<Mutex<VecDeque<(String, usize)>>> = Arc::new(Mutex::new(VecDeque::new()));

    // Add start URL to queue
    {
        let mut q = queue.lock().await;
        q.push_back((start_url.to_string(), 0));
    }
    {
        let mut v = visited.lock().await;
        v.insert(normalize_url(start_url));
    }

    let max_depth_reached: Arc<Mutex<usize>> = Arc::new(Mutex::new(0));

    // Process queue in waves (BFS by depth level)
    loop {
        // Check if we've reached max pages
        let current_count = results.lock().await.len();
        if current_count >= config.max_pages {
            info!("Reached max_pages limit: {}", config.max_pages);
            break;
        }

        // Get batch of URLs to process at current depth
        let batch: Vec<(String, usize)> = {
            let mut q = queue.lock().await;
            let remaining = config.max_pages - current_count;
            let batch_size = remaining.min(config.max_concurrent * 2);

            let mut batch = Vec::new();
            while batch.len() < batch_size && !q.is_empty() {
                if let Some(item) = q.pop_front() {
                    batch.push(item);
                }
            }
            batch
        };

        if batch.is_empty() {
            break;
        }

        // Process batch concurrently (but limit concurrency to avoid overwhelming)
        let state_clone = Arc::clone(state);
        let visited_clone = Arc::clone(&visited);
        let domains_clone = Arc::clone(&unique_domains);
        let max_depth_clone = Arc::clone(&max_depth_reached);
        let config_clone = config.clone();
        let base_domain_clone = base_domain.clone();

        let batch_results: Vec<(CrawlPageResult, Vec<(String, usize)>)> = stream::iter(batch)
            .map(|(url, depth)| {
                let state = Arc::clone(&state_clone);
                let config = config_clone.clone();
                let base_domain = base_domain_clone.clone();
                let max_depth_ref = Arc::clone(&max_depth_clone);
                let domains_ref = Arc::clone(&domains_clone);
                let visited_ref = Arc::clone(&visited_clone);

                async move {
                    let page_start = Instant::now();

                    // Update max depth reached
                    {
                        let mut max_d = max_depth_ref.lock().await;
                        if depth > *max_d {
                            *max_d = depth;
                        }
                    }

                    // Scrape the page
                    let quality_mode = QualityMode::parse_str(&config.quality_mode);
                    match crate::scrape::scrape_url_with_options(
                        &state,
                        &url,
                        use_proxy,
                        quality_mode,
                    )
                    .await
                    {
                        Ok(data) => {
                            // Auth-wall handling: never treat login walls as successful content.
                            // If the START URL is auth-walled, abort the entire crawl and
                            // signal NEED_HITL to the caller for immediate HITL escalation.
                            let is_auth_walled = data.auth_wall_reason.is_some()
                                || data.warnings.iter().any(|w| w == "content_restricted");
                            if is_auth_walled {
                                let reason = data.auth_wall_reason.clone().unwrap_or_else(|| {
                                    "Auth-Wall detected (login page returned HTTP 200)".to_string()
                                });
                                let result = CrawlPageResult {
                                    url: url.clone(),
                                    depth,
                                    success: false,
                                    title: Some(data.title),
                                    word_count: Some(data.word_count),
                                    links_found: Some(data.links.len()),
                                    content_preview: None,
                                    error: Some(format!("NEED_HITL: {} (url: {})", reason, url)),
                                    duration_ms: page_start.elapsed().as_millis() as u64,
                                };
                                return (result, vec![]);
                            }

                            // Extract domain
                            let domain = Url::parse(&url)
                                .ok()
                                .and_then(|u| u.host_str().map(|h| h.to_string()))
                                .unwrap_or_default();

                            // Add to unique domains
                            {
                                let mut domains = domains_ref.lock().await;
                                domains.insert(domain);
                            }

                            // Find new links to crawl (only if not at max depth)
                            let mut new_urls: Vec<(String, usize)> = Vec::new();

                            if depth < config.max_depth {
                                for link in &data.links {
                                    if let Some(absolute_url) = resolve_url(&url, &link.url) {
                                        let normalized = normalize_url(&absolute_url);

                                        // Check if should crawl this URL
                                        if should_crawl(&absolute_url, &base_domain, &config) {
                                            let mut visited = visited_ref.lock().await;
                                            if !visited.contains(&normalized) {
                                                visited.insert(normalized);
                                                new_urls.push((absolute_url, depth + 1));
                                            }
                                        }
                                    }
                                }
                            }

                            // Truncate content to avoid large payloads (FIX for EOF errors)
                            let content_preview =
                                if data.clean_content.len() > config.max_chars_per_page {
                                    Some(
                                        data.clean_content
                                            .chars()
                                            .take(config.max_chars_per_page)
                                            .collect(),
                                    )
                                } else {
                                    Some(data.clean_content.clone())
                                };

                            let result = CrawlPageResult {
                                url: url.clone(),
                                depth,
                                success: true,
                                title: Some(data.title),
                                word_count: Some(data.word_count),
                                links_found: Some(data.links.len()),
                                content_preview,
                                error: None,
                                duration_ms: page_start.elapsed().as_millis() as u64,
                            };

                            (result, new_urls)
                        }
                        Err(e) => {
                            warn!("Failed to crawl {}: {}", url, e);
                            let result = CrawlPageResult {
                                url: url.clone(),
                                depth,
                                success: false,
                                title: None,
                                word_count: None,
                                links_found: None,
                                content_preview: None,
                                error: Some(e.to_string()),
                                duration_ms: page_start.elapsed().as_millis() as u64,
                            };
                            (result, vec![])
                        }
                    }
                }
            })
            .buffer_unordered(config.max_concurrent)
            .collect()
            .await;

        // Process results and add new URLs to queue
        // If the start URL hit an auth-wall, abort immediately (HITL required).
        for (r, _) in batch_results.iter() {
            if r.depth == 0 {
                if let Some(err) = r.error.as_deref() {
                    if err.starts_with("NEED_HITL:") {
                        return Err(anyhow::anyhow!(err.to_string()));
                    }
                }
            }
        }
        for (result, new_urls) in batch_results {
            results.lock().await.push(result);

            let mut q = queue.lock().await;
            for url_depth in new_urls {
                q.push_back(url_depth);
            }
        }
    }

    let final_results = results.lock().await.clone();
    let pages_crawled = final_results.iter().filter(|r| r.success).count();
    let pages_failed = final_results.iter().filter(|r| !r.success).count();
    let final_max_depth = *max_depth_reached.lock().await;
    let domains: Vec<String> = unique_domains.lock().await.iter().cloned().collect();

    // Generate sitemap (all successfully crawled URLs)
    let sitemap: Vec<String> = final_results
        .iter()
        .filter(|r| r.success)
        .map(|r| r.url.clone())
        .collect();

    info!(
        "Crawl completed: {} pages crawled, {} failed, max depth {}, {}ms total",
        pages_crawled,
        pages_failed,
        final_max_depth,
        start_time.elapsed().as_millis()
    );

    Ok(CrawlResponse {
        start_url: start_url.to_string(),
        pages_crawled,
        pages_failed,
        max_depth_reached: final_max_depth,
        total_duration_ms: start_time.elapsed().as_millis() as u64,
        unique_domains: domains,
        results: final_results,
        sitemap: Some(sitemap),
    })
}

/// Normalize URL for deduplication (remove fragments, trailing slashes, etc.)
fn normalize_url(url: &str) -> String {
    if let Ok(mut parsed) = Url::parse(url) {
        parsed.set_fragment(None);
        let mut result = parsed.to_string();
        // Remove trailing slash for consistency
        if result.ends_with('/') && result.len() > 1 {
            result.pop();
        }
        result.to_lowercase()
    } else {
        url.to_lowercase()
    }
}

/// Resolve a potentially relative URL to absolute
fn resolve_url(base: &str, href: &str) -> Option<String> {
    // Skip javascript:, mailto:, tel:, etc.
    if href.starts_with("javascript:")
        || href.starts_with("mailto:")
        || href.starts_with("tel:")
        || href.starts_with("#")
        || href.starts_with("data:")
    {
        return None;
    }

    if let Ok(base_url) = Url::parse(base) {
        if let Ok(resolved) = base_url.join(href) {
            // Only allow http/https
            if resolved.scheme() == "http" || resolved.scheme() == "https" {
                return Some(resolved.to_string());
            }
        }
    }
    None
}

/// Check if a URL should be crawled based on configuration
fn should_crawl(url: &str, base_domain: &str, config: &CrawlConfig) -> bool {
    let parsed = match Url::parse(url) {
        Ok(u) => u,
        Err(_) => return false,
    };

    let url_domain = parsed.host_str().unwrap_or("");

    // Check same domain constraint
    if config.same_domain_only {
        // Allow subdomains (e.g., docs.example.com when base is example.com)
        if !url_domain.ends_with(base_domain) && url_domain != base_domain {
            return false;
        }
    }

    let url_lower = url.to_lowercase();

    // Check exclude patterns
    for pattern in &config.exclude_patterns {
        if url_lower.contains(&pattern.to_lowercase()) {
            return false;
        }
    }

    // Check include patterns (if specified, URL must match at least one)
    if !config.include_patterns.is_empty() {
        let matches_include = config
            .include_patterns
            .iter()
            .any(|p| url_lower.contains(&p.to_lowercase()));
        if !matches_include {
            return false;
        }
    }

    true
}
