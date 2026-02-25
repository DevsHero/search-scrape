//! Session cookie persistence — shared load/inject helpers.
//!
//! After a successful `human_auth_session` / `non_robot_search` HITL flow the
//! browser cookies are saved to `~/.cortex-scout/sessions/{domain_key}.json`.
//! This module provides companion helpers to *load* those cookies and *inject*
//! them into any CDP page so future scrapes of the same domain are
//! automatically authenticated — without any user interaction.
//!
//! Session metadata (expiry, needs_auth flag, last_success) is tracked by the
//! companion [`super::auth_registry`] module which maintains
//! `~/.cortex-scout/auth_map.json`.

use tracing::{info, warn};

// ─────────────────────────────────────────────────────────────────────────────
// Domain key utilities
// ─────────────────────────────────────────────────────────────────────────────

/// Filesystem-safe key derived from a bare hostname string.
fn host_to_key(host: &str) -> String {
    host.replace('.', "_").replace(':', "_")
}

/// Derive the filesystem-safe key used as the session filename from a URL.
///
/// e.g. `https://gist.github.com/foo` → `"gist_github_com"`
pub fn domain_key(url: &str) -> Option<String> {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(host_to_key))
}

/// Derive the key for the *parent* domain of a hostname by stripping the
/// leftmost subdomain segment.
///
/// Returns `None` when the host is already a bare second-level domain.
///
/// * `"gist.github.com"` → `Some("github_com")`
/// * `"github.com"`      → `None`
pub fn parent_domain_key(host: &str) -> Option<String> {
    let dot_pos = host.find('.')?;
    let rest = &host[dot_pos + 1..];
    // Require at least one more dot so we don't return a bare TLD.
    if rest.contains('.') {
        Some(host_to_key(rest))
    } else {
        None
    }
}

/// Return the full path to the session file for a pre-computed key.
fn session_path_by_key(key: &str) -> Option<std::path::PathBuf> {
    let home = dirs::home_dir()?;
    Some(
        home.join(".cortex-scout")
            .join("sessions")
            .join(format!("{}.json", key)),
    )
}

/// Return the full path to the session file for a URL.
pub fn session_path(url: &str) -> Option<std::path::PathBuf> {
    session_path_by_key(&domain_key(url)?)
}

// ─────────────────────────────────────────────────────────────────────────────
// Expiry helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Compute the minimum finite cookie expiry timestamp from a raw cookie array.
///
/// CDP cookies carry an `expires` field that is either:
/// * `-1.0`  — session cookie (no persistent expiry)
/// * A positive Unix timestamp (seconds, as `f64`)
///
/// Returns `None` when every cookie is session-scoped so the caller can treat
/// the session as having no hard TTL.
/// See [`effective_session_expiry`] for a version that defaults to +24 h.
pub fn min_cookie_expiry(raw_cookies: &[serde_json::Value]) -> Option<f64> {
    raw_cookies
        .iter()
        .filter_map(|v| v.get("expires").and_then(|e| e.as_f64()))
        .filter(|&exp| exp > 0.0) // -1 = session cookie, skip
        .reduce(f64::min)
}

/// Like [`min_cookie_expiry`] but applies a **+24-hour default** when every
/// cookie in the jar is session-scoped (`expires == -1`).
///
/// Modern apps (GitHub, Notion, Linear…) often use session-only cookies.
/// Without this fallback those sessions would never be registered in the auth
/// registry and the pre-injection fast-path would be skipped on every visit.
/// A 24-hour window is conservative: stale session cookies simply fall through
/// to the graceful HITL re-auth path.
///
/// Returns `None` only when the cookie array is empty (nothing to track).
pub fn effective_session_expiry(raw_cookies: &[serde_json::Value]) -> Option<f64> {
    if raw_cookies.is_empty() {
        return None;
    }
    if let Some(min_exp) = min_cookie_expiry(raw_cookies) {
        return Some(min_exp); // at least one persistent cookie
    }
    // All session-scoped → default to now + 24 h.
    let default_exp = chrono::Utc::now().timestamp() as f64 + 86_400.0;
    info!(
        "session_store: all cookies are session-scoped — defaulting expiry to +24 h \
         (unix {:.0})",
        default_exp
    );
    Some(default_exp)
}

/// Remove the stored session file for a domain so the next scrape triggers a
/// fresh HITL login flow.  Also calls [`super::auth_registry::invalidate_session`]
/// so the auth registry reflects the stale state immediately.
pub fn invalidate(url: &str) {
    let domain = domain_key(url).unwrap_or_default();
    if let Some(path) = session_path(url) {
        if path.exists() {
            match std::fs::remove_file(&path) {
                Ok(()) => info!(
                    "session_store: 🗑️  removed stale session for '{}' ({})",
                    domain,
                    path.display()
                ),
                Err(e) => warn!(
                    "session_store: failed to remove session file {}: {}",
                    path.display(),
                    e
                ),
            }
        }
    }
    super::auth_registry::invalidate_session(url);
}

// ─────────────────────────────────────────────────────────────────────────────
// Load
// ─────────────────────────────────────────────────────────────────────────────

/// Load stored cookies from the sessions directory by a pre-computed key.
fn load_raw_by_key(key: &str) -> Option<Vec<serde_json::Value>> {
    let path = session_path_by_key(key)?;
    if !path.exists() {
        return None;
    }
    let content = std::fs::read_to_string(&path).ok()?;
    let cookies: Vec<serde_json::Value> = serde_json::from_str(&content).ok()?;
    if cookies.is_empty() {
        return None;
    }
    info!(
        "session_store: 🍪 loaded {} cookies for key '{}' ({})",
        cookies.len(),
        key,
        path.display()
    );
    Some(cookies)
}

