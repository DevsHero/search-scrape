use std::path::Path;

pub const ENV_CHROME_EXECUTABLE: &str = "CHROME_EXECUTABLE";
pub const ENV_LANCEDB_URI: &str = "LANCEDB_URI";
pub const ENV_NEUROSIPHON_ENABLED: &str = "SHADOWCRAWL_NEUROSIPHON";
pub const ENV_MEMORY_DISABLED: &str = "SHADOWCRAWL_MEMORY_DISABLED";

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

/// LanceDB directory/URI for semantic research memory.
///
/// Default behavior is **enabled** with a persistent on-disk store under
/// `~/.shadowcrawl/lancedb` so `research_history` survives VS Code restarts.
///
/// Set `SHADOWCRAWL_MEMORY_DISABLED=1` to disable semantic memory.
pub fn lancedb_uri() -> Option<String> {
    if let Ok(v) = std::env::var(ENV_MEMORY_DISABLED) {
        let v = v.trim().to_ascii_lowercase();
        if matches!(v.as_str(), "1" | "true" | "yes" | "on") {
            return None;
        }
    }

    match std::env::var(ENV_LANCEDB_URI) {
        Ok(v) => {
            let v = v.trim();
            if v.is_empty() {
                None
            } else {
                Some(v.to_string())
            }
        }
        Err(_) => {
            // Stable default path when unset.
            let home = dirs::home_dir()?;
            Some(
                home.join(".shadowcrawl")
                    .join("lancedb")
                    .to_string_lossy()
                    .to_string(),
            )
        }
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
