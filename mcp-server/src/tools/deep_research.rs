/// 🔬 Deep Research — multi-hop search + scrape + semantic filtering pipeline.
///
/// The tool orchestrates:
///  1. Query expansion via `QueryRewriter`.
///  2. Multi-engine web search for each sub-query.
///  3. Reranking to select the most relevant candidate URLs.
///  4. Concurrent batch scraping of selected URLs.
///  5. Semantic chunk filtering (via Model2Vec) to keep only relevant content.
///  6. Optional deeper hops: links extracted from scraped pages drive the next
///     round of scraping, capped at `depth` hops.
///  7. Memory logging so `research_history` can recall the session.
use crate::{
    batch_scrape,
    nlp::semantic_shave,
    query_rewriter::QueryRewriter,
    rerank::Reranker,
    rust_scraper::QualityMode,
    search::search_web_with_params,
    types::{DeepResearchResult, DeepResearchSource, ScrapeBatchResponse},
    AppState,
};
use anyhow::Result;
use std::{collections::HashSet, sync::Arc, time::Instant};
use tracing::{info, warn};

// ─────────────────────────────────────────────────────────────────────────────
// Configuration
// ─────────────────────────────────────────────────────────────────────────────

/// Runtime configuration for a deep-research run.
pub struct DeepResearchConfig {
    /// Number of search + scrape hops (1..=3). Clamped at construction.
    pub depth: u8,
    /// Maximum sources to scrape per hop.
    pub max_sources_per_hop: usize,
    /// Maximum output characters per scraped source passed to `scrape_batch`.
    pub max_chars_per_source: usize,
    /// Maximum concurrent scrape connections.
    pub max_concurrent: usize,
    /// Route requests through the proxy manager.
    pub use_proxy: bool,
    /// Scraper quality mode (balanced / aggressive).
    pub quality_mode: Option<QualityMode>,
    /// Semantic shave threshold [0.0..1.0]. `None` = library default (0.35).
    pub relevance_threshold: Option<f32>,
}

