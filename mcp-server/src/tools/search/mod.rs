pub mod engines;
mod service;

use crate::query_rewriter::{QueryRewriteResult, QueryRewriter};
use crate::rerank::Reranker;
use crate::types::*;
use crate::AppState;
use anyhow::{anyhow, Result};
use futures::future::join_all;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

pub use service::{SearchExecutionOutcome, SearchService};

#[derive(Debug, Default, Clone)]
pub struct SearchParamOverrides {
    pub engines: Option<String>,    // comma-separated list
    pub categories: Option<String>, // comma-separated list
    pub language: Option<String>,   // e.g., "en" or "en-US"
    pub safesearch: Option<u8>,     // 0,1,2
    pub time_range: Option<String>, // e.g., day, week, month, year
    pub pageno: Option<u32>,        // 1..N
    pub disable_recovery: bool,
}

#[derive(Debug, Default, Clone)]
pub struct SearchExtras {
    pub answers: Vec<String>,
    pub suggestions: Vec<String>,
    pub corrections: Vec<String>,
    pub unresponsive_engines: Vec<String>,
    pub degraded_engines: Vec<String>,
    pub skipped_engines: Vec<String>,
    pub query_rewrite: Option<QueryRewriteResult>,
    pub duplicate_warning: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SharedSearchCacheEntry {
    cached_at_ms: i64,
    results: Vec<SearchResult>,
}

struct SharedSearchLeaderLock {
    path: PathBuf,
}

impl Drop for SharedSearchLeaderLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[derive(Debug, Clone, Default)]
struct EngineHealth {
    blocked_streak: u32,
    timeout_streak: u32,
    failure_streak: u32,
    cooldown_until: Option<Instant>,
    last_issue: Option<String>,
}

#[derive(Debug, Clone)]
enum EngineRunStatus {
    Success,
    Recovered { reason: String },
    Blocked { reason: String },
    Timeout,
    Failed { reason: String },
}

#[derive(Debug, Clone)]
struct EngineRunOutput {
    engine: String,
    results: Vec<SearchResult>,
    status: EngineRunStatus,
}

pub struct InternalSearchService {
    engine_health: Mutex<HashMap<String, EngineHealth>>,
    selection_cursor: AtomicUsize,
}

impl Default for InternalSearchService {
    fn default() -> Self {
        Self::new()
    }
}

impl InternalSearchService {
    pub fn new() -> Self {
        Self {
            engine_health: Mutex::new(HashMap::new()),
            selection_cursor: AtomicUsize::new(0),
        }
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

    fn max_engines_per_query(strict_requested: bool, requested_len: usize) -> usize {
        if strict_requested {
            return requested_len.max(1);
        }

        std::env::var("SEARCH_MAX_ENGINES_PER_QUERY")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(3)
            .clamp(1, requested_len.max(1))
    }

    fn search_engine_stagger_ms() -> u64 {
        std::env::var("SEARCH_ENGINE_STAGGER_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(125)
    }

    fn blocked_backoff_base(engine: &str) -> Duration {
        match engine {
            "brave" => Duration::from_secs(45),
            "google" => Duration::from_secs(30),
            "bing" => Duration::from_secs(35),
            _ => Duration::from_secs(25),
        }
    }

    fn timeout_backoff_base() -> Duration {
        Duration::from_secs(12)
    }

    fn failure_backoff_base() -> Duration {
        Duration::from_secs(8)
    }

    fn exp_backoff(base: Duration, streak: u32, cap: Duration) -> Duration {
        let multiplier = 2u32.saturating_pow(streak.saturating_sub(1).min(4));
        let scaled = base.saturating_mul(multiplier.max(1));
        scaled.min(cap)
    }

    fn format_cooldown(engine: &str, issue: &str, remaining: Duration) -> String {
        format!(
            "{}(cooldown:{}s after {})",
            engine,
            remaining.as_secs().max(1),
            issue
        )
    }

    fn select_engines(
        &self,
        requested: &[String],
        strict_requested: bool,
    ) -> (Vec<String>, Vec<String>) {
        if requested.is_empty() {
            return (Vec::new(), Vec::new());
        }

        let budget = Self::max_engines_per_query(strict_requested, requested.len());
        let start = if requested.len() <= 1 || strict_requested {
            0
        } else {
            self.selection_cursor.fetch_add(1, Ordering::Relaxed) % requested.len()
        };
        let ordered: Vec<String> = requested
            .iter()
            .cycle()
            .skip(start)
            .take(requested.len())
            .cloned()
            .collect();

        let now = Instant::now();
        let health = self.engine_health.lock().expect("engine health mutex poisoned");
        let mut active = Vec::new();
        let mut skipped = Vec::new();
        let mut fallback_probe: Option<(String, Duration, String)> = None;

        for engine in ordered {
            let Some(state) = health.get(&engine) else {
                active.push(engine);
                if active.len() >= budget {
                    break;
                }
                continue;
            };

            if let Some(until) = state.cooldown_until {
                if until > now {
                    let remaining = until.duration_since(now);
                    let issue = state
                        .last_issue
                        .clone()
                        .unwrap_or_else(|| "recent_failure".to_string());
                    skipped.push(Self::format_cooldown(&engine, &issue, remaining));

                    match &fallback_probe {
                        Some((_, best_remaining, _)) if *best_remaining <= remaining => {}
                        _ => fallback_probe = Some((engine.clone(), remaining, issue)),
                    }
                    continue;
                }
            }

            active.push(engine);
            if active.len() >= budget {
                break;
            }
        }

        if active.is_empty() {
            if let Some((engine, remaining, issue)) = fallback_probe {
                debug!(
                    "all engines are cooling down; probing '{}' after {:?} remaining ({})",
                    engine, remaining, issue
                );
                active.push(engine);
            }
        }

        (active, skipped)
    }

    fn select_rescue_engine(
        &self,
        requested: &[String],
        attempted: &HashSet<String>,
    ) -> Option<String> {
        let now = Instant::now();
        let health = self.engine_health.lock().expect("engine health mutex poisoned");
        let mut fallback_probe: Option<(String, Duration)> = None;

        for engine in requested {
            if attempted.contains(engine) {
                continue;
            }

            let Some(state) = health.get(engine) else {
                return Some(engine.clone());
            };

            if let Some(until) = state.cooldown_until {
                if until > now {
                    let remaining = until.duration_since(now);
                    match &fallback_probe {
                        Some((_, best_remaining)) if *best_remaining <= remaining => {}
                        _ => fallback_probe = Some((engine.clone(), remaining)),
                    }
                    continue;
                }
            }

            return Some(engine.clone());
        }

        fallback_probe.map(|(engine, _)| engine)
    }

    fn update_engine_health(&self, engine: &str, status: &EngineRunStatus) {
        let mut health = self.engine_health.lock().expect("engine health mutex poisoned");
        let entry = health.entry(engine.to_string()).or_default();
        let now = Instant::now();

        match status {
            EngineRunStatus::Success => {
                entry.blocked_streak = 0;
                entry.timeout_streak = 0;
                entry.failure_streak = 0;
                entry.cooldown_until = None;
                entry.last_issue = None;
            }
            EngineRunStatus::Recovered { reason } => {
                entry.blocked_streak = entry.blocked_streak.saturating_add(1);
                entry.timeout_streak = 0;
                entry.failure_streak = 0;
                let backoff = Self::exp_backoff(
                    Self::blocked_backoff_base(engine),
                    entry.blocked_streak,
                    Duration::from_secs(180),
                );
                entry.cooldown_until = Some(now + backoff);
                entry.last_issue = Some(format!("blocked:{}", reason));
            }
            EngineRunStatus::Blocked { reason } => {
                entry.blocked_streak = entry.blocked_streak.saturating_add(1);
                entry.timeout_streak = 0;
                entry.failure_streak = 0;
                let backoff = Self::exp_backoff(
                    Self::blocked_backoff_base(engine),
                    entry.blocked_streak,
                    Duration::from_secs(600),
                );
                entry.cooldown_until = Some(now + backoff);
                entry.last_issue = Some(format!("blocked:{}", reason));
            }
            EngineRunStatus::Timeout => {
                entry.timeout_streak = entry.timeout_streak.saturating_add(1);
                entry.failure_streak = 0;
                let backoff = Self::exp_backoff(
                    Self::timeout_backoff_base(),
                    entry.timeout_streak,
                    Duration::from_secs(120),
                );
                entry.cooldown_until = Some(now + backoff);
                entry.last_issue = Some("timeout".to_string());
            }
            EngineRunStatus::Failed { reason } => {
                entry.failure_streak = entry.failure_streak.saturating_add(1);
                let backoff = Self::exp_backoff(
                    Self::failure_backoff_base(),
                    entry.failure_streak,
                    Duration::from_secs(60),
                );
                entry.cooldown_until = Some(now + backoff);
                entry.last_issue = Some(format!("failed:{}", reason));
            }
        }
    }

    fn should_run_community_expansion_for(
        query: &str,
        primary_results: usize,
        enabled: bool,
        threshold: usize,
    ) -> bool {
        if !enabled {
            return false;
        }

        let lower = query.to_ascii_lowercase();
        let explicit_needles = [
            "reddit",
            "hacker news",
            "news.ycombinator",
            "community",
            "discussion",
            "forum",
            "forums",
            "issue",
            "issues",
        ];
        if explicit_needles.iter().any(|needle| lower.contains(needle)) {
            return true;
        }

        primary_results < threshold
    }

    fn should_run_community_expansion(query: &str, primary_results: usize) -> bool {
        let enabled = std::env::var("SEARCH_COMMUNITY_SOURCES")
            .unwrap_or_else(|_| "true".to_string())
            .eq_ignore_ascii_case("true");
        let threshold = std::env::var("SEARCH_COMMUNITY_TRIGGER_RESULTS")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(4);
        Self::should_run_community_expansion_for(query, primary_results, enabled, threshold)
    }

    fn extras_from_runs(runs: &[EngineRunOutput], skipped: Vec<String>) -> SearchExtras {
        let mut extras = SearchExtras {
            skipped_engines: skipped,
            ..Default::default()
        };

        for run in runs {
            match &run.status {
                EngineRunStatus::Success => {}
                EngineRunStatus::Recovered { reason } => {
                    extras
                        .degraded_engines
                        .push(format!("{}(recovered_via_fallback:{})", run.engine, reason));
                }
                EngineRunStatus::Blocked { reason } => {
                    extras.unresponsive_engines.push(run.engine.clone());
                    extras
                        .degraded_engines
                        .push(format!("{}(blocked:{})", run.engine, reason));
                }
                EngineRunStatus::Timeout => {
                    extras.unresponsive_engines.push(run.engine.clone());
                    extras
                        .degraded_engines
                        .push(format!("{}(timeout)", run.engine));
                }
                EngineRunStatus::Failed { reason } => {
                    extras.unresponsive_engines.push(run.engine.clone());
                    extras
                        .degraded_engines
                        .push(format!("{}(failed:{})", run.engine, reason));
                }
            }
        }

        extras
    }

    async fn sync_host_guard(&self, run: &EngineRunOutput) {
        match &run.status {
            EngineRunStatus::Success => crate::host_guard::note_search_engine_success(&run.engine).await,
            EngineRunStatus::Recovered { reason } | EngineRunStatus::Blocked { reason } => {
                crate::host_guard::note_search_engine_blocked(&run.engine, reason).await
            }
            EngineRunStatus::Timeout => crate::host_guard::note_search_engine_timeout(&run.engine).await,
            EngineRunStatus::Failed { reason } => {
                crate::host_guard::note_search_engine_failure(&run.engine, reason).await
            }
        }
    }

    async fn run_engine(
        &self,
        state: &Arc<AppState>,
        engine: &str,
        query: &str,
        max_results: usize,
    ) -> EngineRunOutput {
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
                return EngineRunOutput {
                    engine: engine.to_string(),
                    results: Vec::new(),
                    status: EngineRunStatus::Timeout,
                };
            }
        };

        match res {
            Ok(v) => EngineRunOutput {
                engine: engine.to_string(),
                results: v,
                status: EngineRunStatus::Success,
            },
            Err(engines::EngineError::Blocked { reason }) => {
                warn!("engine '{}' blocked: {}", engine, reason);
                let fallback = self
                    .tier2_non_robot_fallback(state, engine, query, max_results)
                    .await;
                match fallback {
                    Some(results) if !results.is_empty() => EngineRunOutput {
                        engine: engine.to_string(),
                        results,
                        status: EngineRunStatus::Recovered { reason },
                    },
                    _ => EngineRunOutput {
                        engine: engine.to_string(),
                        results: Vec::new(),
                        status: EngineRunStatus::Blocked { reason },
                    },
                }
            }
            Err(e) => {
                warn!("engine '{}' failed: {}", engine, e);
                EngineRunOutput {
                    engine: engine.to_string(),
                    results: Vec::new(),
                    status: EngineRunStatus::Failed {
                        reason: e.to_string(),
                    },
                }
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
            keep_open: false,
            instruction_message: None,
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
    ) -> Result<SearchExecutionOutcome> {
        let mut engines_override = overrides.as_ref().and_then(|o| o.engines.clone());
        let explicit_engines = engines_override.is_some();

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
        let (selected_engines, mut skipped_engines) =
            self.select_engines(&engine_list, explicit_engines);
        let max_results = std::env::var("SEARCH_MAX_RESULTS_PER_ENGINE")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(10);

        // Run the healthiest engines in parallel with a light stagger to reduce burstiness.
        let stagger_ms = Self::search_engine_stagger_ms();
        let engine_futs = selected_engines.iter().enumerate().map(|(index, engine)| {
            let effective_query = effective_query.clone();
            async move {
                if index > 0 && stagger_ms > 0 {
                    tokio::time::sleep(Duration::from_millis(stagger_ms * index as u64)).await;
                }
                self.run_engine(state, engine.as_str(), &effective_query, max_results)
                    .await
            }
        });
        let mut engine_runs: Vec<EngineRunOutput> = join_all(engine_futs).await;
        for run in &engine_runs {
            self.update_engine_health(&run.engine, &run.status);
            self.sync_host_guard(run).await;
        }
        let mut results: Vec<SearchResult> = engine_runs
            .iter()
            .flat_map(|run| run.results.clone())
            .collect();

        if results.is_empty() {
            let attempted: HashSet<String> = selected_engines.iter().cloned().collect();
            if let Some(rescue_engine) = self.select_rescue_engine(&engine_list, &attempted) {
                debug!(
                    "primary search returned 0 results; probing rescue engine '{}'",
                    rescue_engine
                );
                let rescue_run = self
                    .run_engine(state, rescue_engine.as_str(), &effective_query, max_results)
                    .await;
                self.update_engine_health(&rescue_run.engine, &rescue_run.status);
                self.sync_host_guard(&rescue_run).await;
                results.extend(rescue_run.results.clone());
                engine_runs.push(rescue_run);
            }
        }

        // Community expansion is expensive and higher-risk. Only use it when the query
        // explicitly asks for community discussion or primary results are too sparse.
        if Self::should_run_community_expansion(&effective_query, results.len()) {
            let community_query = format!(
                "{} (site:reddit.com OR site:news.ycombinator.com)",
                effective_query
            );

            let (community_engines, community_skipped) =
                self.select_engines(&engine_list, explicit_engines);
            skipped_engines.extend(community_skipped);
            let community_futs = community_engines.iter().enumerate().map(|(index, engine)| {
                let community_query = community_query.clone();
                async move {
                    if index > 0 && stagger_ms > 0 {
                        tokio::time::sleep(Duration::from_millis(stagger_ms * index as u64)).await;
                    }
                    self.run_engine(state, engine.as_str(), &community_query, max_results)
                        .await
                }
            });
            let community_runs: Vec<EngineRunOutput> = join_all(community_futs).await;
            for run in &community_runs {
                self.update_engine_health(&run.engine, &run.status);
                self.sync_host_guard(run).await;
            }
            results.extend(community_runs.iter().flat_map(|run| run.results.clone()));
            engine_runs.extend(community_runs);
        }

        Ok(SearchExecutionOutcome {
            results: dedup_and_score_results(results, query),
            extras: Self::extras_from_runs(&engine_runs, skipped_engines),
        })
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
        if let Some(memory) = state.get_memory_or_wait(Duration::from_secs(3)).await {
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
                    info!(
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
            "q={}|eng={}|cat={}|lang={}|safe={}|time={}|page={}|recover={}|ns={}",
            query,
            ov.engines.clone().unwrap_or_default(),
            ov.categories.clone().unwrap_or_default(),
            ov.language.clone().unwrap_or_default(),
            ov.safesearch.map(|v| v.to_string()).unwrap_or_default(),
            ov.time_range.clone().unwrap_or_default(),
            ov.pageno
                .map(|v| v.to_string())
                .unwrap_or_else(|| "1".into()),
            if ov.disable_recovery { 0 } else { 1 },
            if neurosiphon { 1 } else { 0 }
        )
    } else {
        format!("q={}|default|ns={}", query, if neurosiphon { 1 } else { 0 })
    };

    let disable_recovery = overrides
        .as_ref()
        .map(|ov| ov.disable_recovery)
        .unwrap_or(false);

    if let Some(cached) = state.search_cache.get(&cache_key).await {
        debug!("search cache hit for query");
        let cached_extras = SearchExtras {
            suggestions: rewrite_result.suggestions.clone(),
            query_rewrite: Some(rewrite_result),
            duplicate_warning,
            ..Default::default()
        };
        return Ok((cached, cached_extras));
    }

    if let Some(shared) = read_shared_search_cache(&cache_key).await {
        debug!("shared search cache hit for query");
        state.search_cache.insert(cache_key.clone(), shared.clone()).await;
        let cached_extras = SearchExtras {
            suggestions: rewrite_result.suggestions.clone(),
            query_rewrite: Some(rewrite_result),
            duplicate_warning,
            ..Default::default()
        };
        return Ok((shared, cached_extras));
    }

    let _shared_search_lock = if shared_search_cache_enabled() {
        match try_acquire_shared_search_leader(&cache_key) {
            Some(lock) => Some(lock),
            None => {
                if let Some(shared) = wait_for_shared_search_result(&cache_key).await {
                    debug!("shared search cache filled by another process");
                    state.search_cache.insert(cache_key.clone(), shared.clone()).await;
                    let cached_extras = SearchExtras {
                        suggestions: rewrite_result.suggestions.clone(),
                        query_rewrite: Some(rewrite_result),
                        duplicate_warning,
                        ..Default::default()
                    };
                    return Ok((shared, cached_extras));
                }
                try_acquire_shared_search_leader(&cache_key)
            }
        }
    } else {
        None
    };

    let _permit = state
        .outbound_limit
        .acquire()
        .await
        .expect("semaphore closed");

    let mut search_outcome = state
        .search_service
        .search(state, &effective_query, overrides.clone())
        .await
        .map_err(|e| anyhow!("internal search failed: {}", e))?;

    if !disable_recovery && search_outcome.results.is_empty() {
        let recovery_rewrite = if rewrite_result.is_developer_query {
            rewrite_result.clone()
        } else {
            QueryRewriter::new().rewrite_query(query)
        };
        let recovery_queries = zero_result_recovery_queries(query, &effective_query, &recovery_rewrite);
        for fallback_query in recovery_queries {
            info!(
                "search recovery: retrying '{}' with fallback query '{}'",
                query, fallback_query
            );
            let fallback_outcome = match state
                .search_service
                .search(state, &fallback_query, overrides.clone())
                .await
            {
                Ok(outcome) => outcome,
                Err(e) => {
                    warn!(
                        "search recovery failed for fallback query '{}': {}",
                        fallback_query, e
                    );
                    continue;
                }
            };

            merge_search_extras(&mut search_outcome.extras, fallback_outcome.extras);
            if !fallback_outcome.results.is_empty() {
                search_outcome.results = fallback_outcome.results;
                break;
            }
        }
    }

    let raw_results = search_outcome.results;

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
    let mut extras = SearchExtras {
        suggestions: rewrite_result.suggestions.clone(),
        query_rewrite: Some(rewrite_result),
        duplicate_warning,
        ..search_outcome.extras
    };
    extras.unresponsive_engines.sort();
    extras.unresponsive_engines.dedup();
    extras.degraded_engines.sort();
    extras.degraded_engines.dedup();
    extras.skipped_engines.sort();
    extras.skipped_engines.dedup();

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

    let cacheable = !(final_results.is_empty()
        && (!extras.degraded_engines.is_empty() || !extras.skipped_engines.is_empty()));
    if cacheable {
        state
            .search_cache
            .insert(cache_key.clone(), final_results.clone())
            .await;
        write_shared_search_cache(&cache_key, &final_results).await;
    } else {
        debug!("skipping cache for empty degraded search result set");
    }

    if let Some(memory) = state.get_memory() {
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

fn zero_result_recovery_queries(
    original_query: &str,
    effective_query: &str,
    rewrite_result: &QueryRewriteResult,
) -> Vec<String> {
    let mut candidates = Vec::new();

    if let Some(rewritten) = rewrite_result.rewritten.as_ref() {
        if rewritten != original_query && rewritten != effective_query {
            candidates.push(rewritten.clone());
        }
    }

    for suggestion in rewrite_result.suggestions.iter().take(3) {
        if suggestion != original_query
            && suggestion != effective_query
            && !candidates.iter().any(|existing| existing == suggestion)
        {
            candidates.push(suggestion.clone());
        }
    }

    candidates
}

fn merge_search_extras(target: &mut SearchExtras, addition: SearchExtras) {
    target.answers.extend(addition.answers);
    target.suggestions.extend(addition.suggestions);
    target.corrections.extend(addition.corrections);
    target
        .unresponsive_engines
        .extend(addition.unresponsive_engines);
    target.degraded_engines.extend(addition.degraded_engines);
    target.skipped_engines.extend(addition.skipped_engines);

    if target.query_rewrite.is_none() {
        target.query_rewrite = addition.query_rewrite;
    }
    if target.duplicate_warning.is_none() {
        target.duplicate_warning = addition.duplicate_warning;
    }
}

fn shared_search_cache_enabled() -> bool {
    match std::env::var("SEARCH_SHARED_CACHE") {
        Ok(v) => {
            let lower = v.trim().to_ascii_lowercase();
            !(lower.is_empty() || lower == "0" || lower == "false" || lower == "no" || lower == "off")
        }
        Err(_) => true,
    }
}

fn shared_search_cache_ttl_secs() -> u64 {
    std::env::var("SEARCH_SHARED_CACHE_TTL_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(300)
}

fn shared_search_cache_path(cache_key: &str) -> PathBuf {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    cache_key.hash(&mut hasher);
    let filename = format!("{:016x}.json", hasher.finish());

    std::env::var("SEARCH_SHARED_CACHE_DIR")
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs::cache_dir()
                .unwrap_or_else(std::env::temp_dir)
                .join("cortex-scout")
                .join("shared-search-cache")
        })
        .join(filename)
}

fn shared_search_lock_path(cache_key: &str) -> PathBuf {
    shared_search_cache_path(cache_key).with_extension("lock")
}

fn shared_search_lock_wait_secs() -> u64 {
    std::env::var("SEARCH_SHARED_CACHE_WAIT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
    .unwrap_or(180)
}

fn shared_search_lock_stale_secs() -> u64 {
    std::env::var("SEARCH_SHARED_CACHE_LOCK_STALE_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(120)
}

fn try_acquire_shared_search_leader(cache_key: &str) -> Option<SharedSearchLeaderLock> {
    if !shared_search_cache_enabled() {
        return None;
    }

    let path = shared_search_lock_path(cache_key);
    if let Some(parent) = path.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return None;
        }
    }

    for attempt in 0..2 {
        match std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
        {
            Ok(_) => return Some(SharedSearchLeaderLock { path }),
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists && attempt == 0 => {
                let stale = std::fs::metadata(&path)
                    .ok()
                    .and_then(|meta| meta.modified().ok())
                    .and_then(|modified| modified.elapsed().ok())
                    .map(|elapsed| elapsed.as_secs() > shared_search_lock_stale_secs())
                    .unwrap_or(false);
                if stale {
                    let _ = std::fs::remove_file(&path);
                    continue;
                }
                return None;
            }
            Err(_) => return None,
        }
    }

    None
}

async fn read_shared_search_cache(cache_key: &str) -> Option<Vec<SearchResult>> {
    if !shared_search_cache_enabled() {
        return None;
    }

    let path = shared_search_cache_path(cache_key);
    let bytes = tokio::fs::read(&path).await.ok()?;
    let entry: SharedSearchCacheEntry = serde_json::from_slice(&bytes).ok()?;
    let age_ms = chrono::Utc::now()
        .timestamp_millis()
        .saturating_sub(entry.cached_at_ms);
    if age_ms > (shared_search_cache_ttl_secs() as i64 * 1000) {
        let _ = tokio::fs::remove_file(path).await;
        return None;
    }

    Some(entry.results)
}

async fn wait_for_shared_search_result(cache_key: &str) -> Option<Vec<SearchResult>> {
    let deadline = Instant::now() + Duration::from_secs(shared_search_lock_wait_secs());
    let lock_path = shared_search_lock_path(cache_key);
    while Instant::now() < deadline {
        if let Some(shared) = read_shared_search_cache(cache_key).await {
            return Some(shared);
        }

        let stale = std::fs::metadata(&lock_path)
            .ok()
            .and_then(|meta| meta.modified().ok())
            .and_then(|modified| modified.elapsed().ok())
            .map(|elapsed| elapsed.as_secs() > shared_search_lock_stale_secs())
            .unwrap_or(false);
        if stale {
            let _ = std::fs::remove_file(&lock_path);
            break;
        }

        if !lock_path.exists() {
            break;
        }

        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    None
}

async fn write_shared_search_cache(cache_key: &str, results: &[SearchResult]) {
    if !shared_search_cache_enabled() || results.is_empty() {
        return;
    }

    let path = shared_search_cache_path(cache_key);
    if let Some(parent) = path.parent() {
        if tokio::fs::create_dir_all(parent).await.is_err() {
            return;
        }
    }

    let entry = SharedSearchCacheEntry {
        cached_at_ms: chrono::Utc::now().timestamp_millis(),
        results: results.to_vec(),
    };
    let Ok(bytes) = serde_json::to_vec(&entry) else {
        return;
    };

    let temp_path = path.with_extension("tmp");
    if tokio::fs::write(&temp_path, bytes).await.is_ok() {
        let _ = tokio::fs::rename(&temp_path, &path).await;
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn community_expansion_only_runs_when_needed() {
        assert!(InternalSearchService::should_run_community_expansion_for(
            "rust model context protocol reddit discussion",
            20,
            true,
            4,
        ));
        assert!(InternalSearchService::should_run_community_expansion_for(
            "rust model context protocol",
            2,
            true,
            4,
        ));
        assert!(!InternalSearchService::should_run_community_expansion_for(
            "rust model context protocol",
            8,
            true,
            4,
        ));
    }

    #[test]
    fn select_engines_skips_cooling_down_engines() {
        let service = InternalSearchService::new();
        {
            let mut health = service
                .engine_health
                .lock()
                .expect("engine health mutex poisoned");
            health.insert(
                "google".to_string(),
                EngineHealth {
                    cooldown_until: Some(Instant::now() + Duration::from_secs(30)),
                    last_issue: Some("blocked:http_429".to_string()),
                    ..Default::default()
                },
            );
        }

        let requested = vec![
            "google".to_string(),
            "bing".to_string(),
            "duckduckgo".to_string(),
            "brave".to_string(),
        ];
        let (selected, skipped) = service.select_engines(&requested, false);

        assert!(!selected.iter().any(|engine| engine == "google"));
        assert!(selected.iter().any(|engine| engine == "bing"));
        assert!(skipped.iter().any(|entry| entry.contains("google(cooldown")));
    }

    #[test]
    fn blocked_status_generates_degraded_telemetry() {
        let extras = InternalSearchService::extras_from_runs(
            &[
                EngineRunOutput {
                    engine: "google".to_string(),
                    results: Vec::new(),
                    status: EngineRunStatus::Blocked {
                        reason: "http_429".to_string(),
                    },
                },
                EngineRunOutput {
                    engine: "bing".to_string(),
                    results: Vec::new(),
                    status: EngineRunStatus::Recovered {
                        reason: "cloudflare".to_string(),
                    },
                },
            ],
            vec!["duckduckgo(cooldown:30s after timeout)".to_string()],
        );

        assert_eq!(extras.unresponsive_engines, vec!["google".to_string()]);
        assert!(extras
            .degraded_engines
            .iter()
            .any(|entry| entry == "google(blocked:http_429)"));
        assert!(extras
            .degraded_engines
            .iter()
            .any(|entry| entry == "bing(recovered_via_fallback:cloudflare)"));
        assert_eq!(extras.skipped_engines.len(), 1);
    }
}