/// Load stored cookies for the domain of `url` as raw JSON values.
///
/// **Subdomain fallback:** if no session file exists for the full hostname
/// (e.g. `gist.github.com`), tries the parent domain (`github.com`) before
/// returning `None`.  A single `human_auth_session` on github.com will
/// therefore satisfy scrapes of any `*.github.com` subdomain automatically.
///
/// Returns `None` when no session file can be found for this domain or its
/// parent, or when the file is empty / unreadable.
pub fn load_raw(url: &str) -> Option<Vec<serde_json::Value>> {
    let host = url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_string()))?;

    // Primary: exact hostname.
    let exact_key = host_to_key(&host);
    if let Some(raw) = load_raw_by_key(&exact_key) {
        return Some(raw);
    }

    // Subdomain fallback: try the parent domain (one level up).
    if let Some(parent_key) = parent_domain_key(&host) {
        if let Some(raw) = load_raw_by_key(&parent_key) {
            info!(
                "session_store: 🔗 subdomain fallback — using parent session '{}' for '{}'",
                parent_key, exact_key
            );
            return Some(raw);
        }
    }

    None
}

// ─────────────────────────────────────────────────────────────────────────────
// Inject
// ─────────────────────────────────────────────────────────────────────────────

/// Inject stored session cookies into a live CDP page **before** navigation.
///
/// Cookies are deserialized from raw JSON (`Vec<serde_json::Value>`) into
/// chromiumoxide [`CookieParam`]s and set via the `Network.setCookies` CDP
/// command.  Any individual cookie that fails to deserialise is silently
/// skipped so a partially-malformed session file never blocks a scrape.
///
/// Call this **before** `page.goto(url)` so the cookies are included in the
/// initial HTTP request.
pub async fn inject_into_page(page: &chromiumoxide::Page, raw_cookies: &[serde_json::Value]) {
    use chromiumoxide::cdp::browser_protocol::network::{CookieParam, SetCookiesParams};

    let cookie_params: Vec<CookieParam> = raw_cookies
        .iter()
        .filter_map(|v| serde_json::from_value::<CookieParam>(v.clone()).ok())
        .collect();

    if cookie_params.is_empty() {
        warn!(
            "session_store: stored session JSON contained no valid CookieParams — skipping injection"
        );
        return;
    }

    let count = cookie_params.len();
    match page.execute(SetCookiesParams::new(cookie_params)).await {
        Ok(_) => info!(
            "session_store: 💉 injected {} session cookies into CDP page",
            count
        ),
        Err(e) => warn!("session_store: failed to inject session cookies: {}", e),
    }
}

/// Convenience: load cookies for `url` and inject them into `page` in one call.
///
/// Returns `true` when cookies were found and injected, enabling callers to
/// add post-injection stealth behaviour (jitter delay, mouse micro-move).
/// Returns `false` when no stored session exists for this domain.
pub async fn auto_inject(page: &chromiumoxide::Page, url: &str) -> bool {
    if let Some(raw) = load_raw(url) {
        inject_into_page(page, &raw).await;
        true
    } else {
        false
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── (c) Session-only Cookies ─────────────────────────────────────────────

    /// All cookies are session-scoped (expires = -1).  The effective expiry
    /// must be ≈ now + 24 h (within a 60 s tolerance for test execution).
    #[test]
    fn test_effective_expiry_all_session_scoped() {
        let cookies = vec![
            json!({"name": "session_id", "value": "abc", "expires": -1.0}),
            json!({"name": "csrf_token", "value": "xyz", "expires": -1.0}),
        ];
        let now = chrono::Utc::now().timestamp() as f64;
        let exp = effective_session_expiry(&cookies)
            .expect("should return Some for non-empty session-only jar");
        let diff = (exp - (now + 86_400.0)).abs();
        assert!(diff < 60.0, "expected ≈ now+24h, diff was {diff:.1}s");
        // The original function must still return None.
        assert!(min_cookie_expiry(&cookies).is_none());
    }

    /// Mix of session-scoped and persistent → must return min persistent expiry.
    #[test]
    fn test_effective_expiry_mixed_prefers_persistent() {
        let future_ts = 1_800_000_000.0_f64;
        let cookies = vec![
            json!({"name": "session_id", "value": "s", "expires": -1.0}),
            json!({"name": "remember_me", "value": "1", "expires": future_ts}),
        ];
        let exp = effective_session_expiry(&cookies).unwrap();
        assert!((exp - future_ts).abs() < 1.0);
    }

    /// Empty array → None (nothing to track).
    #[test]
    fn test_effective_expiry_empty_returns_none() {
        assert!(effective_session_expiry(&[]).is_none());
    }

    // ── Subdomain Mapping ────────────────────────────────────────────────────

    #[test]
    fn test_parent_domain_key_strips_one_level() {
        assert_eq!(
            parent_domain_key("gist.github.com"),
            Some("github_com".into())
        );
        assert_eq!(
            parent_domain_key("www.example.com"),
            Some("example_com".into())
        );
        assert_eq!(
            parent_domain_key("api.v2.service.io"),
            Some("v2_service_io".into())
        );
    }

    #[test]
    fn test_parent_domain_key_bare_domain_returns_none() {
        assert!(parent_domain_key("github.com").is_none());
        assert!(parent_domain_key("localhost").is_none());
    }

    #[test]
    fn test_domain_key_from_url() {
        assert_eq!(
            domain_key("https://gist.github.com/user/abc"),
            Some("gist_github_com".into())
        );
        assert_eq!(
            domain_key("https://github.com/user"),
            Some("github_com".into())
        );
    }
}
