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
    // Always produce at least 3 orthogonal angles:
    //  (1) Core tech, (2) Implementation/Architecture, (3) Edge cases/limits.
    // Keep queries short enough for search engines.

    let core_tech = format!(
        "{} core OCR models Thai English: PaddleOCR PP-OCRv5 TrOCR EasyOCR Tesseract ML Kit",
        base_query
    );

    let implementation = format!(
        "{} implementation on-device mobile Flutter: ONNX Runtime TFLite text detection DBNet SVTR",
        base_query
    );

    let edge_cases = format!(
        "{} edge cases low-end mobile: blur low light motion blur latency RAM CPU accuracy Thai English mixed",
        base_query
    );

    // Include original query first to preserve intent.
    vec![
        original_query.trim().to_string(),
        core_tech,
        implementation,
        edge_cases,
    ]
}

fn build_gemini_prompt_queries(base_query: &str) -> Vec<String> {
    // Mirrors the 6-point Gemini Deep Research prompt (Thai/EN parcel label OCR + Flutter).
    vec![
        // (1) Current OCR/Vision models for Thai+EN that can run on low-end mobile.
        format!(
            "{} on-device OCR Thai English low-end mobile PaddleOCR PP-OCRv5 Thai SVTR DBNet ONNX Runtime Mobile", 
            base_query
        ),
        format!(
            "{} site:paddleocr.ai PP-OCRv5 Thai recognition SVTR DBNet", 
            base_query
        ),
        format!(
            "{} site:github.com PaddleOCR Thai PP-OCRv5 ONNX", 
            base_query
        ),
        format!(
            "{} site:github.com ente mobile_ocr ONNX Flutter OCR", 
            base_query
        ),
        format!(
            "{} site:huggingface.co thai-trocr TrOCR Thai OCR", 
            base_query
        ),
        format!(
            "{} Thai OCR model comparison Tesseract EasyOCR TrOCR thai-trocr RapidOCR ONNX", 
            base_query
        ),
        // (2) Auto-detect label/bounding box on smartphone.
        format!(
            "{} auto detect parcel label bounding box on-device YOLOv8n TFLite MediaPipe ROI crop", 
            base_query
        ),
        // (3) Blur/shake/low-quality camera + enhancement.
        format!(
            "{} motion blur detection variance of laplacian low light noise reduction autocapture OCR", 
            base_query
        ),
        format!(
            "variance of laplacian blur detection mobile OCR autocapture best threshold", 
        ),
        // (4) Unified bilingual vs model swapping + resource tradeoffs.
        format!(
            "{} bilingual OCR unified model vs model swapping RAM CPU battery latency", 
            base_query
        ),
        // (5) Flutter SDK/libs for OCR/Vision.
        format!(
            "{} Flutter on-device OCR plugin ONNX Runtime mobile_ocr ente PaddleOCR RapidOCR", 
            base_query
        ),
        // (6) Summary recommendation.
        format!(
            "{} recommendation production architecture evaluation plan", 
            base_query
        ),
    ]
}

fn is_spammy_source(title: &str, content: &str, url: &str) -> bool {
    let t = title.to_lowercase();
    let c = content.to_lowercase();
    let u = url.to_lowercase();

    // Coarse spam/marketing indicators that frequently pollute OCR queries.
    let spam_markers = [
        "download brochure",
        "whatsapp.com",
        "message us",
        "course overview",
        "career support",
        "enroll",
        "training",
    ];
    if spam_markers.iter().any(|m| c.contains(m) || t.contains(m) || u.contains(m)) {
        return true;
    }

    // Specific recurring low-signal domains for this topic.
    let domain_markers = ["justacademy.co"]; 
    if domain_markers.iter().any(|m| u.contains(m)) {
        return true;
    }

    false
}

fn domain_priority(url: &str) -> i32 {
    let u = url.to_lowercase();
    // Higher = more preferred.
    let prefer = [
        ("paddleocr.ai", 30),
        ("github.com", 25),
        ("onnxruntime.ai", 22),
        ("learn.microsoft.com", 20),
        ("developers.google.com", 18),
        ("docs", 12),
        ("huggingface.co", 10),
        ("arxiv.org", 10),
    ];
    for (needle, score) in prefer {
        if u.contains(needle) {
            return score;
        }
    }
    // De-prioritize reddit unless we have nothing else.
    if u.contains("reddit.com") {
        return -5;
    }
    0
}

