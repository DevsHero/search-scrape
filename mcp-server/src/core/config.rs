use std::path::Path;

pub const ENV_CHROME_EXECUTABLE: &str = "CHROME_EXECUTABLE";
pub const ENV_LANCEDB_URI: &str = "LANCEDB_URI";
pub const ENV_NEUROSIPHON_ENABLED: &str = "SHADOWCRAWL_NEUROSIPHON";

/// Optional override for the Chromium-family browser executable.
///
/// Default behavior is **auto-discovery** (see `scraping::browser_manager::find_chrome_executable()`).
/// This function only returns a value when `CHROME_EXECUTABLE` is set to an existing path.
pub fn chrome_executable_override() -> Option<String> {
    let p = std::env::var(ENV_CHROME_EXECUTABLE).ok()?;
    let p = p.trim();
    if p.is_empty() {
        return None;
    }
    if Path::new(p).exists() {
        Some(p.to_string())
    } else {
        None
    }
}

/// Optional LanceDB directory/URI for semantic research memory.
///
/// Default behavior is **disabled** (no implicit on-disk state) unless `LANCEDB_URI` is set.
pub fn lancedb_uri() -> Option<String> {
    let v = std::env::var(ENV_LANCEDB_URI).ok()?;
    let v = v.trim();
    if v.is_empty() {
        None
    } else {
        Some(v.to_string())
    }
}

/// Global toggle for NeuroSiphon-derived optimizations (content router, noise filter,
/// semantic shaving, import nuking, search rewrite/rerank, etc.).
///
/// Default: enabled. Set `SHADOWCRAWL_NEUROSIPHON=0` (or `false`/`no`) to disable.
pub fn neurosiphon_enabled() -> bool {
    let Ok(v) = std::env::var(ENV_NEUROSIPHON_ENABLED) else {
        return true;
    };
    let v = v.trim().to_ascii_lowercase();
    if v.is_empty() {
        return true;
    }
    !matches!(v.as_str(), "0" | "false" | "no" | "off" | "disabled")
}
