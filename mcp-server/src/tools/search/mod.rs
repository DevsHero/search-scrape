pub mod engines;
mod service;

use crate::query_rewriter::{QueryRewriteResult, QueryRewriter};
use crate::rerank::Reranker;
use crate::types::*;
use crate::AppState;
use anyhow::{anyhow, Result};
use futures::future::join_all;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info, warn};

pub use service::SearchService;

#[derive(Debug, Default, Clone)]
pub struct SearchParamOverrides {
    pub engines: Option<String>,    // comma-separated list
    pub categories: Option<String>, // comma-separated list
    pub language: Option<String>,   // e.g., "en" or "en-US"
    pub safesearch: Option<u8>,     // 0,1,2
    pub time_range: Option<String>, // e.g., day, week, month, year
    pub pageno: Option<u32>,        // 1..N
}

#[derive(Debug, Default, Clone)]
pub struct SearchExtras {
    pub answers: Vec<String>,
    pub suggestions: Vec<String>,
    pub corrections: Vec<String>,
    pub unresponsive_engines: Vec<String>,
    pub query_rewrite: Option<QueryRewriteResult>,
    pub duplicate_warning: Option<String>,
}

pub struct InternalSearchService;

impl Default for InternalSearchService {
    fn default() -> Self {
        Self::new()
    }
}

impl InternalSearchService {
    pub fn new() -> Self {
        Self
    }

    fn parse_engine_list(engines: Option<String>) -> Vec<String> {
        engines
            .unwrap_or_else(|| {
                std::env::var("SEARCH_ENGINES")
                    .unwrap_or_else(|_| "google,bing,duckduckgo,brave".to_string())
            })
            .split(',')
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| !s.is_empty())
            .collect()
    }

    async fn run_engine(
        &self,
        state: &Arc<AppState>,
        engine: &str,
        query: &str,
        max_results: usize,
    ) -> Vec<SearchResult> {
        let client = &state.http_client;
        let timeout = engine_timeout(engine);

        let fut = async {
            match engine {
                "duckduckgo" | "ddg" => {
                    engines::duckduckgo::search(client, query, max_results).await
                }
                "bing" => engines::bing::search(client, query, max_results).await,
                "google" => engines::google::search(client, query, max_results).await,
                "brave" => engines::brave::search(client, query, max_results).await,
                other => {
                    debug!("unknown search engine requested: {}", other);
                    Ok(Vec::new())
                }
            }
        };

        let res = match tokio::time::timeout(timeout, fut).await {
            Ok(v) => v,
            Err(_) => {
                warn!(
                    "engine '{}' timed out after {}ms (tail latency pruned)",
                    engine,
                    timeout.as_millis()
                );
                return Vec::new();
            }
        };

        match res {
            Ok(v) => v,
            Err(engines::EngineError::Blocked { reason }) => {
                warn!("engine '{}' blocked: {}", engine, reason);
                self.tier2_non_robot_fallback(state, engine, query, max_results)
                    .await
                    .unwrap_or_default()
            }
            Err(e) => {
                warn!("engine '{}' failed: {}", engine, e);
                Vec::new()
            }
        }
    }

    #[cfg(feature = "non_robot_search")]
    async fn tier2_non_robot_fallback(
        &self,
        state: &Arc<AppState>,
        engine: &str,
        query: &str,
        max_results: usize,
    ) -> Option<Vec<SearchResult>> {
        use crate::features::non_robot_search::{execute_non_robot_search, NonRobotSearchConfig};
        use crate::rust_scraper::QualityMode;

        // Best-effort: only when explicitly enabled (to avoid unexpected HITL prompts).
        if !std::env::var("SEARCH_TIER2_NON_ROBOT")
            .unwrap_or_else(|_| "true".to_string())
            .eq_ignore_ascii_case("true")
        {
            return None;
        }

        let url = match engine {
            "duckduckgo" | "ddg" => {
                let mut u = reqwest::Url::parse("https://duckduckgo.com/html/").ok()?;
                u.query_pairs_mut().append_pair("q", query);
                u
            }
            "bing" => {
                let mut u = reqwest::Url::parse("https://www.bing.com/search").ok()?;
                u.query_pairs_mut().append_pair("q", query);
                u
            }
            "google" => {
                let mut u = reqwest::Url::parse("https://www.google.com/search").ok()?;
                u.query_pairs_mut().append_pair("q", query);
                u.query_pairs_mut().append_pair("hl", "en");
                u.query_pairs_mut()
                    .append_pair("num", &max_results.clamp(5, 10).to_string());
                u
            }
            "brave" => {
                let mut u = reqwest::Url::parse("https://search.brave.com/search").ok()?;
                u.query_pairs_mut().append_pair("q", query);
                u
            }
            _ => return None,
        };

        let cfg = NonRobotSearchConfig {
            url: url.to_string(),
            max_chars: 400_000,
            use_proxy: false,
            quality_mode: QualityMode::Balanced,
            captcha_grace: std::time::Duration::from_secs(5),
            human_timeout: std::time::Duration::from_secs(60),
            user_profile_path: None,
            auto_scroll: false,
            wait_for_selector: None,
        };

        match execute_non_robot_search(state, cfg).await {
            Ok(scraped) => {
                let html = scraped.content;
                let parsed = match engine {
                    "duckduckgo" | "ddg" => engines::duckduckgo::parse_results(&html, max_results),
                    "bing" => engines::bing::parse_results(&html, max_results),
                    "google" => engines::google::parse_results(&html, max_results),
                    "brave" => engines::brave::parse_results(&html, max_results),
                    _ => Vec::new(),
                };
                if parsed.is_empty() {
                    warn!(
                        "tier2 fallback got HTML but parsed 0 results for engine '{}'",
                        engine
                    );
                }
                Some(parsed)
            }
            Err(e) => {
                warn!("tier2 non_robot_search fallback failed: {}", e);
                None
            }
        }
    }

    #[cfg(not(feature = "non_robot_search"))]
    async fn tier2_non_robot_fallback(
        &self,
        _state: &Arc<AppState>,
        _engine: &str,
        _query: &str,
        _max_results: usize,
    ) -> Option<Vec<SearchResult>> {
        None
    }
}