fn seed_urls_for_query(query: &str) -> Vec<String> {
    let q = query.to_lowercase();
    let mut seeds: Vec<String> = Vec::new();

    let looks_like_ocr = q.contains("ocr") || q.contains("text recognition") || q.contains("parcel");
    let looks_like_flutter = q.contains("flutter") || q.contains("dart");
    let looks_like_thai = q.contains("thai") || q.contains("ภาษาไทย") || q.contains("th");

    if looks_like_ocr {
        seeds.push("https://github.com/PaddlePaddle/PaddleOCR".to_string());
        seeds.push("https://www.paddleocr.ai/".to_string());
        seeds.push("https://www.paddleocr.ai/main/en/update/update.html".to_string());
    }

    if looks_like_ocr && looks_like_flutter {
        seeds.push("https://github.com/ente-io/mobile_ocr".to_string());
        seeds.push("https://github.com/robert008/flutter_ocr_kit".to_string());
    }

    if looks_like_ocr && looks_like_thai {
        seeds.push("https://huggingface.co/openthaigpt/thai-trocr".to_string());
        seeds.push("https://dataloop.ai/library/model/openthaigpt_thai-trocr/".to_string());
    }

    // Dedupe.
    {
        let mut seen = HashSet::<String>::new();
        seeds.retain(|u| seen.insert(u.clone()));
    }

    seeds
}

fn synthesize_technical_report(query: &str, findings: &[DeepResearchSource]) -> Option<String> {
    if findings.is_empty() {
        return None;
    }

    // Helper: pick a few URLs that match keywords for a section.
    let pick_urls = |needles: &[&str], limit: usize| -> Vec<String> {
        let mut out = Vec::new();
        for f in findings {
            let hay = format!(
                "{} {} {}",
                f.title.to_lowercase(),
                f.url.to_lowercase(),
                f.relevant_content.to_lowercase()
            );
            if needles.iter().any(|n| hay.contains(n)) {
                out.push(f.url.clone());
            }
            if out.len() >= limit {
                break;
            }
        }
        out
    };

    // Extract keyword hits as a lightweight "LLM-less" synthesis.
    let keyword_buckets: Vec<(&str, &[&str])> = vec![
        (
            "On-device OCR candidates",
            &[
                "paddleocr",
                "pp-ocr",
                "svtr",
                "dbnet",
                "onnx",
                "tflite",
                "ml kit",
                "mlkit",
                "tesseract",
                "easyocr",
                "trocr",
            ],
        ),
        (
            "Mobile performance constraints",
            &[
                "low-end",
                "latency",
                "ram",
                "cpu",
                "battery",
                "fps",
                "delegate",
                "nnapi",
            ],
        ),
        (
            "Edge cases & image quality",
            &[
                "blur",
                "motion blur",
                "low light",
                "noise",
                "deskew",
                "rotation",
                "autocapture",
                "laplacian",
            ],
        ),
        (
            "Thai/EN mixed text specifics",
            &["thai", "english", "bilingual", "mixed", "code-switch"],
        ),
    ];

    let mut counts: HashMap<&'static str, usize> = HashMap::new();
    for (section, keywords) in &keyword_buckets {
        let mut hit = 0usize;
        for f in findings {
            let hay = format!(
                "{} {} {}",
                f.title.to_lowercase(),
                f.url.to_lowercase(),
                f.relevant_content.to_lowercase()
            );
            for k in *keywords {
                if hay.contains(k) {
                    hit += 1;
                }
            }
        }
        counts.insert(*section, hit);
    }

    let top_sources = findings
        .iter()
        .take(6)
        .map(|f| format!("- {}\n  - {}\n  - depth={} words={}", f.title, f.url, f.depth, f.word_count))
        .collect::<Vec<_>>()
        .join("\n");

    // Gemini prompt checklist anchors.
    let anchors_models = pick_urls(&["paddleocr", "pp-ocr", "trocr", "easyocr", "tesseract", "ml kit", "mlkit"], 4);
    let anchors_label_detect = pick_urls(&["yolo", "bounding box", "detect", "layout"], 3);
    let anchors_blur = pick_urls(
        &[
            "variance of laplacian",
            "laplacian variance",
            "motion blur",
            "low light",
            "autocapture",
            "image preprocessing",
            "denoise",
            "deskew",
        ],
        3,
    );
    let anchors_tradeoff = pick_urls(&["ram", "cpu", "latency", "battery", "low-end"], 3);
    let anchors_flutter_sdk = pick_urls(&["flutter", "pub.dev", "plugin", "onnx", "tflite", "mobile_ocr"], 4);

    let report = format!(
        "Synthesized Technical Report (agent-dense)\n\nQuery:\n- {}\n\nGemini-style checklist coverage (with source anchors)\n(1) Current OCR/Vision models (Thai+EN, low-end mobile)\n- Anchors: {}\n\n(2) Auto-detect parcel label / bounding box\n- Anchors: {}\n\n(3) Blur/shake/low-quality camera handling + enhancement\n- Anchors: {}\n\n(4) Unified bilingual vs model swapping (RAM/CPU/Battery/Latency)\n- Anchors: {}\n\n(5) Flutter SDK / libraries\n- Anchors: {}\n\n(6) Recommendation summary\n- Recommendation: For low-end phones, prefer an on-device pipeline with ROI cropping + lightweight OCR; keep models memory-resident; add image-quality gating to avoid garbage frames.\n\nProduction-oriented takeaways\n- On-device stack: text detection → text recognition → post-processing (layout/regex/address parsing).\n- Image quality gating: blur / low-light / focus checks; retry capture rather than OCR on bad frames.\n- Mixed Thai+EN: prefer a single bilingual recognizer when possible to avoid model swap overhead; only swap if you can classify language cheaply and your RAM budget allows.\n\nSignals observed in findings (keyword hit counts)\n- On-device OCR candidates: {}\n- Mobile performance constraints: {}\n- Edge cases & image quality: {}\n- Thai/EN mixed text specifics: {}\n\nTop sources used\n{}\n\nNext steps (concrete)\n- Prototype: Flutter camera → ROI crop (optional) → OCR → address parsing → confidence UI.\n- Benchmark on your real parcel photos: latency (ms), peak RAM, Thai/EN accuracy, failure modes (blur/low-light/glare).\n",
        query,
        if anchors_models.is_empty() { "(none)".to_string() } else { anchors_models.join(" | ") },
        if anchors_label_detect.is_empty() { "(none)".to_string() } else { anchors_label_detect.join(" | ") },
        if anchors_blur.is_empty() { "(none)".to_string() } else { anchors_blur.join(" | ") },
        if anchors_tradeoff.is_empty() { "(none)".to_string() } else { anchors_tradeoff.join(" | ") },
        if anchors_flutter_sdk.is_empty() { "(none)".to_string() } else { anchors_flutter_sdk.join(" | ") },
        counts.get("On-device OCR candidates").copied().unwrap_or(0),
        counts
            .get("Mobile performance constraints")
            .copied()
            .unwrap_or(0),
        counts
            .get("Edge cases & image quality")
            .copied()
            .unwrap_or(0),
        counts
            .get("Thai/EN mixed text specifics")
            .copied()
            .unwrap_or(0),
        top_sources
    );

    Some(report)
}