impl Default for DeepResearchConfig {
    fn default() -> Self {
        Self {
            depth: 1,
            max_sources_per_hop: 5,
            max_chars_per_source: 8_000,
            max_concurrent: 3,
            use_proxy: false,
            quality_mode: None,
            relevance_threshold: None,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Core pipeline
// ─────────────────────────────────────────────────────────────────────────────

/// Execute the deep-research pipeline and return a structured report.
///
/// # Arguments
/// * `state`  — shared application state (HTTP client, caches, memory, proxies)
/// * `query`  — the research question / topic
/// * `config` — pipeline parameters (depth, source limits, proxy, quality)
pub async fn deep_research(
    state: Arc<AppState>,
    query: String,
    config: DeepResearchConfig,
) -> Result<DeepResearchResult> {
    let start = Instant::now();
    let depth = config.depth.clamp(1, 3);

    let mut all_findings: Vec<DeepResearchSource> = Vec::new();
    let mut all_urls_seen: HashSet<String> = HashSet::new();
    let mut all_sub_queries: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    // ── Hop 1: expand the query into focused sub-queries ──────────────────
    let rewriter = QueryRewriter::new();
    let rewrite_result = rewriter.rewrite_query(&query);
    let base_query = rewrite_result.best_query().to_string();
    all_sub_queries.push(base_query.clone());

    // `hop_queries` drives search on the current hop.
    // `hop_urls`   holds extra URLs to scrape directly (from prior-hop links).
    let mut hop_queries: Vec<String> = vec![base_query];
    let mut hop_urls: Vec<String> = Vec::new();

    for current_depth in 1..=depth {
        info!(
            "deep_research hop {}/{}: {} queries, {} link-URLs",
            current_depth,
            depth,
            hop_queries.len(),
            hop_urls.len()
        );

        // ── Search phase ─────────────────────────────────────────────────
        let mut candidate_urls: Vec<String> = hop_urls.clone();

        for q in &hop_queries {
            let results = match search_web_with_params(&state, q, None).await {
                Ok((r, _)) => r,
                Err(e) => {
                    warn!("deep_research search failed for '{}': {}", q, e);
                    warnings.push(format!("search_failed:{}", q));
                    continue;
                }
            };

            // Rerank for relevance and take top-K URLs.
            let reranker = Reranker::new(q);
            let top = reranker.rerank_top(results, config.max_sources_per_hop);
            for r in top {
                if !r.url.is_empty() {
                    candidate_urls.push(r.url);
                }
            }
        }

        // Deduplicate against already-processed URLs.
        let new_urls: Vec<String> = candidate_urls
            .into_iter()
            .filter(|u| !u.is_empty() && u.starts_with("http") && all_urls_seen.insert(u.clone()))
            // Cap per hop to avoid overwhelming the scraper.
            .take(config.max_sources_per_hop * 2)
            .collect();

        if new_urls.is_empty() {
            info!(
                "deep_research hop {}: no new URLs — stopping early",
                current_depth
            );
            break;
        }

        // ── Batch scrape ─────────────────────────────────────────────────
        let batch: ScrapeBatchResponse = match batch_scrape::scrape_batch(
            &state,
            new_urls.clone(),
            config.max_concurrent,
            Some(config.max_chars_per_source),
            config.use_proxy,
            config.quality_mode.clone(),
        )
        .await
        {
            Ok(b) => b,
            Err(e) => {
                warn!("deep_research batch scrape hop {}: {}", current_depth, e);
                warnings.push(format!("batch_scrape_failed_hop{}:{}", current_depth, e));
                break;
            }
        };

        // ── Semantic shave + collect findings ─────────────────────────────
        let mut next_hop_urls: Vec<String> = Vec::new();

        for result in batch.results {
            let Some(scrape) = result.data else {
                continue;
            };

            // Prefer clean_content; fall back to raw content.
            let raw_content = if !scrape.clean_content.is_empty() {
                scrape.clean_content.clone()
            } else {
                scrape.content.clone()
            };

            // Apply semantic shave when the embedding model is available.
            let (relevant_content, kept, total) = if let Some(memory) = &state.memory {
                match memory.get_embedding_model().await {
                    Ok(model) => {
                        match semantic_shave::semantic_shave(
                            model,
                            &raw_content,
                            &query,
                            config.relevance_threshold,
                        )
                        .await
                        {
                            Ok(shaved) => shaved,
                            Err(e) => {
                                warn!("semantic_shave failed for {}: {}", scrape.url, e);
                                (raw_content.clone(), 0, 0)
                            }
                        }
                    }
                    Err(_) => (raw_content.clone(), 0, 0),
                }
            } else {
                (raw_content.clone(), 0, 0)
            };

            if total > 0 {
                info!(
                    "deep_research semantic_shave: {}/{} chunks kept for {}",
                    kept, total, scrape.url
                );
            }

            // Skip sources that ended up with no content after shaving.
            if relevant_content.trim().is_empty() {
                continue;
            }

            let word_count = relevant_content.split_whitespace().count();

            // Collect links from this page to feed the next hop.
            if current_depth < depth {
                for link in &scrape.links {
                    if link.url.starts_with("http") {
                        next_hop_urls.push(link.url.clone());
                    }
                }
            }

            all_findings.push(DeepResearchSource {
                url: scrape.url,
                title: scrape.title,
                domain: scrape.domain,
                relevant_content,
                word_count,
                depth: current_depth,
                via_query: hop_queries.first().cloned(),
            });
        }

        // ── Prepare next hop ──────────────────────────────────────────────
        // For hops > 1 we scrape discovered links directly (no new search).
        hop_queries.clear();
        hop_urls = next_hop_urls
            .into_iter()
            .filter(|u| all_urls_seen.insert(u.clone()))
            .take(config.max_sources_per_hop * 3)
            .collect();
    }

    // Sort findings: most-content first acts as a rough relevance proxy when
    // the embedding model is absent; with shaving enabled the ordering already
    // reflects semantic density.
    all_findings.sort_by(|a, b| b.word_count.cmp(&a.word_count));

    let all_urls: Vec<String> = all_urls_seen.into_iter().collect();
    let sources_discovered = all_urls.len();
    let sources_scraped = all_findings.len();

    // ── Log session to persistent memory ─────────────────────────────────
    if let Some(memory) = &state.memory {
        let preview_json = serde_json::json!({
            "sources": sources_scraped,
            "top_sources": all_findings.iter().take(3).map(|f| &f.url).collect::<Vec<_>>(),
        });
        let _ = memory
            .log_search(query.clone(), &preview_json, sources_scraped)
            .await;
    }

    Ok(DeepResearchResult {
        query,
        depth_used: depth,
        sources_discovered,
        sources_scraped,
        key_findings: all_findings,
        all_urls,
        sub_queries: all_sub_queries,
        warnings,
        total_duration_ms: start.elapsed().as_millis() as u64,
    })
}
