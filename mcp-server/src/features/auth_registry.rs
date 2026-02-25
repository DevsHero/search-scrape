//! Auth Registry — per-domain authentication metadata cache.
//!
//! Persisted as `~/.cortex-scout/auth_map.json`.  Each entry records whether a
//! domain needs auth, when the last successful scrape happened, what kind of
//! auth was used, and when the stored session expires.
//!
//! # Design rationale
//!
//! LanceDB (the existing semantic memory store) uses Arrow schemas + ML
//! embeddings — ideal for fuzzy research-history queries but massively
//! over-engineered for a lookup table of O(100) domains.  A plain JSON file is
//! chosen instead:
//!
//! * Zero new dependencies
//! * Sub-millisecond reads for a typical auth map
//! * Embeds directly in the binary — no Docker, no external process
//! * Perfectly readable / editable by the operator
//!
//! ## Thread safety
//!
//! The registry is loaded fresh on every read to avoid stale in-memory state
//! across long-running server sessions.  Writes are atomic (write-to-temp then
//! rename) so concurrent processes cannot observe a partial file.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{info, warn};

// ─────────────────────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────────────────────

/// Authentication metadata for a single domain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainRecord {
    /// Set to `true` as soon as a successful `human_auth_session` HITL flow
    /// completes for this domain.
    pub needs_auth: bool,

    /// ISO-8601 timestamp of the most recent successful scrape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_success: Option<DateTime<Utc>>,

    /// Free-form label describing how auth is provided.
    /// Typical values: `"session_cookies"`, `"persistent_profile"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_type: Option<String>,

    /// Unix timestamp (seconds) of the earliest-expiring cookie in the stored
    /// session.  `None` means the session never expires (all cookies are
    /// session-scoped, i.e. `expires = -1`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_expiry: Option<f64>,

    /// How many consecutive times the stored session has successfully bypassed
    /// auth.  Useful for confidence-based decisions.
    #[serde(default)]
    pub successful_injections: u32,

    /// How many times the stored session was found to be invalid / expired
    /// after injection (i.e. the scrape still returned high `auth_risk_score`).
    #[serde(default)]
    pub failed_injections: u32,
}

impl DomainRecord {
    /// Returns `true` when the session is still within its validity window.
    ///
    /// * If `session_expiry` is `None` the session is session-scoped and is
    ///   treated as always valid (it will naturally expire when the cookie jar
    ///   is cleared).
    /// * If a concrete timestamp is stored, compare it against the current UTC
    ///   clock with a 60-second safety margin (to account for clock skew and
    ///   in-flight requests).
    pub fn is_session_valid(&self) -> bool {
        match self.session_expiry {
            None => true, // session-scoped cookies; no TTL
            Some(exp) => {
                let now = Utc::now().timestamp() as f64;
                let margin = 60.0; // seconds
                now < (exp - margin)
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Persistence helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Return the path to `~/.cortex-scout/auth_map.json`.
fn auth_map_path() -> Option<std::path::PathBuf> {
    dirs::home_dir().map(|h| h.join(".cortex-scout").join("auth_map.json"))
}

/// Load the full auth registry from disk.
///
/// Returns an empty map if the file does not exist or cannot be parsed.
pub fn load() -> HashMap<String, DomainRecord> {
    let path = match auth_map_path() {
        Some(p) => p,
        None => return HashMap::new(),
    };

    if !path.exists() {
        return HashMap::new();
    }

    let content = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            warn!("auth_registry: failed to read {}: {}", path.display(), e);
            return HashMap::new();
        }
    };

    match serde_json::from_str::<HashMap<String, DomainRecord>>(&content) {
        Ok(m) => m,
        Err(e) => {
            warn!(
                "auth_registry: failed to parse {}: {} — returning empty map",
                path.display(),
                e
            );
            HashMap::new()
        }
    }
}

/// Persist the full auth registry to disk atomically.
///
/// Writes to `{path}.tmp` first, then renames to the final path so readers
/// never observe a partially-written file.
fn save(map: &HashMap<String, DomainRecord>) {
    let path = match auth_map_path() {
        Some(p) => p,
        None => {
            warn!("auth_registry: cannot locate home directory — registry not saved");
            return;
        }
    };

    // Ensure parent directory exists.
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            warn!(
                "auth_registry: failed to create {}: {}",
                parent.display(),
                e
            );
            return;
        }
    }