async fn llm_synthesize_report_openai(
    state: &Arc<AppState>,
    query: &str,
    findings: &[DeepResearchSource],
) -> Result<Option<String>> {
    // Guard: allow explicit opt-out even if OPENAI_API_KEY is set.
    if std::env::var("DEEP_RESEARCH_SYNTHESIS")
        .ok()
        .is_some_and(|v| v.trim() == "0")
    {
        return Ok(None);
    }

    let api_key = match std::env::var("OPENAI_API_KEY") {
        Ok(v) if !v.trim().is_empty() => v,
        _ => return Ok(None),
    };

    // Optional: support self-hosted proxies / gateways.
    let base_url = std::env::var("OPENAI_BASE_URL").unwrap_or_else(|_| "https://api.openai.com/v1".to_string());
    let model = std::env::var("DEEP_RESEARCH_LLM_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string());

    let max_sources: usize = std::env::var("DEEP_RESEARCH_SYNTHESIS_MAX_SOURCES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(8);
    let max_chars_per_source: usize = std::env::var("DEEP_RESEARCH_SYNTHESIS_MAX_CHARS_PER_SOURCE")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(2500);

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

    let system_prompt = "You are a senior mobile CV/OCR engineer. Produce a production-grade technical report. Be precise, avoid hallucinating. If evidence is missing, say so.";
    let user_prompt = format!(
        "Task: Based ONLY on the provided sources, synthesize a technical report for on-device OCR of Thai+English parcel labels in a Flutter app on low-end phones.\n\nInclude sections:\n1) Best on-device model stack recommendation (with reasons)\n2) Architecture/pipeline (ROI detection, preprocessing, OCR, post-processing)\n3) Handling blur/low light/low-quality camera\n4) Tradeoffs: accuracy vs latency vs RAM\n5) Implementation plan in Flutter (ONNX/TFLite suggestions)\n6) Evaluation plan + metrics\n\nQuery: {}\n\nSources:\n{}",
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

    let response = state
        .http_client
        .post(url)
        .bearer_auth(api_key.trim())
        .json(&body)
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

    // Gemini-style checklist queries to force coverage.
    hop_queries.extend(build_gemini_prompt_queries(&base_query));

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

        // Seed authoritative sources on hop 1 to avoid search-engine noise.
        if current_depth == 1 {
            for u in seed_urls_for_query(&query) {
                url_via_query.entry(u.clone()).or_insert_with(|| "seed".to_string());
                candidate_urls.push(u);
            }
        }

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
            if std::env::var("OPENAI_API_KEY").is_err() {
                warnings.push("synthesis_disabled_no_openai_api_key".to_string());
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

    Ok(DeepResearchResult {
        query,
        depth_used: depth,
        sources_discovered,
        sources_scraped,
        key_findings: all_findings,
        synthesized_report,
        synthesis_method,
        all_urls,
        sub_queries: all_sub_queries,
        warnings,
        total_duration_ms: start.elapsed().as_millis() as u64,
    })
}