#[async_trait::async_trait]
impl SearchService for InternalSearchService {
    async fn search(
        &self,
        state: &Arc<AppState>,
        query: &str,
        overrides: Option<SearchParamOverrides>,
    ) -> Result<Vec<SearchResult>> {
        let mut engines_override = overrides.as_ref().and_then(|o| o.engines.clone());

        // Context-based forcing (roughly equivalent to the legacy external search engine forcing).
        let query_lower = query.to_lowercase();
        let mut effective_query = query.to_string();
        if engines_override.is_none()
            && (query_lower.contains("github")
                || query_lower.contains("repo")
                || query_lower.contains("repository"))
        {
            effective_query = format!("{} site:github.com", query);
        } else if engines_override.is_none()
            && (query_lower.contains("stackoverflow") || query_lower.contains("stack overflow"))
        {
            effective_query = format!("{} site:stackoverflow.com", query);
        }

        let engine_list = Self::parse_engine_list(engines_override.take());
        let max_results = std::env::var("SEARCH_MAX_RESULTS_PER_ENGINE")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(10);

        // Run all engines in parallel (Tier 1); each engine can still do best-effort Tier 2 fallback.
        let engine_futs = engine_list
            .iter()
            .map(|engine| self.run_engine(state, engine.as_str(), &effective_query, max_results));
        let engine_batches: Vec<Vec<SearchResult>> = join_all(engine_futs).await;
        let mut results: Vec<SearchResult> = engine_batches.into_iter().flatten().collect();

        // Optional "community" expansion, similar intent to old SEARCH_COMMUNITY_SOURCES.
        if std::env::var("SEARCH_COMMUNITY_SOURCES")
            .unwrap_or_else(|_| "true".to_string())
            .to_lowercase()
            == "true"
        {
            let community_query = format!(
                "{} (site:reddit.com OR site:news.ycombinator.com)",
                effective_query
            );

            let community_engines = Self::parse_engine_list(None);
            let community_futs = community_engines.iter().map(|engine| {
                self.run_engine(state, engine.as_str(), &community_query, max_results)
            });
            let community_batches: Vec<Vec<SearchResult>> = join_all(community_futs).await;
            results.extend(community_batches.into_iter().flatten());
        }

        Ok(dedup_and_score_results(results, query))
    }
}

