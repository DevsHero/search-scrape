use crate::nlp::semantic_shave;
use crate::rust_scraper::QualityMode;
use crate::rust_scraper::RustScraper;
use crate::types::*;
use crate::AppState;
use anyhow::{anyhow, Result};
use backoff::future::retry;
use backoff::ExponentialBackoffBuilder;
use select::predicate::Predicate;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use tracing::{info, warn};

#[derive(Debug, Clone, Default)]
pub struct ScrapeUrlOptions {
    pub use_proxy: bool,
    pub quality_mode: Option<QualityMode>,
    pub query: Option<String>,
    pub strict_relevance: bool,
    pub relevance_threshold: Option<f32>,
    pub extract_app_state: bool,

    // Optional: return only the most relevant sections for the query (short output).
    pub extract_relevant_sections: bool,
    pub section_limit: Option<usize>,
    pub section_threshold: Option<f32>,
}

pub async fn scrape_url(state: &Arc<AppState>, url: &str) -> Result<ScrapeResponse> {
    scrape_url_full(state, url, ScrapeUrlOptions::default()).await
}

pub async fn scrape_url_with_options(
    state: &Arc<AppState>,
    url: &str,
    use_proxy: bool,
    quality_mode: Option<QualityMode>,
) -> Result<ScrapeResponse> {
    scrape_url_full(
        state,
        url,
        ScrapeUrlOptions {
            use_proxy,
            quality_mode,
            ..Default::default()
        },
    )
    .await
}