    let json = match serde_json::to_string_pretty(map) {
        Ok(s) => s,
        Err(e) => {
            warn!("auth_registry: serialization failed: {}", e);
            return;
        }
    };

    // Atomic write via temp file + rename.
    let tmp = path.with_extension("tmp");
    if let Err(e) = std::fs::write(&tmp, &json) {
        warn!(
            "auth_registry: failed to write temp file {}: {}",
            tmp.display(),
            e
        );
        return;
    }
    if let Err(e) = std::fs::rename(&tmp, &path) {
        warn!(
            "auth_registry: failed to rename {} → {}: {}",
            tmp.display(),
            path.display(),
            e
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

/// Extract the bare hostname from a URL (e.g. `"github.com"` from any GitHub URL).
pub fn hostname(url: &str) -> Option<String> {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_string()))
}

/// Look up the current record for `url`'s domain.
///
/// Returns `None` when no record has been stored.
pub fn get(url: &str) -> Option<DomainRecord> {
    let domain = hostname(url)?;
    load().remove(&domain)
}

/// Return `true` when:
/// 1. There is a stored record that says `needs_auth = true`.
/// 2. The stored session has not yet expired.
///
/// Callers can use this to skip scoring and pre-emptively inject cookies.
pub fn is_session_valid(url: &str) -> bool {
    match get(url) {
        Some(r) if r.needs_auth => r.is_session_valid(),
        _ => false,
    }
}

/// Record that the domain requires authentication and register the expiry of
/// the freshly-saved session cookies.
///
/// `session_expiry` is the Unix timestamp of the earliest-expiring cookie
/// (computed by `session_store::min_cookie_expiry`).  Pass `None` when all
/// cookies are session-scoped.
pub fn mark_requires_auth(url: &str, session_expiry: Option<f64>) {
    let domain = match hostname(url) {
        Some(d) => d,
        None => return,
    };

    let mut map = load();
    let entry = map.entry(domain.clone()).or_insert_with(|| DomainRecord {
        needs_auth: false,
        last_success: None,
        auth_type: None,
        session_expiry: None,
        successful_injections: 0,
        failed_injections: 0,
    });

    entry.needs_auth = true;
    entry.auth_type = Some("session_cookies".to_string());
    entry.session_expiry = session_expiry;

    info!(
        "auth_registry: 🔐 {} marked needs_auth=true, expiry={:?}",
        domain, session_expiry
    );

    save(&map);
}

/// Mark a successful authenticated scrape for this domain and increment the
/// `successful_injections` counter.
pub fn mark_success(url: &str) {
    let domain = match hostname(url) {
        Some(d) => d,
        None => return,
    };

    let mut map = load();
    if let Some(entry) = map.get_mut(&domain) {
        entry.last_success = Some(Utc::now());
        entry.successful_injections += 1;
        info!(
            "auth_registry: ✅ {} — injection success #{} (last_success updated)",
            domain, entry.successful_injections
        );
        save(&map);
    }
}

/// Mark a failed injection (session expired mid-request or server-side logout)
/// and invalidate the stored session so the next attempt triggers a fresh HITL.
pub fn invalidate_session(url: &str) {
    let domain = match hostname(url) {
        Some(d) => d,
        None => return,
    };

    let mut map = load();
    if let Some(entry) = map.get_mut(&domain) {
        entry.session_expiry = Some(0.0); // force is_session_valid() → false
        entry.failed_injections += 1;
        warn!(
            "auth_registry: ⚠️  {} — session expired/invalid (injection #{} failed); will trigger re-auth",
            domain, entry.failed_injections
        );
        save(&map);
    }
}

/// Hard-remove all stored data for a domain (useful for operator-initiated reset).
pub fn remove(url: &str) {
    let domain = match hostname(url) {
        Some(d) => d,
        None => return,
    };
    let mut map = load();
    if map.remove(&domain).is_some() {
        info!("auth_registry: 🗑️  removed record for {}", domain);
        save(&map);
    }
}
