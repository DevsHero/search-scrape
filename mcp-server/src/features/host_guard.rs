use anyhow::{Context, Result};
use fs2::FileExt;
use rand::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::warn;

#[derive(Debug, Clone, Copy)]
pub struct GatePolicy {
    pub min_gap: Duration,
    pub max_gap: Duration,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct CoordinationState {
    buckets: HashMap<String, BucketState>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct BucketState {
    next_allowed_at_ms: u64,
    failure_streak: u32,
    last_reason: Option<String>,
    updated_at_ms: u64,
}

const STATE_TTL_MS: u64 = 24 * 60 * 60 * 1000;

fn guard_disabled() -> bool {
    match std::env::var("CORTEX_SCOUT_HOST_GUARD_DISABLED") {
        Ok(v) => {
            let lower = v.trim().to_ascii_lowercase();
            lower == "1" || lower == "true" || lower == "yes" || lower == "on"
        }
        Err(_) => false,
    }
}

fn guard_state_path() -> PathBuf {
    if let Ok(path) = std::env::var("CORTEX_SCOUT_HOST_GUARD_PATH") {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }

    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("cortex-scout")
        .join("host-guard.json")
}

fn guard_lock_path() -> PathBuf {
    let state_path = guard_state_path();
    let lock_name = state_path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| format!("{}.lock", name))
        .unwrap_or_else(|| "host-guard.lock".to_string());
    state_path.with_file_name(lock_name)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn prune_state(state: &mut CoordinationState, now: u64) {
    state.buckets.retain(|_, bucket| {
        now.saturating_sub(bucket.updated_at_ms.max(bucket.next_allowed_at_ms)) <= STATE_TTL_MS
    });
}

fn random_gap(policy: GatePolicy) -> u64 {
    let min_ms = policy.min_gap.as_millis() as u64;
    let max_ms = policy.max_gap.as_millis() as u64;
    let (min_ms, max_ms) = if min_ms > max_ms {
        (max_ms, min_ms)
    } else {
        (min_ms, max_ms)
    };
    if min_ms == max_ms {
        return min_ms;
    }

    let mut rng = rand::rng();
    rng.random_range(min_ms..=max_ms)
}

fn with_state_mut<T>(f: impl FnOnce(&mut CoordinationState) -> T) -> Result<T> {
    let state_path = guard_state_path();
    let lock_path = guard_lock_path();

    if let Some(parent) = state_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create host guard dir: {}", parent.display()))?;
    }

    let lock_file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
        .with_context(|| format!("failed to open host guard lock: {}", lock_path.display()))?;

    lock_file
        .lock_exclusive()
        .with_context(|| format!("failed to lock host guard: {}", lock_path.display()))?;

    let mut state = std::fs::read(&state_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<CoordinationState>(&bytes).ok())
        .unwrap_or_default();

    let now = now_ms();
    prune_state(&mut state, now);

    let output = f(&mut state);

    let bytes = serde_json::to_vec(&state).context("failed to serialize host guard state")?;
    let temp_path = state_path.with_extension("tmp");
    std::fs::write(&temp_path, bytes)
        .with_context(|| format!("failed to write host guard temp state: {}", temp_path.display()))?;
    std::fs::rename(&temp_path, &state_path).with_context(|| {
        format!(
            "failed to atomically replace host guard state: {}",
            state_path.display()
        )
    })?;

    lock_file.unlock().ok();
    Ok(output)
}

fn reserve_slot_sync(bucket_key: &str, policy: GatePolicy) -> Result<Duration> {
    with_state_mut(|state| {
        let now = now_ms();
        let bucket = state.buckets.entry(bucket_key.to_string()).or_default();
        let scheduled_start = now.max(bucket.next_allowed_at_ms);
        let wait_ms = scheduled_start.saturating_sub(now);
        bucket.next_allowed_at_ms = scheduled_start.saturating_add(random_gap(policy));
        bucket.updated_at_ms = now;
        Duration::from_millis(wait_ms)
    })
}

fn reward_slot_sync(bucket_key: &str) -> Result<()> {
    with_state_mut(|state| {
        if let Some(bucket) = state.buckets.get_mut(bucket_key) {
            bucket.failure_streak = bucket.failure_streak.saturating_sub(1);
            bucket.updated_at_ms = now_ms();
            if bucket.failure_streak == 0 && bucket.next_allowed_at_ms < bucket.updated_at_ms {
                bucket.last_reason = None;
            }
        }
    })
}

fn penalty_duration(base: Duration, streak: u32, cap: Duration) -> Duration {
    let multiplier = 2u32.saturating_pow(streak.saturating_sub(1).min(4));
    base.saturating_mul(multiplier.max(1)).min(cap)
}

fn penalize_slot_sync(bucket_key: &str, reason: &str, base: Duration, cap: Duration) -> Result<()> {
    with_state_mut(|state| {
        let now = now_ms();
        let bucket = state.buckets.entry(bucket_key.to_string()).or_default();
        bucket.failure_streak = bucket.failure_streak.saturating_add(1);
        let penalty = penalty_duration(base, bucket.failure_streak, cap).as_millis() as u64;
        let anchor = bucket.next_allowed_at_ms.max(now);
        bucket.next_allowed_at_ms = anchor.saturating_add(penalty);
        bucket.last_reason = Some(reason.to_string());
        bucket.updated_at_ms = now;
    })
}

fn search_policy(engine: &str) -> GatePolicy {
    let default = match engine {
        "duckduckgo" | "ddg" => (2500, 4500),
        "brave" => (2200, 4000),
        "google" => (1800, 3200),
        "bing" => (1800, 3200),
        _ => (1800, 3200),
    };

    let min_ms = std::env::var("SEARCH_HOST_MIN_GAP_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(default.0);
    let max_ms = std::env::var("SEARCH_HOST_MAX_GAP_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(default.1.max(min_ms));

    GatePolicy {
        min_gap: Duration::from_millis(min_ms),
        max_gap: Duration::from_millis(max_ms.max(min_ms)),
    }
}

fn scrape_policy() -> GatePolicy {
    let min_ms = std::env::var("SCRAPE_HOST_MIN_GAP_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(900);
    let max_ms = std::env::var("SCRAPE_HOST_MAX_GAP_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(1800);

    GatePolicy {
        min_gap: Duration::from_millis(min_ms),
        max_gap: Duration::from_millis(max_ms.max(min_ms)),
    }
}

fn search_bucket_key(engine: &str) -> String {
    format!("search-engine:{}", engine)
}

fn url_bucket_key(url: &str) -> Option<String> {
    let host = url::Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(|host| host.to_ascii_lowercase()))?;
    Some(format!("scrape-host:{}", host))
}

async fn reserve_slot(bucket_key: String, policy: GatePolicy) {
    if guard_disabled() {
        return;
    }

    let wait = match tokio::task::spawn_blocking(move || reserve_slot_sync(&bucket_key, policy)).await
    {
        Ok(Ok(wait)) => wait,
        Ok(Err(e)) => {
            warn!("host guard reserve failed: {}", e);
            return;
        }
        Err(e) => {
            warn!("host guard task join failed: {}", e);
            return;
        }
    };

    if !wait.is_zero() {
        tokio::time::sleep(wait).await;
    }
}

async fn penalize_slot(bucket_key: String, reason: String, base: Duration, cap: Duration) {
    if guard_disabled() {
        return;
    }

    match tokio::task::spawn_blocking(move || penalize_slot_sync(&bucket_key, &reason, base, cap)).await
    {
        Ok(Ok(())) => {}
        Ok(Err(e)) => warn!("host guard penalty update failed: {}", e),
        Err(e) => warn!("host guard penalty task join failed: {}", e),
    }
}

async fn reward_slot(bucket_key: String) {
    if guard_disabled() {
        return;
    }

    match tokio::task::spawn_blocking(move || reward_slot_sync(&bucket_key)).await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => warn!("host guard reward update failed: {}", e),
        Err(e) => warn!("host guard reward task join failed: {}", e),
    }
}

pub async fn wait_for_search_engine(engine: &str) {
    reserve_slot(search_bucket_key(engine), search_policy(engine)).await;
}

pub async fn note_search_engine_success(engine: &str) {
    reward_slot(search_bucket_key(engine)).await;
}

pub async fn note_search_engine_blocked(engine: &str, reason: &str) {
    let base = match reason {
        r if r.contains("http_429") => Duration::from_secs(45),
        r if r.contains("captcha") || r.contains("cloudflare") => Duration::from_secs(90),
        _ => Duration::from_secs(30),
    };
    penalize_slot(
        search_bucket_key(engine),
        format!("blocked:{}", reason),
        base,
        Duration::from_secs(600),
    )
    .await;
}

pub async fn note_search_engine_timeout(engine: &str) {
    penalize_slot(
        search_bucket_key(engine),
        "timeout".to_string(),
        Duration::from_secs(20),
        Duration::from_secs(180),
    )
    .await;
}

pub async fn note_search_engine_failure(engine: &str, reason: &str) {
    penalize_slot(
        search_bucket_key(engine),
        format!("failed:{}", reason),
        Duration::from_secs(10),
        Duration::from_secs(60),
    )
    .await;
}

pub async fn wait_for_url_host(url: &str) {
    if let Some(bucket_key) = url_bucket_key(url) {
        reserve_slot(bucket_key, scrape_policy()).await;
    }
}

pub async fn note_url_host_blocked(url: &str, reason: &str) {
    if let Some(bucket_key) = url_bucket_key(url) {
        penalize_slot(
            bucket_key,
            format!("blocked:{}", reason),
            Duration::from_secs(15),
            Duration::from_secs(180),
        )
        .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn with_test_path<T>(name: &str, f: impl FnOnce() -> T) -> T {
        let _guard = test_lock().lock().expect("test mutex poisoned");
        let path = std::env::temp_dir().join(format!(
            "cortex-scout-host-guard-{}-{}.json",
            name,
            now_ms()
        ));
        std::env::set_var("CORTEX_SCOUT_HOST_GUARD_PATH", &path);
        std::env::remove_var("CORTEX_SCOUT_HOST_GUARD_DISABLED");
        let result = f();
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_file_name(format!(
            "{}.lock",
            path.file_name().and_then(|name| name.to_str()).unwrap_or("guard")
        )));
        std::env::remove_var("CORTEX_SCOUT_HOST_GUARD_PATH");
        result
    }

    #[test]
    fn reserve_slot_serializes_requests() {
        with_test_path("reserve", || {
            let policy = GatePolicy {
                min_gap: Duration::from_millis(50),
                max_gap: Duration::from_millis(50),
            };
            let first = reserve_slot_sync("search-engine:google", policy).expect("reserve succeeds");
            let second = reserve_slot_sync("search-engine:google", policy).expect("reserve succeeds");

            assert!(first <= Duration::from_millis(5));
            assert!(second >= Duration::from_millis(45));
        });
    }

    #[test]
    fn penalty_extends_cooldown() {
        with_test_path("penalty", || {
            let policy = GatePolicy {
                min_gap: Duration::from_millis(10),
                max_gap: Duration::from_millis(10),
            };
            reserve_slot_sync("search-engine:brave", policy).expect("initial reserve succeeds");
            penalize_slot_sync(
                "search-engine:brave",
                "blocked:http_429",
                Duration::from_millis(100),
                Duration::from_millis(100),
            )
            .expect("penalty succeeds");

            let wait = reserve_slot_sync("search-engine:brave", policy).expect("reserve succeeds");
            assert!(wait >= Duration::from_millis(95));
        });
    }
}