fn dedup_and_score_results(results: Vec<SearchResult>, query: &str) -> Vec<SearchResult> {
    #[derive(Default)]
    struct Acc {
        result: SearchResult,
        engines: HashSet<String>,
    }

    let mut map: HashMap<String, Acc> = HashMap::new();
    for mut r in results {
        // Normalize engine source fields (older callers may only set `engine`).
        if r.engine_source.is_none() {
            r.engine_source = r.engine.clone();
        }
        if r.engine_sources.is_empty() {
            if let Some(ref e) = r.engine_source {
                r.engine_sources = vec![e.clone()];
            }
        }

        if r.breadcrumbs.is_empty() {
            r.breadcrumbs = breadcrumbs_from_url(&r.url);
        }

        let engine = r
            .engine_source
            .clone()
            .unwrap_or_else(|| "unknown".to_string());
        let key = normalize_url_key(&r.url);

        map.entry(key)
            .and_modify(|acc| {
                acc.engines.insert(engine.clone());

                // Keep a full set of corroborating engines.
                for src in &r.engine_sources {
                    if !src.trim().is_empty() {
                        acc.engines.insert(src.clone());
                    }
                }

                if acc.result.title.trim().is_empty() && !r.title.trim().is_empty() {
                    acc.result.title = std::mem::take(&mut r.title);
                }
                if acc.result.content.trim().is_empty() && !r.content.trim().is_empty() {
                    acc.result.content = std::mem::take(&mut r.content);
                }

                // Prefer richer metadata if available.
                if acc.result.domain.is_none() {
                    acc.result.domain = r.domain.clone();
                }
                if acc.result.source_type.is_none() {
                    acc.result.source_type = r.source_type.clone();
                }

                if acc.result.published_at.is_none() {
                    acc.result.published_at = r.published_at.clone();
                }

                if acc.result.rich_snippet.is_none() {
                    acc.result.rich_snippet = r.rich_snippet.clone();
                }

                if acc.result.top_answer.is_none() {
                    acc.result.top_answer = r.top_answer.clone();
                }

                if acc.result.breadcrumbs.is_empty() && !r.breadcrumbs.is_empty() {
                    acc.result.breadcrumbs = r.breadcrumbs.clone();
                } else if !r.breadcrumbs.is_empty() {
                    // Union, keep order stable-ish.
                    let mut seen = HashSet::new();
                    let mut merged = Vec::new();
                    for b in acc.result.breadcrumbs.iter().chain(r.breadcrumbs.iter()) {
                        let k = b.trim().to_ascii_lowercase();
                        if k.is_empty() {
                            continue;
                        }
                        if seen.insert(k) {
                            merged.push(b.clone());
                        }
                    }
                    acc.result.breadcrumbs = merged;
                }
            })
            .or_insert_with(|| {
                let mut engines = HashSet::new();
                engines.insert(engine.clone());
                Acc { result: r, engines }
            });
    }

    let mut out: Vec<SearchResult> = map
        .into_values()
        .map(|mut acc| {
            let engine_count = acc.engines.len().max(1);
            let mut engine_sources: Vec<String> = acc.engines.into_iter().collect();
            engine_sources.sort();

            // Confidence scoring: multi-engine corroboration + domain/source + breadcrumbs + recency.
            // This score is intentionally coarse; semantic reranker still runs later.
            let corroboration_bonus = (engine_count as f64 - 1.0).max(0.0) * 0.35;
            let mut domain_weight =
                domain_weight(query, &acc.result.domain, &acc.result.source_type);
            if breadcrumbs_have_high_value_keywords(&acc.result.breadcrumbs) {
                domain_weight *= 1.20;
            }

            let recency_bonus = recency_bonus(&acc.result.published_at);
            let base = 1.0 * domain_weight + corroboration_bonus + recency_bonus;
            acc.result.score = Some(base);

            acc.result.engine_source = if engine_count == 1 {
                engine_sources.first().cloned()
            } else {
                None
            };

            acc.result.engine = Some(if engine_count == 1 {
                engine_sources
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "unknown".to_string())
            } else {
                format!("multi:{}", engine_sources.join(","))
            });

            acc.result.engine_sources = engine_sources;

            acc.result
        })
        .collect();

    out.sort_by(|a, b| {
        b.score
            .unwrap_or(0.0)
            .partial_cmp(&a.score.unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out
}

fn engine_timeout(engine: &str) -> Duration {
    let base_default_ms = std::env::var("SEARCH_ENGINE_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(2_500);

    // Built-in per-engine defaults (can be overridden by env).
    let builtin_ms = match engine {
        "duckduckgo" | "ddg" => 4_500,
        "brave" => 3_500,
        _ => base_default_ms,
    };

    let key = format!("SEARCH_ENGINE_TIMEOUT_MS_{}", engine.to_ascii_uppercase());
    let ms = std::env::var(key)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(builtin_ms);

    Duration::from_millis(ms.max(250))
}

pub(crate) fn split_date_prefix(snippet: &str) -> (Option<String>, String) {
    let s = snippet.trim();
    if s.is_empty() {
        return (None, String::new());
    }

    // Common patterns:
    // - "Jan 10, 2024 — ..."
    // - "2024-01-10 · ..."
    // - "January 10, 2024 - ..."
    let patterns = [
        r"^(20\d{2}-\d{2}-\d{2})\s*(?:[-—·|]|\u00b7)\s+(.+)$",
        r"^((?:Jan(?:uary)?|Feb(?:ruary)?|Mar(?:ch)?|Apr(?:il)?|May|Jun(?:e)?|Jul(?:y)?|Aug(?:ust)?|Sep(?:tember)?|Oct(?:ober)?|Nov(?:ember)?|Dec(?:ember)?)\s+\d{1,2},\s+20\d{2})\s*(?:[-—·|]|\u00b7)\s+(.+)$",
    ];

    for pat in patterns {
        if let Ok(re) = regex::Regex::new(pat) {
            if let Some(cap) = re.captures(s) {
                let date = cap.get(1).map(|m| m.as_str().to_string());
                let rest = cap
                    .get(2)
                    .map(|m| m.as_str().trim().to_string())
                    .unwrap_or_default();
                if !rest.is_empty() {
                    return (date, rest);
                }
            }
        }
    }

    (None, s.to_string())
}

pub(crate) fn breadcrumbs_from_url(url_str: &str) -> Vec<String> {
    let Ok(url) = url::Url::parse(url_str) else {
        return Vec::new();
    };

    let mut parts = Vec::new();
    if let Some(host) = url.host_str() {
        parts.push(host.to_string());
    }

    let mut segs = url
        .path_segments()
        .map(|s| {
            s.filter(|p| !p.trim().is_empty())
                .take(3)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for s in segs.drain(..) {
        parts.push(s.to_string());
    }
    parts
}

pub(crate) fn extract_published_at_from_text(text: &str) -> Option<String> {
    // Best-effort: pull the first date-like token from a snippet.
    // We intentionally return a string for downstream use and optional parsing.
    let t = text.trim();
    if t.is_empty() {
        return None;
    }

    // Fast path: ISO-like date.
    if let Some(m) = regex::Regex::new(r"\b(20\d{2}-\d{2}-\d{2})\b")
        .ok()
        .and_then(|re| re.find(t))
    {
        return Some(m.as_str().to_string());
    }

    // Month name patterns: "Jan 2, 2026" or "January 2, 2026".
    if let Some(m) = regex::Regex::new(
        r"\b(Jan(?:uary)?|Feb(?:ruary)?|Mar(?:ch)?|Apr(?:il)?|May|Jun(?:e)?|Jul(?:y)?|Aug(?:ust)?|Sep(?:tember)?|Oct(?:ober)?|Nov(?:ember)?|Dec(?:ember)?)\s+\d{1,2},\s+20\d{2}\b",
    )
    .ok()
    .and_then(|re| re.find(t))
    {
        return Some(m.as_str().to_string());
    }

    None
}

fn breadcrumbs_have_high_value_keywords(breadcrumbs: &[String]) -> bool {
    let needles = [
        "docs",
        "documentation",
        "manual",
        "reference",
        "api",
        "github",
        "wiki",
    ];
    breadcrumbs.iter().any(|b| {
        let lower = b.to_ascii_lowercase();
        needles.iter().any(|n| lower.contains(n))
    })
}

fn recency_bonus(published_at: &Option<String>) -> f64 {
    let Some(s) = published_at.as_ref() else {
        return 0.0;
    };

    // Parse a few common formats.
    let now = chrono::Utc::now().date_naive();

    let parsed = chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.date_naive())
        .or_else(|| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
        .or_else(|| chrono::NaiveDate::parse_from_str(s, "%b %d, %Y").ok())
        .or_else(|| chrono::NaiveDate::parse_from_str(s, "%B %d, %Y").ok());

    let Some(date) = parsed else {
        return 0.0;
    };

    let days = (now - date).num_days();
    if days < 0 {
        // Future date (clock skew / SERP quirks).
        return 0.05;
    }

    match days {
        0..=30 => 0.25,
        31..=365 => 0.10,
        _ => 0.0,
    }
}

fn normalize_url_key(url: &str) -> String {
    let trimmed = url.trim();
    let Ok(mut parsed) = url::Url::parse(trimmed) else {
        return trimmed.to_string();
    };

    parsed.set_fragment(None);

    // Drop high-noise tracking params (common across engines / social referrers).
    if parsed.query().is_some() {
        let mut kept: Vec<(String, String)> = Vec::new();
        for (k, v) in parsed.query_pairs() {
            let k_lower = k.to_ascii_lowercase();
            if k_lower.starts_with("utm_")
                || matches!(
                    k_lower.as_str(),
                    "gclid" | "fbclid" | "yclid" | "mc_cid" | "mc_eid" | "ref" | "ref_src"
                )
            {
                continue;
            }
            kept.push((k.to_string(), v.to_string()));
        }
        kept.sort();
        parsed.set_query(None);
        {
            let mut qp = parsed.query_pairs_mut();
            for (k, v) in kept {
                qp.append_pair(&k, &v);
            }
        }
    }

    parsed.to_string()
}

fn domain_weight(query: &str, domain: &Option<String>, source_type: &Option<String>) -> f64 {
    let topic = classify_query_topic(query);

    let mut weight: f64 = match source_type.as_deref().unwrap_or("other") {
        "repo" => 1.40_f64,
        "docs" => 1.35_f64,
        "qa" => 1.25_f64,
        "package" => 1.20_f64,
        "blog" => 1.00_f64,
        "video" => 0.85_f64,
        "gaming" => 0.25_f64,
        _ => 1.00_f64,
    };

    if let Some(d) = domain.as_ref() {
        let d = d.to_ascii_lowercase();
        if d.ends_with(".gov") || d.ends_with(".edu") {
            weight *= 1.50;
        }

        // High-authority standards bodies / official references.
        if d == "ietf.org" || d.ends_with(".ietf.org") {
            weight *= 1.50;
        }
        if d == "w3.org" || d.ends_with(".w3.org") {
            weight *= 1.50;
        }
        if d.ends_with(".rust-lang.org") || d == "rust-lang.org" {
            weight *= 1.35;
        }
        if d.ends_with("learn.microsoft.com") {
            weight *= 1.25;
        }

        if d.contains("wikipedia.org") {
            weight *= 1.30;
        }

        // Topic-specific boosts.
        match topic {
            QueryTopic::Code => {
                if d.contains("github.com") || d.contains("stackoverflow.com") {
                    weight *= 1.25;
                }
                if d.contains("docs.rs") {
                    weight *= 1.30;
                }
            }
            QueryTopic::News => {
                if d.contains("reuters.com") || d.contains("apnews.com") {
                    weight *= 1.25;
                }
                if d.contains("bbc.co") || d.contains("bbc.com") {
                    weight *= 1.10;
                }
            }
            QueryTopic::General => {}
        }

        // Light penalties for common low-signal domains.
        if d.contains("pinterest.") || d.contains("facebook.") || d.contains("tiktok.") {
            weight *= 0.60;
        }
        if d.contains("medium.com") {
            weight *= 0.95;
        }

        // Ads / tracking domains: aggressively downrank.
        if d.contains("doubleclick.net")
            || d.contains("googleadservices.com")
            || d.contains("googlesyndication.com")
        {
            weight *= 0.10;
        }
    }

    weight.clamp(0.10_f64, 3.0_f64)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum QueryTopic {
    Code,
    News,
    General,
}

fn classify_query_topic(query: &str) -> QueryTopic {
    let q = query.to_ascii_lowercase();
    let code_needles = [
        "rust",
        "python",
        "javascript",
        "typescript",
        "golang",
        "error",
        "exception",
        "stack trace",
        "crate",
        "npm",
        "api",
        "sdk",
        "how to",
        "tutorial",
    ];
    if code_needles.iter().any(|n| q.contains(n)) {
        return QueryTopic::Code;
    }

    let news_needles = [
        "news", "latest", "today", "breaking", "report", "2026", "2025",
    ];
    if news_needles.iter().any(|n| q.contains(n)) {
        return QueryTopic::News;
    }

    QueryTopic::General
}

pub async fn search_web(
    state: &Arc<AppState>,
    query: &str,
) -> Result<(Vec<SearchResult>, SearchExtras)> {
    search_web_with_params(state, query, None).await
}

pub async fn search_web_with_params(
    state: &Arc<AppState>,
    query: &str,
    overrides: Option<SearchParamOverrides>,
) -> Result<(Vec<SearchResult>, SearchExtras)> {
    info!("Searching for: {}", query);

    let neurosiphon = crate::core::config::neurosiphon_enabled();

    // Phase 2: Check for recent duplicates if memory enabled
    let mut duplicate_warning = None;
    if neurosiphon {
        if let Some(memory) = &state.memory {
        match memory.find_recent_duplicate(query, 6).await {
            Ok(Some((entry, score))) => {
                let time_ago = chrono::Utc::now().signed_duration_since(entry.timestamp);
                let hours = time_ago.num_hours();
                let minutes = time_ago.num_minutes();

                let time_str = if hours > 0 {
                    format!("{} hour{} ago", hours, if hours == 1 { "" } else { "s" })
                } else {
                    format!(
                        "{} minute{} ago",
                        minutes,
                        if minutes == 1 { "" } else { "s" }
                    )
                };

                duplicate_warning = Some(format!(
                    "⚠️ Similar search found from {} (similarity: {:.2}). Consider checking history first.",
                    time_str, score
                ));
                warn!(
                    "Duplicate search detected: {} ({} ago)",
                    entry.query, time_str
                );
            }
            Ok(None) => {}
            Err(e) => warn!("Failed to check for duplicates: {}", e),
        }
        }
    }

    // Phase 2: Query rewriting for developer queries
    let rewrite_result = if neurosiphon {
        let rewriter = QueryRewriter::new();
        rewriter.rewrite_query(query)
    } else {
        QueryRewriteResult {
            original: query.to_string(),
            rewritten: None,
            suggestions: Vec::new(),
            detected_keywords: Vec::new(),
            is_developer_query: false,
        }
    };

    let effective_query = if neurosiphon && rewrite_result.was_rewritten() {
        info!(
            "Query rewritten: '{}' -> '{}'",
            query,
            rewrite_result.best_query()
        );
        rewrite_result.best_query().to_string()
    } else {
        query.to_string()
    };

    let cache_key = if let Some(ref ov) = overrides {
        format!(
            "q={}|eng={}|cat={}|lang={}|safe={}|time={}|page={}|ns={}",
            query,
            ov.engines.clone().unwrap_or_default(),
            ov.categories.clone().unwrap_or_default(),
            ov.language.clone().unwrap_or_default(),
            ov.safesearch.map(|v| v.to_string()).unwrap_or_default(),
            ov.time_range.clone().unwrap_or_default(),
            ov.pageno
                .map(|v| v.to_string())
                .unwrap_or_else(|| "1".into()),
            if neurosiphon { 1 } else { 0 }
        )
    } else {
        format!("q={}|default|ns={}", query, if neurosiphon { 1 } else { 0 })
    };

    if let Some(cached) = state.search_cache.get(&cache_key).await {
        debug!("search cache hit for query");
        let cached_extras = SearchExtras {
            query_rewrite: Some(rewrite_result),
            duplicate_warning,
            ..Default::default()
        };
        return Ok((cached, cached_extras));
    }

    let _permit = state
        .outbound_limit
        .acquire()
        .await
        .expect("semaphore closed");

    let raw_results = state
        .search_service
        .search(state, &effective_query, overrides.clone())
        .await
        .map_err(|e| anyhow!("internal search failed: {}", e))?;

    debug!("Internal search returned {} raw results", raw_results.len());

    // Convert to our format with enhanced metadata
    let mut seen = std::collections::HashSet::new();
    let mut results: Vec<SearchResult> = Vec::new();
    for result in raw_results.into_iter() {
        if seen.insert(result.url.clone()) {
            results.push(result);
        }
    }

    // Internal engines don't provide external-backend "extras"; keep only rewrite+dup warning.
    let extras = SearchExtras {
        query_rewrite: Some(rewrite_result),
        duplicate_warning,
        ..Default::default()
    };

    // Enhanced semantic reranking with keyword boosting (NeuroSiphon mode)
    let final_results = if neurosiphon {
        let reranker = Reranker::new(query);
        let boosted_results = boost_by_early_keywords(&results, query);
        let reranked_results = reranker.rerank_top(boosted_results, 50);

        info!(
            "Reranked {} results by relevance (with keyword boosting)",
            reranked_results.len()
        );
        reranked_results
    } else {
        results
    };

    state
        .search_cache
        .insert(cache_key, final_results.clone())
        .await;

    if let Some(memory) = &state.memory {
        let result_json = serde_json::to_value(&final_results).unwrap_or_default();

        if let Err(e) = memory
                .log_search(query.to_string(), &result_json, final_results.len())
            .await
        {
            warn!("Failed to log search to history: {}", e);
        }
    }

    Ok((final_results, extras))
}

/// Classify search result by domain and source type
/// Returns (domain, source_type)
pub(crate) fn classify_search_result(url_str: &str) -> (Option<String>, String) {
    let domain = url::Url::parse(url_str)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_string()));

    let source_type = if let Some(ref d) = domain {
        let d_lower = d.to_lowercase();

        if d_lower.ends_with(".github.io")
            || d_lower.contains("docs.rs")
            || d_lower.contains("readthedocs")
            || d_lower.contains("rust-lang.org")
            || d_lower.contains("doc.rust-lang")
            || d_lower.contains("developer.mozilla.org")
            || d_lower.contains("learn.microsoft.com")
            || d_lower.contains("man7.org")
            || d_lower.contains("devdocs.io")
        {
            "docs".to_string()
        } else if d_lower.contains("github.com")
            || d_lower.contains("gitlab.com")
            || d_lower.contains("bitbucket.org")
            || d_lower.contains("codeberg.org")
        {
            "repo".to_string()
        } else if d_lower.contains("news")
            || d_lower.contains("blog")
            || d_lower.contains("medium.com")
            || d_lower.contains("dev.to")
            || d_lower.contains("hackernews")
            || d_lower.contains("reddit.com")
            || d_lower.contains("thenewstack.io")
        {
            "blog".to_string()
        } else if d_lower.contains("youtube.com") || d_lower.contains("vimeo.com") {
            "video".to_string()
        } else if d_lower.contains("stackoverflow.com") || d_lower.contains("stackexchange.com") {
            "qa".to_string()
        } else if d_lower.contains("crates.io")
            || d_lower.contains("npmjs.com")
            || d_lower.contains("pypi.org")
        {
            "package".to_string()
        } else if d_lower.contains("steam")
            || d_lower.contains("facepunch")
            || d_lower.contains("game")
        {
            "gaming".to_string()
        } else {
            "other".to_string()
        }
    } else {
        "other".to_string()
    };

    (domain, source_type)
}

/// Boost results with query keywords in first 200 chars
fn boost_by_early_keywords(results: &[SearchResult], query: &str) -> Vec<SearchResult> {
    let query_tokens: Vec<String> = query
        .to_lowercase()
        .split_whitespace()
        .filter(|s| s.len() > 2)
        .map(|s| s.to_string())
        .collect();

    if query_tokens.is_empty() {
        return results.to_vec();
    }

    let mut boosted_results: Vec<(SearchResult, f64)> = results
        .iter()
        .map(|result| {
            let mut boost_score = result.score.unwrap_or(1.0);

            let content_preview: String = result.content.chars().take(200).collect();
            let content_lower = content_preview.to_lowercase();

            let mut early_matches = 0;
            for token in &query_tokens {
                if content_lower.contains(token) {
                    early_matches += 1;
                }
            }

            if early_matches > 0 {
                boost_score *= 1.0 + (early_matches as f64 * 0.2);
            }

            (result.clone(), boost_score)
        })
        .collect();

    boosted_results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    boosted_results.into_iter().map(|(r, _)| r).collect()
}