/// Full scrape with optional Semantic Shaving.
///
/// - `query`: optional search query for Semantic Shaving.
/// - `strict_relevance`: when `true`, filter content to only query-relevant paragraphs.
/// - `relevance_threshold`: cosine similarity threshold (default 0.35).
/// - `extract_app_state`: when `true`, force-return the raw SPA JSON (Next.js/Nuxt/Remix
///   `__NEXT_DATA__` etc.) even if it is sparse.  Defaults to `false`, which causes the
///   SPA fast-path to fall back to readability when fewer than 100 readable words are found.
pub async fn scrape_url_full(
    state: &Arc<AppState>,
    url: &str,
    options: ScrapeUrlOptions,
) -> Result<ScrapeResponse> {
    let ScrapeUrlOptions {
        use_proxy,
        quality_mode,
        query,
        strict_relevance,
        relevance_threshold,
        extract_app_state,
        extract_relevant_sections,
        section_limit,
        section_threshold,
    } = options;
    let query = query.as_deref();

    info!("Scraping URL: {}", url);

    // Validate URL
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(anyhow!("Invalid URL: must start with http:// or https://"));
    }

    // 🧬 Smart URL rewrite: transform well-known URL patterns into their cleanest form.
    // GitHub /blob/ pages → raw.githubusercontent.com returns plain text directly.
    let url_rewritten = rewrite_url_for_clean_content(url);
    let url: &str = url_rewritten.as_deref().unwrap_or(url);

    // Cache key must include knobs that affect output; otherwise comparisons (and correctness)
    // are broken because a previous scrape can be returned for a different mode.
    let cache_key = compute_scrape_cache_key(
        url,
        ScrapeCacheKeyKnobs {
            quality_mode,
            query,
            strict_relevance,
            relevance_threshold,
            extract_app_state,
            extract_relevant_sections,
            section_limit,
            section_threshold,
        },
    );

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
        if let Some(cached) = state.scrape_cache.get(&cache_key).await {
            if cached.word_count == 0 || cached.clean_content.trim().is_empty() {
                // Invalidate poor/empty cache entries and recompute
                state.scrape_cache.invalidate(&cache_key).await;
            } else {
                return Ok(cached);
            }
        }
    } else {
        // In testing mode, always invalidate cache
        state.scrape_cache.invalidate(&cache_key).await;
    }

    // Concurrency control
    let _permit = state
        .outbound_limit
        .acquire()
        .await
        .expect("semaphore closed");

    // 🚀 UNIVERSAL CDP STRATEGY: Try native CDP
    let cdp_available = crate::scraping::browser_manager::native_browser_available();

    if cdp_available {
        info!("🚀 CDP available, attempting universal stealth mode");

        let rust_scraper = RustScraper::new_with_quality_mode(quality_mode.map(|m| m.as_str()))
            .with_extract_app_state(extract_app_state);
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
                    Ok(mut result) => {
                        // Record proxy success if used
                        if let (Some(proxy_url), Some(manager)) =
                            (cdp_proxy.as_ref(), state.proxy_manager.as_ref())
                        {
                            let _ = manager.record_proxy_result(proxy_url, true, None).await;
                        }

                        // 🧬 Semantic Shaving must run even on the CDP fast-path.
                        apply_semantic_shaving_if_enabled(
                            state,
                            &mut result,
                            query,
                            strict_relevance,
                            relevance_threshold,
                        )
                        .await;

                        apply_relevant_section_extract_if_enabled(
                            state,
                            &mut result,
                            query,
                            extract_relevant_sections,
                            section_limit,
                            section_threshold,
                        )
                        .await;

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
                            .insert(cache_key.clone(), result.clone())
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
                                    if let Ok(mut result) = rust_scraper.process_html(&html, url).await
                                    {
                                        let _ = proxy_manager
                                            .record_proxy_result(&new_proxy_url, true, None)
                                            .await;

                                        apply_semantic_shaving_if_enabled(
                                            state,
                                            &mut result,
                                            query,
                                            strict_relevance,
                                            relevance_threshold,
                                        )
                                        .await;

                                        apply_relevant_section_extract_if_enabled(
                                            state,
                                            &mut result,
                                            query,
                                            extract_relevant_sections,
                                            section_limit,
                                            section_threshold,
                                        )
                                        .await;

                                        // Note: retry path uses the same cache key.
                                        state
                                            .scrape_cache
                                            .insert(cache_key.clone(), result.clone())
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

    let rust_scraper = RustScraper::new_with_quality_mode(quality_mode.map(|m| m.as_str()))
        .with_extract_app_state(extract_app_state);
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
    let should_use_native_cdp = (result.extraction_score.map(|s| s < 0.35).unwrap_or(false)
        || result.word_count < 50)
        && !result.warnings.contains(&"native_cdp_rendered".to_string());

    if should_use_native_cdp {
        if crate::scraping::browser_manager::native_browser_available() {
            info!(
                "Low quality extraction (score: {:.2}, words: {}), attempting native CDP fallback",
                result.extraction_score.unwrap_or(0.0),
                result.word_count
            );

            match rust_scraper.scrape_with_browserless(&url_owned).await {
                Ok(cdp_result) => {
                    if cdp_result.word_count > result.word_count + 20 {
                        info!(
                            "✨ Native CDP improved extraction: {} → {} words",
                            result.word_count, cdp_result.word_count
                        );
                        result = cdp_result;
                    } else {
                        info!(
                            "Native CDP didn't improve extraction significantly, keeping original"
                        );
                    }
                }
                Err(e) => {
                    info!("Native CDP fallback failed: {}, using static result", e);
                    result.warnings.push("cdp_fallback_failed".to_string());
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

    apply_semantic_shaving_if_enabled(
        state,
        &mut result,
        query,
        strict_relevance,
        relevance_threshold,
    )
    .await;

    apply_relevant_section_extract_if_enabled(
        state,
        &mut result,
        query,
        extract_relevant_sections,
        section_limit,
        section_threshold,
    )
    .await;

    state
        .scrape_cache
        .insert(cache_key.clone(), result.clone())
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

#[derive(Clone, Copy, Debug)]
struct ScrapeCacheKeyKnobs<'a> {
    quality_mode: Option<QualityMode>,
    query: Option<&'a str>,
    strict_relevance: bool,
    relevance_threshold: Option<f32>,
    extract_app_state: bool,
    extract_relevant_sections: bool,
    section_limit: Option<usize>,
    section_threshold: Option<f32>,
}

fn compute_scrape_cache_key(url: &str, knobs: ScrapeCacheKeyKnobs<'_>) -> String {
    let ScrapeCacheKeyKnobs {
        quality_mode,
        query,
        strict_relevance,
        relevance_threshold,
        extract_app_state,
        extract_relevant_sections,
        section_limit,
        section_threshold,
    } = knobs;
    let ns = if crate::core::config::neurosiphon_enabled() { 1 } else { 0 };
    let qm = quality_mode.unwrap_or(QualityMode::Balanced).as_str();
    let eas = if extract_app_state { 1 } else { 0 };
    let ers = if extract_relevant_sections { 1 } else { 0 };
    let mut key = format!("{}|qm={}|ns={}|eas={}|ers={}", url, qm, ns, eas, ers);
    if strict_relevance {
        let threshold = relevance_threshold.unwrap_or(semantic_shave::DEFAULT_RELEVANCE_THRESHOLD);
        key.push_str(&format!("|sr=1|t={:.3}", threshold));
        if let Some(q) = query {
            let q = q.trim();
            if !q.is_empty() {
                let mut hasher = DefaultHasher::new();
                q.hash(&mut hasher);
                key.push_str(&format!("|q={:016x}", hasher.finish()));
            }
        }
    }

    if extract_relevant_sections {
        let limit = section_limit.unwrap_or(5);
        let thr = section_threshold.unwrap_or(0.45);
        key.push_str(&format!("|sl={}|sth={:.3}", limit, thr));
        if let Some(q) = query {
            let q = q.trim();
            if !q.is_empty() {
                let mut hasher = DefaultHasher::new();
                q.hash(&mut hasher);
                key.push_str(&format!("|sq={:016x}", hasher.finish()));
            }
        }
    }
    key
}

fn split_markdown_sections(markdown: &str) -> Vec<String> {
    let mut sections: Vec<String> = Vec::new();
    let mut current: Vec<String> = Vec::new();

    for line in markdown.lines() {
        let is_heading = line.starts_with('#') && line.chars().take_while(|c| *c == '#').count() <= 6;
        if is_heading && !current.is_empty() {
                let s = current.join("\n").trim().to_string();
                if !s.is_empty() {
                    sections.push(s);
                }
                current.clear();
        }
        current.push(line.to_string());
    }

    if !current.is_empty() {
        let s = current.join("\n").trim().to_string();
        if !s.is_empty() {
            sections.push(s);
        }
    }

    // If we didn't find headings, treat the whole doc as a single section.
    if sections.is_empty() {
        let s = markdown.trim().to_string();
        if !s.is_empty() {
            sections.push(s);
        }
    }

    sections
}

async fn apply_relevant_section_extract_if_enabled(
    state: &Arc<AppState>,
    result: &mut ScrapeResponse,
    query: Option<&str>,
    extract_relevant_sections: bool,
    section_limit: Option<usize>,
    section_threshold: Option<f32>,
) {
    if !extract_relevant_sections {
        return;
    }

    let Some(q) = query.map(str::trim).filter(|s| !s.is_empty()) else {
        result.warnings.push("section_extract_missing_query".to_string());
        return;
    };

    let original = result.clean_content.clone();
    let sections = split_markdown_sections(&original);
    let total = sections.len();
    if total <= 1 {
        result.warnings.push("section_extract_single_section".to_string());
        return;
    }

    let limit = section_limit.unwrap_or(5).clamp(1, 20);
    let threshold = section_threshold.unwrap_or(0.45);

    // Prefer embedding similarity when memory/model is available; otherwise do a lightweight
    // keyword fallback so the feature still works without LanceDB.
    let mut scored: Vec<(usize, f32)> = Vec::new();
    if let Some(memory) = &state.memory {
        if let Ok(model) = memory.get_embedding_model().await {
            let q_owned = q.to_string();
            let sections_owned = sections.clone();
            let model_clone = std::sync::Arc::clone(&model);

            let embed_result: anyhow::Result<(Vec<f32>, Vec<Vec<f32>>)> = match tokio::task::spawn_blocking(move || {
                let q_vec = model_clone.encode_single(&q_owned);
                let s_vecs = sections_owned
                    .iter()
                    .map(|s| model_clone.encode_single(s))
                    .collect::<Vec<_>>();
                Ok::<_, anyhow::Error>((q_vec, s_vecs))
            })
            .await
            {
                Ok(v) => v,
                Err(e) => {
                    warn!("section_extract spawn_blocking failed: {}", e);
                    result
                        .warnings
                        .push("section_extract_embedding_failed".to_string());
                    Err(anyhow!("spawn_blocking failed: {}", e))
                }
            };

            match embed_result {
                Ok((q_vec, s_vecs)) => {
                    for (i, s_vec) in s_vecs.iter().enumerate() {
                        let sim = semantic_shave::cosine_similarity(&q_vec, s_vec);
                        scored.push((i, sim));
                    }
                }
                Err(e) => {
                    warn!("section_extract embedding failed: {}", e);
                    result.warnings.push("section_extract_embedding_failed".to_string());
                }
            }
        } else {
            result
                .warnings
                .push("section_extract_model_unavailable".to_string());
        }
    } else {
        result.warnings.push("section_extract_no_memory".to_string());
    }

    if scored.is_empty() {
        // Keyword fallback scoring
        let q_lc = q.to_ascii_lowercase();
        for (i, s) in sections.iter().enumerate() {
            let s_lc = s.to_ascii_lowercase();
            let score = if s_lc.contains(&q_lc) { 1.0 } else { 0.0 };
            scored.push((i, score));
        }
    }

    // Keep sections meeting threshold; otherwise keep top-scoring section.
    let mut keep: Vec<(usize, f32)> = scored
        .iter()
        .copied()
        .filter(|(_, sim)| *sim >= threshold)
        .collect();
    keep.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    if keep.is_empty() {
        if let Some(best) = scored
            .iter()
            .copied()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        {
            keep.push(best);
            result.warnings.push(format!(
                "section_extract: no sections met threshold {:.2}; kept best section",
                threshold
            ));
        }
    }

    keep.truncate(limit);
    let kept = keep.len();

    // Re-join in original order
    let mut keep_idxs = keep.into_iter().map(|(i, _)| i).collect::<Vec<_>>();
    keep_idxs.sort_unstable();
    let extracted = keep_idxs
        .into_iter()
        .filter_map(|i| sections.get(i).cloned())
        .collect::<Vec<_>>()
        .join("\n\n");

    // Safety: never inflate output.
    if extracted.len() > original.len() {
        result.warnings.push("section_extract_aborted_expanded".to_string());
        return;
    }

    result.clean_content = extracted;
    result.word_count = result.clean_content.split_whitespace().count();
    result.warnings.push(format!(
        "section_extract: kept {}/{} sections",
        kept, total
    ));
}

async fn apply_semantic_shaving_if_enabled(
    state: &Arc<AppState>,
    result: &mut ScrapeResponse,
    query: Option<&str>,
    strict_relevance: bool,
    relevance_threshold: Option<f32>,
) {
    if !crate::core::config::neurosiphon_enabled() {
        return;
    }

    // Filter content to only query-relevant paragraphs using Model2Vec cosine similarity.
    // Activated when: strict_relevance=true AND query is provided AND memory (model) is available.
    if !strict_relevance {
        return;
    }

    let Some(q) = query.map(str::trim).filter(|s| !s.is_empty()) else {
        result.warnings.push("semantic_shave_missing_query".to_string());
        return;
    };

    let Some(memory) = &state.memory else {
        warn!("semantic_shave requested but memory/model not enabled");
        result.warnings.push("semantic_shave_no_memory".to_string());
        return;
    };

    let model = match memory.get_embedding_model().await {
        Ok(m) => m,
        Err(e) => {
            warn!("⚠️ Could not load embedding model for semantic shave: {}", e);
            result
                .warnings
                .push("semantic_shave_model_unavailable".to_string());
            return;
        }
    };

    let before_words = result.word_count;
    match semantic_shave::semantic_shave(model, &result.clean_content, q, relevance_threshold).await {
        Ok((shaved, kept, total)) => {
            result.clean_content = shaved;
            result.word_count = result.clean_content.split_whitespace().count();
            result.warnings.push(format!(
                "semantic_shave: kept {}/{} chunks ({} → {} words)",
                kept, total, before_words, result.word_count
            ));
            info!(
                "🪒 Semantic shave applied: {}/{} chunks kept ({} → {} words)",
                kept, total, before_words, result.word_count
            );
        }
        Err(e) => {
            warn!("⚠️ Semantic shave failed (non-fatal): {}", e);
            result.warnings.push("semantic_shave_failed".to_string());
        }
    }
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

/// Rewrite certain URL patterns into variants that produce cleaner content for the pipeline.
///
/// Currently handles:
/// - `github.com/{owner}/{repo}/blob/{ref}/{path}` → `raw.githubusercontent.com/{owner}/{repo}/{ref}/{path}`
///   GitHub blob pages are React SPAs; the raw URL returns plain source text directly.
fn rewrite_url_for_clean_content(url: &str) -> Option<String> {
    // GitHub file blob viewer pages
    if url.contains("github.com/") && url.contains("/blob/") && !url.contains("raw.githubusercontent.com") {
        if let Some(blob_idx) = url.find("/blob/") {
            let prefix = &url[..blob_idx]; // "https://github.com/owner/repo"
            let after_blob = &url[blob_idx + "/blob".len()..]; // "/main/README.md"
            if let Some(gh_idx) = prefix.find("github.com") {
                let scheme_prefix = &prefix[..gh_idx]; // "https://"
                let repo_path = &prefix[(gh_idx + "github.com".len())..]; // "/owner/repo"
                let raw_url = format!("{}raw.githubusercontent.com{}{}", scheme_prefix, repo_path, after_blob);
                info!("🔀 GitHub blob → raw URL: {}", raw_url);
                return Some(raw_url);
            }
        }
    }
    None
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

    fn mk_response(clean_content: &str) -> ScrapeResponse {
        ScrapeResponse {
            url: "https://example.com".to_string(),
            title: "Test".to_string(),
            content: String::new(),
            clean_content: clean_content.to_string(),
            embedded_state_json: None,
            embedded_data_sources: vec![],
            hydration_status: HydrationStatus {
                json_found: false,
                settle_time_ms: None,
                noise_reduction_ratio: 0.0,
            },
            meta_description: String::new(),
            meta_keywords: String::new(),
            headings: vec![],
            links: vec![],
            images: vec![],
            timestamp: "".to_string(),
            status_code: 200,
            content_type: "text/html".to_string(),
            word_count: clean_content.split_whitespace().count(),
            language: "en".to_string(),
            canonical_url: None,
            site_name: None,
            author: None,
            published_at: None,
            og_title: None,
            og_description: None,
            og_image: None,
            reading_time_minutes: None,
            code_blocks: vec![],
            truncated: false,
            actual_chars: 0,
            max_chars_limit: None,
            extraction_score: None,
            warnings: vec![],
            domain: None,
        }
    }

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

    #[tokio::test]
    async fn test_extract_relevant_sections_keyword_fallback_keeps_matching_section() {
        let state = Arc::new(AppState::new(reqwest::Client::new()));

        let doc = r#"
# Intro
This is a long document about many things.

## Unrelated
This section does not mention the keyword.

## Quantization Requirements
Unsloth quantization requires bitsandbytes and a supported GPU.

## Other
More details.
"#;

        let mut result = mk_response(doc);
        let before_len = result.clean_content.len();

        apply_relevant_section_extract_if_enabled(
            &state,
            &mut result,
            Some("quantization"),
            true,
            Some(5),
            Some(0.45),
        )
        .await;

        assert!(result.clean_content.len() <= before_len);
        assert!(result.clean_content.to_ascii_lowercase().contains("quantization"));
        assert!(result
            .warnings
            .iter()
            .any(|w| w == "section_extract_no_memory"));
        assert!(result
            .warnings
            .iter()
            .any(|w| w.starts_with("section_extract: kept ")));
    }

    #[tokio::test]
    async fn test_extract_relevant_sections_requires_query() {
        let state = Arc::new(AppState::new(reqwest::Client::new()));
        let doc = "# A\nHello\n\n## B\nWorld\n";
        let mut result = mk_response(doc);

        apply_relevant_section_extract_if_enabled(
            &state,
            &mut result,
            None,
            true,
            None,
            None,
        )
        .await;

        assert_eq!(result.clean_content, doc);
        assert!(result
            .warnings
            .iter()
            .any(|w| w == "section_extract_missing_query"));
    }

    #[test]
    fn test_rewrite_github_blob_url_rewrites_blob_pages() {
        let input = "https://github.com/microsoft/vscode/blob/main/README.md";
        let result = rewrite_url_for_clean_content(input);
        assert_eq!(
            result.as_deref(),
            Some("https://raw.githubusercontent.com/microsoft/vscode/main/README.md")
        );
    }

    #[test]
    fn test_rewrite_github_blob_url_nested_path() {
        let input = "https://github.com/user/repo/blob/feature/my-branch/src/main.rs";
        let result = rewrite_url_for_clean_content(input);
        assert_eq!(
            result.as_deref(),
            Some("https://raw.githubusercontent.com/user/repo/feature/my-branch/src/main.rs")
        );
    }

    #[test]
    fn test_rewrite_github_blob_url_ignores_non_blob() {
        assert!(rewrite_url_for_clean_content("https://github.com/user/repo").is_none());
        assert!(rewrite_url_for_clean_content("https://github.com/user/repo/issues/1").is_none());
        assert!(rewrite_url_for_clean_content("https://docs.python.org/3/").is_none());
        assert!(rewrite_url_for_clean_content("https://raw.githubusercontent.com/user/repo/main/f.rs").is_none());
    }

}
