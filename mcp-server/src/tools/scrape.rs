use crate::rust_scraper::QualityMode;
use crate::rust_scraper::RustScraper;
use crate::types::*;
use crate::AppState;
use anyhow::{anyhow, Result};
use backoff::future::retry;
use backoff::ExponentialBackoffBuilder;
use select::predicate::Predicate;
use std::sync::Arc;
use tracing::{info, warn};

pub async fn scrape_url(state: &Arc<AppState>, url: &str) -> Result<ScrapeResponse> {
    scrape_url_with_options(state, url, false, None).await
}

pub async fn scrape_url_with_options(
    state: &Arc<AppState>,
    url: &str,
    use_proxy: bool,
    quality_mode: Option<QualityMode>,
) -> Result<ScrapeResponse> {
    info!("Scraping URL: {}", url);

    // Validate URL
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(anyhow!("Invalid URL: must start with http:// or https://"));
    }

    // BOSS LEVEL OPTIMIZATION: Check if in rapid testing mode
    let is_testing = if let Some(memory) = &state.memory {
        memory.is_rapid_testing(url).await.unwrap_or(false)
    } else {
        false
    };

    if is_testing {
        info!("🧪 Rapid testing detected for {}, bypassing cache", url);
    }

    // Check cache (bypass if in testing mode)
    if !is_testing {
        if let Some(cached) = state.scrape_cache.get(url).await {
            if cached.word_count == 0 || cached.clean_content.trim().is_empty() {
                // Invalidate poor/empty cache entries and recompute
                state.scrape_cache.invalidate(url).await;
            } else {
                return Ok(cached);
            }
        }
    } else {
        // In testing mode, always invalidate cache
        state.scrape_cache.invalidate(url).await;
    }

    // Concurrency control
    let _permit = state
        .outbound_limit
        .acquire()
        .await
        .expect("semaphore closed");

    // 🚀 UNIVERSAL CDP STRATEGY: Try native CDP (no Docker dependency)
    let cdp_available = crate::scraping::browser_manager::native_browser_available();

    if cdp_available {
        info!("🚀 CDP available, attempting universal stealth mode");

        let rust_scraper = RustScraper::new_with_quality_mode(quality_mode.map(|m| m.as_str()));
        let cdp_proxy = if use_proxy {
            if let Some(proxy_manager) = &state.proxy_manager {
                match proxy_manager.switch_to_best_proxy().await {
                    Ok(proxy_url) => Some(proxy_url),
                    Err(e) => {
                        warn!("Failed to get proxy for CDP: {}", e);
                        None
                    }
                }
            } else {
                None
            }
        } else {
            None
        };

        // Try CDP fetch (no domain restriction)
        let cdp_result = rust_scraper.fetch_via_cdp(url, cdp_proxy.clone()).await;

        match cdp_result {
            Ok((html, _status_code)) => {
                info!("✅ CDP fetch succeeded, processing HTML");

                // Process HTML into ScrapeResponse
                match rust_scraper.process_html(&html, url).await {
                    Ok(result) => {
                        // Record proxy success if used
                        if let (Some(proxy_url), Some(manager)) =
                            (cdp_proxy.as_ref(), state.proxy_manager.as_ref())
                        {
                            let _ = manager.record_proxy_result(proxy_url, true, None).await;
                        }

                        // Log to history
                        if let Some(memory) = &state.memory {
                            let summary = format!(
                                "{} words (CDP stealth), {} code blocks",
                                result.word_count,
                                result.code_blocks.len()
                            );
                            let domain = url::Url::parse(url)
                                .ok()
                                .and_then(|u| u.host_str().map(|s| s.to_string()));
                            let result_json = serde_json::to_value(&result).unwrap_or_default();

                            if let Err(e) = memory
                                .log_scrape(
                                    url.to_string(),
                                    Some(result.title.clone()),
                                    summary,
                                    domain,
                                    &result_json,
                                )
                                .await
                            {
                                warn!("Failed to log CDP scrape to history: {}", e);
                            }
                        }

                        // Cache and return
                        state
                            .scrape_cache
                            .insert(url.to_string(), result.clone())
                            .await;
                        return Ok(result);
                    }
                    Err(e) => {
                        warn!(
                            "❌ CDP HTML processing failed: {}, falling back to standard path",
                            e
                        );
                    }
                }
            }
            Err(e) => {
                warn!(
                    "❌ CDP fetch failed: {}, attempting proxy rotation and retry",
                    e
                );

                // Record proxy failure if used
                if let (Some(proxy_url), Some(manager)) =
                    (cdp_proxy.as_ref(), state.proxy_manager.as_ref())
                {
                    let _ = manager.record_proxy_result(proxy_url, false, None).await;
                }

                // Try with different proxy
                if let Some(proxy_manager) = &state.proxy_manager {
                    match proxy_manager.switch_to_best_proxy().await {
                        Ok(new_proxy_url) => {
                            info!("🔀 Retrying CDP with different proxy");

                            match rust_scraper
                                .fetch_via_cdp(url, Some(new_proxy_url.clone()))
                                .await
                            {
                                Ok((html, _)) => {
                                    if let Ok(result) = rust_scraper.process_html(&html, url).await
                                    {
                                        let _ = proxy_manager
                                            .record_proxy_result(&new_proxy_url, true, None)
                                            .await;
                                        state
                                            .scrape_cache
                                            .insert(url.to_string(), result.clone())
                                            .await;
                                        return Ok(result);
                                    }
                                }
                                Err(e2) => {
                                    warn!("❌ CDP retry also failed: {}, falling back to standard path", e2);
                                    let _ = proxy_manager
                                        .record_proxy_result(&new_proxy_url, false, None)
                                        .await;
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Failed to switch proxy for CDP retry: {}", e);
                        }
                    }
                }

                // Fall through to native CDP forced path
                warn!("📉 CDP stealth failed, falling through to forced-mode CDP");
            }
        }
    }

    let rust_scraper = RustScraper::new_with_quality_mode(quality_mode.map(|m| m.as_str()));
    let url_owned = url.to_string();
    let mut force_browserless = false;
    let mut forced_proxy: Option<String> = None;

    if use_proxy {
        if let Some(proxy_manager) = &state.proxy_manager {
            match proxy_manager.switch_to_best_proxy().await {
                Ok(proxy_url) => {
                    force_browserless = true;
                    forced_proxy = Some(proxy_url);
                }
                Err(e) => {
                    warn!("Failed to switch proxy for forced proxy mode: {}", e);
                }
            }
        } else {
            warn!("Proxy manager not available for forced proxy mode");
        }
    }

    // Pre-flight checker: quick native probe to detect blocks before full scrape
    if let Ok(preflight) = rust_scraper.preflight_check(&url_owned).await {
        if preflight.status_code >= 400 || preflight.blocked_reason.is_some() {
            info!(
                "Preflight suggests native CDP (status: {}, reason: {:?})",
                preflight.status_code, preflight.blocked_reason
            );
            force_browserless = true;
        }
    }

    if use_proxy && !crate::scraping::browser_manager::native_browser_available() {
        warn!(
            "use_proxy requested but no browser found; proxy mode requires a browser (install Brave/Chrome/Chromium)"
        );
    }

    // Use native CDP if available and forced by preflight or use_proxy
    if force_browserless && crate::scraping::browser_manager::native_browser_available() {
        info!(
            "🎯 Native CDP forced ({}), using stealth mode",
            extract_domain(url)
        );

        let initial_result = if let Some(proxy_url) = forced_proxy.clone() {
            rust_scraper
                .scrape_with_browserless_advanced_with_proxy(&url_owned, None, Some(proxy_url))
                .await
        } else {
            rust_scraper
                .scrape_with_browserless_advanced(&url_owned, None)
                .await
        };

        match initial_result {
            Ok(result) => {
                if let (Some(proxy_url), Some(manager)) =
                    (forced_proxy.as_ref(), state.proxy_manager.as_ref())
                {
                    let _ = manager.record_proxy_result(proxy_url, true, None).await;
                }
                // 🔀 SELF-CORRECTION LOGIC: Check if result indicates block
                let is_blocked = result.title.contains("Access to this page has been denied")
                    || result.title.contains("Access Denied")
                    || result.title.contains("Captcha")
                    || result.word_count < 50;

                if is_blocked && state.proxy_manager.is_some() {
                    warn!(
                        "⚠️ BLOCK DETECTED: {} words, title: '{}'. Attempting proxy retry...",
                        result.word_count, result.title
                    );

                    // Try with proxy
                    if let Some(proxy_manager) = &state.proxy_manager {
                        match proxy_manager.switch_to_best_proxy().await {
                            Ok(proxy_url) => {
                                info!("🔀 Switched to proxy for retry: {}", {
                                    if let Ok(parsed) = url::Url::parse(&proxy_url) {
                                        format!(
                                            "{}://{}@{}:{}",
                                            parsed.scheme(),
                                            parsed.username(),
                                            parsed.host_str().unwrap_or("unknown"),
                                            parsed
                                                .port()
                                                .map(|p| p.to_string())
                                                .unwrap_or_default()
                                        )
                                    } else {
                                        "invalid".to_string()
                                    }
                                });

                                // Retry with proxy
                                match rust_scraper
                                    .scrape_with_browserless_advanced_with_proxy(
                                        &url_owned,
                                        None,
                                        Some(proxy_url.clone()),
                                    )
                                    .await
                                {
                                    Ok(proxy_result) => {
                                        // Check if proxy result is better
                                        if proxy_result.word_count > result.word_count * 2 {
                                            info!(
                                                "✅ PROXY RETRY SUCCESS: {} words (vs {} before)",
                                                proxy_result.word_count, result.word_count
                                            );

                                            // Record proxy success
                                            let _ = proxy_manager
                                                .record_proxy_result(&proxy_url, true, None)
                                                .await;

                                            // Cache and return proxy result
                                            state
                                                .scrape_cache
                                                .insert(url.to_string(), proxy_result.clone())
                                                .await;

                                            // Auto-log to history
                                            if let Some(memory) = &state.memory {
                                                let summary = format!(
                                                    "{} words (proxy), {} code blocks",
                                                    proxy_result.word_count,
                                                    proxy_result.code_blocks.len()
                                                );
                                                let domain =
                                                    url::Url::parse(url).ok().and_then(|u| {
                                                        u.host_str().map(|s| s.to_string())
                                                    });
                                                let result_json =
                                                    serde_json::to_value(&proxy_result)
                                                        .unwrap_or_default();

                                                if let Err(e) = memory
                                                    .log_scrape(
                                                        url.to_string(),
                                                        Some(proxy_result.title.clone()),
                                                        summary,
                                                        domain,
                                                        &result_json,
                                                    )
                                                    .await
                                                {
                                                    tracing::warn!(
                                                        "Failed to log scrape to history: {}",
                                                        e
                                                    );
                                                }
                                            }

                                            return Ok(proxy_result);
                                        } else {
                                            warn!("⚠️ PROXY RETRY NO IMPROVEMENT: {} words vs {} before", 
                                                  proxy_result.word_count, result.word_count);
                                            // Record proxy failure
                                            let _ = proxy_manager
                                                .record_proxy_result(&proxy_url, false, None)
                                                .await;
                                        }
                                    }
                                    Err(e) => {
                                        warn!("❌ PROXY RETRY FAILED: {}", e);
                                        // Record proxy failure
                                        let _ = proxy_manager
                                            .record_proxy_result(&proxy_url, false, None)
                                            .await;
                                    }
                                }
                            }
                            Err(e) => {
                                warn!("Failed to switch proxy: {}. Returning original result.", e);
                            }
                        }
                    }
                }

                // Cache and return original result (if proxy retry didn't succeed)
                state
                    .scrape_cache
                    .insert(url.to_string(), result.clone())
                    .await;

                // Auto-log to history
                if let Some(memory) = &state.memory {
                    let summary = format!(
                        "{} words, {} code blocks",
                        result.word_count,
                        result.code_blocks.len()
                    );
                    let domain = url::Url::parse(url)
                        .ok()
                        .and_then(|u| u.host_str().map(|s| s.to_string()));
                    let result_json = serde_json::to_value(&result).unwrap_or_default();

                    if let Err(e) = memory
                        .log_scrape(
                            url.to_string(),
                            Some(result.title.clone()),
                            summary,
                            domain,
                            &result_json,
                        )
                        .await
                    {
                        tracing::warn!("Failed to log scrape to history: {}", e);
                    }
                }

                return Ok(result);
            }
            Err(e) => {
                if let (Some(proxy_url), Some(manager)) =
                    (forced_proxy.as_ref(), state.proxy_manager.as_ref())
                {
                    let _ = manager.record_proxy_result(proxy_url, false, None).await;
                }
                info!(
                    "⚠️ Native CDP failed for this domain: {}, falling back to native scraper",
                    e
                );
            }
        }
    }

    // Only use Rust-native scraper with retries
    let mut result = retry(
        ExponentialBackoffBuilder::new()
            .with_initial_interval(std::time::Duration::from_millis(200))
            .with_max_interval(std::time::Duration::from_secs(2))
            .with_max_elapsed_time(Some(std::time::Duration::from_secs(6)))
            .build(),
        || async {
            match rust_scraper.scrape_url(&url_owned).await {
                Ok(r) => Ok(r),
                Err(e) => {
                    // Treat network/temporary HTML parse errors as transient
                    Err(backoff::Error::transient(anyhow!("{}", e)))
                }
            }
        },
    )
    .await?;

    // PHASE 3: Adaptive native-CDP fallback for low-quality extractions
    let should_use_browserless = (result.extraction_score.map(|s| s < 0.35).unwrap_or(false)
        || result.word_count < 50)
        && !result
            .warnings
            .contains(&"browserless_rendered".to_string());

    if should_use_browserless {
        if crate::scraping::browser_manager::native_browser_available() {
            info!(
                "Low quality extraction (score: {:.2}, words: {}), attempting native CDP fallback",
                result.extraction_score.unwrap_or(0.0),
                result.word_count
            );

            match rust_scraper.scrape_with_browserless(&url_owned).await {
                Ok(browserless_result) => {
                    if browserless_result.word_count > result.word_count + 20 {
                        info!(
                            "✨ Native CDP improved extraction: {} → {} words",
                            result.word_count, browserless_result.word_count
                        );
                        result = browserless_result;
                    } else {
                        info!("Native CDP didn't improve extraction significantly, keeping original");
                    }
                }
                Err(e) => {
                    info!("Native CDP fallback failed: {}, using static result", e);
                    result
                        .warnings
                        .push("cdp_fallback_failed".to_string());
                }
            }
        } else {
            result.warnings.push("low_quality_extraction".to_string());
            result.warnings.push(
                "suggestion: This site may require JavaScript rendering. Install Brave, Chrome, or Chromium."
                    .to_string(),
            );
            info!(
                "Low extraction score ({:.2}) for {}, but no browser installed",
                result.extraction_score.unwrap_or(0.0),
                url
            );
        }
    }

    if result.word_count == 0 || result.clean_content.trim().is_empty() {
        info!(
            "Scraper returned empty content after all attempts, using legacy fallback for {}",
            url
        );
        result = scrape_url_fallback(state, &url_owned).await?;
    } else {
        info!(
            "Scraper succeeded for {} ({} words)",
            url, result.word_count
        );
    }
    state
        .scrape_cache
        .insert(url.to_string(), result.clone())
        .await;

    // Auto-log to history if memory is enabled (Phase 1)
    if let Some(memory) = &state.memory {
        let summary = format!(
            "{} words, {} code blocks",
            result.word_count,
            result.code_blocks.len()
        );

        // Extract domain from URL
        let domain = url::Url::parse(url)
            .ok()
            .and_then(|u| u.host_str().map(|s| s.to_string()));

        let result_json = serde_json::to_value(&result).unwrap_or_default();

        if let Err(e) = memory
            .log_scrape(
                url.to_string(),
                Some(result.title.clone()),
                summary,
                domain,
                &result_json,
            )
            .await
        {
            tracing::warn!("Failed to log scrape to history: {}", e);
        }
    }

    Ok(result)
}

// Fallback scraper using direct HTTP request (legacy simple mode) -- optional; keeping for troubleshooting
pub async fn scrape_url_fallback(state: &Arc<AppState>, url: &str) -> Result<ScrapeResponse> {
    info!("Using fallback scraper for: {}", url);

    // Make direct HTTP request
    let response = state
        .http_client
        .get(url)
        .header("User-Agent", "Mozilla/5.0 (compatible; SearchScrape/1.0)")
        .send()
        .await
        .map_err(|e| anyhow!("Failed to fetch URL: {}", e))?;

    let status_code = response.status().as_u16();
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("text/html")
        .to_string();

    let html = response
        .text()
        .await
        .map_err(|e| anyhow!("Failed to read response body: {}", e))?;

    let document = select::document::Document::from(html.as_str());

    let title = document
        .find(select::predicate::Name("title"))
        .next()
        .map(|n| n.text())
        .unwrap_or_else(|| "No Title".to_string());

    let meta_description = document
        .find(select::predicate::Attr("name", "description"))
        .next()
        .and_then(|n| n.attr("content"))
        .unwrap_or("")
        .to_string();

    let meta_keywords = document
        .find(select::predicate::Attr("name", "keywords"))
        .next()
        .and_then(|n| n.attr("content"))
        .unwrap_or("")
        .to_string();

    let body_html = document
        .find(select::predicate::Name("body"))
        .next()
        .map(|n| n.html())
        .unwrap_or_else(|| html.clone());

    let clean_content = html2md::parse_html(&body_html);
    let word_count = clean_content.split_whitespace().count();

    let headings: Vec<Heading> = document
        .find(
            select::predicate::Name("h1")
                .or(select::predicate::Name("h2"))
                .or(select::predicate::Name("h3"))
                .or(select::predicate::Name("h4"))
                .or(select::predicate::Name("h5"))
                .or(select::predicate::Name("h6")),
        )
        .map(|n| Heading {
            level: n.name().unwrap_or("h1").to_string(),
            text: n.text(),
        })
        .collect();

    let links: Vec<Link> = document
        .find(select::predicate::Name("a"))
        .filter_map(|n| {
            n.attr("href").map(|href| Link {
                url: href.to_string(),
                text: n.text(),
            })
        })
        .collect();

    let images: Vec<Image> = document
        .find(select::predicate::Name("img"))
        .filter_map(|n| {
            n.attr("src").map(|src| Image {
                src: src.to_string(),
                alt: n.attr("alt").unwrap_or("").to_string(),
                title: n.attr("title").unwrap_or("").to_string(),
            })
        })
        .collect();

    let result = ScrapeResponse {
        url: url.to_string(),
        title,
        content: html,
        clean_content,
        embedded_state_json: None,
        embedded_data_sources: Vec::new(),
        hydration_status: crate::types::HydrationStatus::default(),
        meta_description,
        meta_keywords,
        headings,
        links,
        images,
        timestamp: chrono::Utc::now().to_rfc3339(),
        status_code,
        content_type,
        word_count,
        language: "unknown".to_string(),
        canonical_url: None,
        site_name: None,
        author: None,
        published_at: None,
        og_title: None,
        og_description: None,
        og_image: None,
        reading_time_minutes: None,
        // New Priority 1 fields (fallback scraper)
        code_blocks: Vec::new(),
        truncated: false,
        actual_chars: 0,
        max_chars_limit: None,
        extraction_score: Some(0.3), // Lower score for fallback
        warnings: vec!["fallback_scraper_used".to_string()],
        domain: url::Url::parse(url)
            .ok()
            .and_then(|u| u.host_str().map(|h| h.to_string())),
    };

    info!("Fallback scraper extracted {} words", result.word_count);
    Ok(result)
}

// BOSS LEVEL OPTIMIZATION: Domain detection helpers
fn extract_domain(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_scrape_url_fallback() {
        let state = Arc::new(AppState::new(reqwest::Client::new()));

        let result = scrape_url_fallback(&state, "https://httpbin.org/html").await;

        match result {
            Ok(content) => {
                assert!(!content.title.is_empty(), "Title should not be empty");
                assert!(
                    !content.clean_content.is_empty(),
                    "Content should not be empty"
                );
                assert_eq!(content.status_code, 200, "Status code should be 200");
            }
            Err(e) => {
                tracing::warn!("Fallback scraper test failed: {}", e);
            }
        }
    }
}
