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
use anyhow::{Context, Result};
use std::{collections::HashMap, collections::HashSet, sync::Arc, time::Instant};
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
            max_sources_per_hop: 10,
            max_chars_per_source: 20_000,
            max_concurrent: 3,
            use_proxy: false,
            quality_mode: None,
            relevance_threshold: Some(0.25),
        }
    }
}

fn normalize_query_for_dedupe(value: &str) -> String {
    value
        .trim()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn build_multi_dimensional_queries(original_query: &str, base_query: &str) -> Vec<String> {
    // Always produce exactly 3 orthogonal research angles derived from the query itself.
    // (1) Core concepts / state-of-the-art
    let core = format!("{} state of the art benchmarks architecture comparison", base_query);
    // (2) Implementation / best practices / production
    let implementation = format!(
        "{} implementation best practices production libraries frameworks",
        base_query
    );
    // (3) Edge cases / limitations / performance tradeoffs
    let edge_cases = format!(
        "{} limitations edge cases performance tradeoffs benchmarks",
        base_query
    );

    // Original query first to preserve intent.
    vec![
        original_query.trim().to_string(),
        core,
        implementation,
        edge_cases,
    ]
}



fn is_spammy_source(title: &str, content: &str, url: &str) -> bool {
    let t = title.to_lowercase();
    let c = content.to_lowercase();
    let u = url.to_lowercase();

    // Generic marketing / low-signal content indicators.
    let spam_markers = [
        "download brochure",
        "enroll now",
        "whatsapp us",
        "course overview",
        "career support",
        "get certified",
        "join our bootcamp",
        "limited seats",
        "free demo",
        "register now",
    ];
    spam_markers.iter().any(|m| c.contains(m) || t.contains(m) || u.contains(m))
}

fn domain_priority(url: &str) -> i32 {
    let u = url.to_lowercase();
    // Generic preference: authoritative docs/repos over social/marketing/course sites.
    let prefer = [
        ("github.com", 25),
        ("arxiv.org", 22),
        ("huggingface.co", 20),
        ("docs.", 18),
        ("developer.", 15),
        ("developers.", 15),
        ("learn.microsoft.com", 14),
        ("developers.google.com", 14),
        ("pytorch.org", 12),
        ("tensorflow.org", 12),
    ];
    for (needle, score) in prefer {
        if u.contains(needle) {
            return score;
        }
    }
    if u.contains("reddit.com") || u.contains("quora.com") {
        return -5;
    }
    0
}



fn synthesize_technical_report(query: &str, findings: &[DeepResearchSource]) -> Option<String> {
    if findings.is_empty() {
        return None;
    }

    // Generic 3-axis keyword buckets — no domain-specific assumptions.
    let keyword_buckets: &[(&str, &[&str])] = &[
        (
            "Core concepts / state-of-the-art",
            &[
                "architecture", "benchmark", "sota", "paper", "model",
                "algorithm", "approach", "method", "framework", "library",
            ],
        ),
        (
            "Implementation / production",
            &[
                "implementation", "sdk", "api", "code", "install", "config",
                "deploy", "integrate", "plugin", "package",
            ],
        ),
        (
            "Performance / tradeoffs",
            &[
                "latency", "throughput", "memory", "cpu", "gpu", "accuracy",
                "speed", "benchmark", "ms", "fps", "ram",
            ],
        ),
        (
            "Limitations / edge cases",
            &[
                "limitation", "drawback", "issue", "edge case", "failure",
                "problem", "caveat", "workaround", "tradeoff",
            ],
        ),
    ];

    let mut bucket_lines: Vec<(&str, Vec<String>)> = Vec::new();
    for (section, keywords) in keyword_buckets {
        let mut hits: Vec<String> = Vec::new();
        for f in findings {
            let hay = format!(
                "{} {} {}",
                f.title.to_lowercase(),
                f.url.to_lowercase(),
                f.relevant_content.to_lowercase()
            );
            if keywords.iter().any(|k| hay.contains(k)) && hits.len() < 3 {
                hits.push(format!("- [{}]({})", f.title, f.url));
            }
        }
        bucket_lines.push((section, hits));
    }

    let top_sources = findings
        .iter()
        .take(5)
        .map(|f| format!("- [{}]({}) (depth={}, ~{} words)", f.title, f.url, f.depth, f.word_count))
        .collect::<Vec<_>>()
        .join("\n");

    let mut sections = String::new();
    for (section, hits) in &bucket_lines {
        sections.push_str(&format!("\n## {}\n", section));
        if hits.is_empty() {
            sections.push_str("- _(no relevant sources found)_\n");
        } else {
            for h in hits {
                sections.push_str(h);
                sections.push('\n');
            }
        }
    }

    Some(format!(
        "# Fact-Sheet (heuristic_v1)\n\n**Query:** {}\n**Sources scraped:** {}\n{}\n## Top Sources\n{}\n",
        query,
        findings.len(),
        sections,
        top_sources
    ))
}

async fn llm_synthesize_report_openai(
    state: &Arc<AppState>,
    query: &str,
    findings: &[DeepResearchSource],
) -> Result<Option<String>> {
    let dr_cfg = &state.shadow_config.deep_research;

    // Guard: synthesis can be disabled via config or legacy env var.
    if !dr_cfg.resolve_synthesis_enabled() {
        return Ok(None);
    }

    // API key: shadowcrawl.json → OPENAI_API_KEY env var → skip synthesis.
    // Empty string is valid for key-less local endpoints (Ollama / LM Studio).
    let api_key = match dr_cfg.resolve_api_key() {
        Some(k) => k,
        None => return Ok(None), // no key configured anywhere — skip synthesis
    };

    // LLM endpoint + model: shadowcrawl.json → env vars → hardcoded defaults.
    let base_url = dr_cfg.resolve_base_url();
    let model = dr_cfg.resolve_model();

    let max_sources = dr_cfg.resolve_max_sources();
    let max_chars_per_source = dr_cfg.resolve_max_chars_per_source();

    let mut packed_sources = String::new();
    for (i, f) in findings.iter().take(max_sources).enumerate() {
        let mut snippet = f.relevant_content.clone();
        if snippet.chars().count() > max_chars_per_source {
            snippet = snippet.chars().take(max_chars_per_source).collect::<String>();
            snippet.push_str("\n…[truncated]\n");
        }

        packed_sources.push_str(&format!(
            "SOURCE {}\nurl: {}\ntitle: {}\ndepth: {}\ncontent:\n{}\n\n",
            i + 1,
            f.url,
            f.title,
            f.depth,
            snippet
        ));
    }

    if packed_sources.trim().is_empty() {
        return Ok(None);
    }

    let system_prompt = "You are a data-synthesis core operating for an AI agent. Your goal is maximum information density. Extract concrete facts, metrics, architectures, and code/tool names. STRICT RULE: NO introductions, NO conclusions, NO conversational filler. Use strict Markdown formats (bullet points, tables, code blocks).";
    let user_prompt = format!(
        "Synthesize a dense technical fact-sheet from these sources based on the query: {}.\nIgnore marketing fluff. Output only: key facts, metrics, architecture names, tool names, code references, known limitations.\n\nSources:\n{}",
        query, packed_sources
    );

    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));

    let body = serde_json::json!({
        "model": model,
        "temperature": 0.2,
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_prompt}
        ]
    });

    let builder = state.http_client.post(url).json(&body);
    // Only send Authorization header when a key is provided.
    // Key-less local endpoints (Ollama / LM Studio) work without it.
    let builder = if api_key.is_empty() {
        builder
    } else {
        builder.bearer_auth(api_key.trim())
    };
    let response = builder
        .send()
        .await
        .context("openai chat.completions request failed")?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!(
            "openai chat.completions failed: status={} body={}",
            status,
            text
        ));
    }

    let value: serde_json::Value = response
        .json()
        .await
        .context("openai response json parse failed")?;

    let content = value
        .get("choices")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    Ok(content)
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
    let mut skipped_spammy = 0usize;

    if config.use_proxy && state.proxy_manager.is_none() {
        warnings.push("use_proxy_requested_but_proxy_manager_unavailable".to_string());
    }

    // If proxy is requested but the pool is tiny, refill automatically using proxy_source.json.
    if config.use_proxy {
        if let Some(pm) = &state.proxy_manager {
            let refill = tokio::time::timeout(
                std::time::Duration::from_secs(45),
                pm.ensure_min_proxies(&state, 100, 30),
            )
            .await;

            match refill {
                Err(_) => {
                    warnings.push("proxy_pool_refill_timeout".to_string());
                }
                Ok(result) => match result {
                Ok(Some(stats)) => {
                    warnings.push(format!("proxy_pool_refilled:{}", stats));
                }
                Ok(None) => {}
                Err(e) => warnings.push(format!("proxy_pool_refill_failed:{}", e)),
                },
            }
        }
    }

    // ── Hop 1: expand the query into focused sub-queries ──────────────────
    let rewriter = QueryRewriter::new();
    let rewrite_result = rewriter.rewrite_query(&query);
    let base_query = rewrite_result.best_query().to_string();

    // Multi-dimensional rewriting: always include 3 angles.
    let mut hop_queries: Vec<String> = build_multi_dimensional_queries(&query, &base_query);

    // Also include any QueryRewriter suggestions (deduped, capped).
    for s in rewrite_result.suggestions.iter().take(4) {
        hop_queries.push(s.clone());
    }

    // Dedupe queries (case/whitespace-insensitive) and cap to avoid request flood.
    {
        let mut seen = HashSet::<String>::new();
        hop_queries.retain(|q| seen.insert(normalize_query_for_dedupe(q)));
        hop_queries.truncate(8);
    }

    all_sub_queries.extend(hop_queries.clone());
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
        let mut url_via_query: HashMap<String, String> = HashMap::new();

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
            let mut top = top;
            // Domain-based tie-breaker to prefer docs/repos.
            top.sort_by(|a, b| domain_priority(&b.url).cmp(&domain_priority(&a.url)));

            for r in top {
                if !r.url.is_empty() {
                    url_via_query.entry(r.url.clone()).or_insert_with(|| q.clone());
                    candidate_urls.push(r.url);
                }
            }
        }

        // Deduplicate against already-processed URLs.
        let new_urls: Vec<String> = candidate_urls
            .into_iter()
            .filter(|u| !u.is_empty() && u.starts_with("http") && all_urls_seen.insert(u.clone()))
            // Cap per hop to avoid overwhelming the scraper.
            .take(config.max_sources_per_hop * 3)
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

        // Dynamic relevance threshold: start from config, and if we end up with too many empty
        // outputs after shaving, we can relax (lower threshold) a bit on the remaining pages.
        let mut adaptive_threshold = config.relevance_threshold;
        let mut shaved_empty_count = 0usize;
        let mut shaved_attempted_count = 0usize;

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

            // For short pages, semantic shaving often removes too much signal; keep whole.
            let raw_word_count = raw_content.split_whitespace().count();

            // Apply semantic shave when the embedding model is available.
            let (relevant_content, kept, total) = if raw_word_count < 200 {
                (raw_content.clone(), 0, 0)
            } else if let Some(memory) = &state.memory {
                match memory.get_embedding_model().await {
                    Ok(model) => {
                        shaved_attempted_count += 1;

                        // Adapt threshold if we're dropping too much content.
                        let threshold = adaptive_threshold.or(Some(0.25));

                        match semantic_shave::semantic_shave(
                            model,
                            &raw_content,
                            &query,
                            threshold,
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

            if shaved_attempted_count > 0
                && relevant_content.trim().is_empty()
                && raw_word_count >= 200
            {
                shaved_empty_count += 1;
                // If more than 50% of attempted shaves become empty, relax threshold.
                if shaved_empty_count * 2 >= shaved_attempted_count {
                    adaptive_threshold = Some(
                        (adaptive_threshold.unwrap_or(0.25) * 0.85)
                            .clamp(0.15, 0.35),
                    );
                }
            }

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

            if is_spammy_source(&scrape.title, &relevant_content, &scrape.url) {
                skipped_spammy += 1;
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

            let scraped_url = scrape.url.clone();

            all_findings.push(DeepResearchSource {
                url: scraped_url.clone(),
                title: scrape.title,
                domain: scrape.domain,
                relevant_content,
                word_count,
                depth: current_depth,
                via_query: url_via_query.get(&scraped_url).cloned(),
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

    if skipped_spammy > 0 {
        warnings.push(format!("skipped_spammy_sources:{}", skipped_spammy));
    }

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

    let (synthesized_report, synthesis_method) = match llm_synthesize_report_openai(
        &state,
        &query,
        &all_findings,
    )
    .await
    {
        Ok(Some(report)) => (Some(report), Some("openai_chat_completions".to_string())),
        Ok(None) => {
            if state.shadow_config.deep_research.resolve_api_key().is_none() {
                warnings.push("synthesis_disabled_no_api_key".to_string());
            }
            (
                synthesize_technical_report(&query, &all_findings),
                Some("heuristic_v1".to_string()),
            )
        }
        Err(e) => {
            warnings.push(format!("synthesis_failed:{}", e));
            (
                synthesize_technical_report(&query, &all_findings),
                Some("heuristic_v1_fallback".to_string()),
            )
        }
    };

    // When LLM synthesis succeeds, clear key_findings to avoid sending redundant
    // token-heavy raw content back to the caller alongside the synthesized report.
    let llm_succeeded = synthesis_method
        .as_deref()
        .is_some_and(|m| m == "openai_chat_completions");
    let final_findings = if llm_succeeded { Vec::new() } else { all_findings };

    Ok(DeepResearchResult {
        query,
        depth_used: depth,
        sources_discovered,
        sources_scraped,
        key_findings: final_findings,
        synthesized_report,
        synthesis_method,
        all_urls,
        sub_queries: all_sub_queries,
        warnings,
        total_duration_ms: start.elapsed().as_millis() as u64,
    })
}
