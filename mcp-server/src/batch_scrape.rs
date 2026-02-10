use crate::scrape::scrape_url;
use crate::types::*;
use crate::AppState;
use anyhow::Result;
use futures::stream::{self, StreamExt};
use std::sync::Arc;
use std::time::Instant;
use tracing::{info, warn};

/// Scrape multiple URLs concurrently in batch
/// Optimized for high-throughput scraping with controlled concurrency
pub async fn scrape_batch(
    state: &Arc<AppState>,
    urls: Vec<String>,
    max_concurrent: usize,
    max_chars: Option<usize>,
) -> Result<ScrapeBatchResponse> {
    let start_time = Instant::now();
    let total_urls = urls.len();

    info!("Starting batch scrape of {} URLs (concurrency: {})", total_urls, max_concurrent);

    // Use futures stream for concurrent scraping with limited concurrency
    let results: Vec<ScrapeBatchResult> = stream::iter(urls)
        .map(|url| {
            let state = Arc::clone(state);
            let max_chars = max_chars;
            async move {
                let url_start = Instant::now();
               
                match scrape_url(&state, &url).await {
                    Ok(mut data) => {
                        // Truncate content if max_chars specified
                        if let Some(max) = max_chars {
                            if data.clean_content.len() > max {
                                data.clean_content = data.clean_content.chars().take(max).collect();
                                data.truncated = true;
                                data.max_chars_limit = Some(max);
                            }
                        }
                        
                        ScrapeBatchResult {
                            url,
                            success: true,
                            data: Some(data),
                            error: None,
                            duration_ms: url_start.elapsed().as_millis() as u64,
                        }
                    }
                    Err(e) => {
                        warn!("Failed to scrape {}: {}", url, e);
                        ScrapeBatchResult {
                            url,
                            success: false,
                            data: None,
                            error: Some(e.to_string()),
                            duration_ms: url_start.elapsed().as_millis() as u64,
                        }
                    }
                }
            }
        })
        .buffer_unordered(max_concurrent)
        .collect()
        .await;

    let successful = results.iter().filter(|r| r.success).count();
    let failed = results.iter().filter(|r| !r.success).count();

    info!(
        "Batch scrape completed: {}/{} successful, {} failed, {}ms total",
        successful,
        total_urls,
        failed,
        start_time.elapsed().as_millis()
    );

    Ok(ScrapeBatchResponse {
        total: total_urls,
        successful,
        failed,
        total_duration_ms: start_time.elapsed().as_millis() as u64,
        results,
    })
}
