use crate::types::*;
use crate::AppState;
use crate::rust_scraper::QualityMode;
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
    use_proxy: bool,
    quality_mode: Option<QualityMode>,
) -> Result<ScrapeBatchResponse> {
    let start_time = Instant::now();
    let total_urls = urls.len();

    info!("Starting batch scrape of {} URLs (concurrency: {})", total_urls, max_concurrent);

    // Use futures stream for concurrent scraping with limited concurrency
    let results: Vec<ScrapeBatchResult> = stream::iter(urls)
        .map(|url| {
            let state = Arc::clone(state);
            let max_chars = max_chars;
            let quality_mode = quality_mode;
            async move {
                let url_start = Instant::now();
               
                match crate::scrape::scrape_url_with_options(
                    &state,
                    &url,
                    use_proxy,
                    quality_mode,
                )
                .await {
                    Ok(mut data) => {
                        data.actual_chars = data.clean_content.len();

                        // Truncate content if max_chars specified
                        if let Some(max) = max_chars {
                            crate::content_quality::apply_scrape_content_limit(&mut data, max, true);
                        }

                        // Keep batch JSON focused/clean by default (avoid huge <head>/<script> noise)
                        // Consumers can still use clean_content, headings, links, images, metadata.
                        data.content.clear();
                        crate::content_quality::push_warning_unique(
                            &mut data.warnings,
                            "raw_html_omitted_in_batch_output",
                        );
                        
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
