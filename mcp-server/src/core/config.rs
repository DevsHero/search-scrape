use std::path::Path;

// ---------------------------------------------------------------------------
// ShadowConfig — file-based config loader (cortex-scout.json) with env-var fallback
// ---------------------------------------------------------------------------

/// Deep-research sub-config (mirrors the `deep_research` key in cortex-scout.json).
#[derive(serde::Deserialize, Default, Clone, Debug)]
pub struct ShadowDeepResearchConfig {
    /// Whether the deep_research tool is exposed at all. Defaults to `true`.
    pub enabled: Option<bool>,
    /// LLM endpoint — e.g. `https://api.openai.com/v1` or `http://localhost:11434/v1` (Ollama).
    pub llm_base_url: Option<String>,
    /// API key. Never logged. Leave blank for key-less local endpoints.
    pub llm_api_key: Option<String>,
    /// Model name — e.g. `gpt-4o-mini`, `llama3`, `mistral`.
    pub llm_model: Option<String>,
    /// Max source documents fed to the LLM synthesis step. Default: 8.
    pub synthesis_max_sources: Option<usize>,
    /// Max characters extracted per source for synthesis. Default: 2500.
    pub synthesis_max_chars_per_source: Option<usize>,
    /// Set to `false` to run search+scrape only, skipping LLM synthesis entirely.
    pub synthesis_enabled: Option<bool>,
    /// Max tokens the LLM may generate in the synthesis response.
    /// Tune this to fit your model's context window.
    /// Default: 1024. For small models (4k ctx) try 512–1024; for large models try 2048–4096.
    pub synthesis_max_tokens: Option<u32>,
}

impl ShadowDeepResearchConfig {
    /// API key: JSON field → `OPENAI_API_KEY` env var → `None`.
    ///
    /// When `llm_api_key` is explicitly set to `""` in the config file, returns `Some("")`.
    /// This signals "no key required" (Ollama / LM Studio) — synthesis proceeds without auth.
    /// Returns `None` only when the field is absent from config AND `OPENAI_API_KEY` is unset.
    pub fn resolve_api_key(&self) -> Option<String> {
        // If the field is present in JSON (even as empty string), use it as-is.
        // An explicit empty string means "no key required" (local endpoint).
        if let Some(k) = &self.llm_api_key {
            return Some(k.trim().to_string());
        }
        // Field absent from JSON — fall back to OPENAI_API_KEY env var.
        std::env::var("OPENAI_API_KEY").ok().filter(|v| !v.trim().is_empty())
    }

    /// LLM base URL: JSON field → `OPENAI_BASE_URL` env var → `https://api.openai.com/v1`.
    pub fn resolve_base_url(&self) -> String {
        if let Some(u) = &self.llm_base_url {
            if !u.trim().is_empty() {
                return u.clone();
            }
        }
        std::env::var("OPENAI_BASE_URL")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| "https://api.openai.com/v1".to_string())
    }

    /// Model name: JSON field → `DEEP_RESEARCH_LLM_MODEL` env var → `gpt-4o-mini`.
    pub fn resolve_model(&self) -> String {
        if let Some(m) = &self.llm_model {
            if !m.trim().is_empty() {
                return m.clone();
            }
        }
        std::env::var("DEEP_RESEARCH_LLM_MODEL")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| "gpt-4o-mini".to_string())
    }

    /// Max synthesis sources: JSON field → `DEEP_RESEARCH_SYNTHESIS_MAX_SOURCES` env → 8.
    pub fn resolve_max_sources(&self) -> usize {
        if let Some(n) = self.synthesis_max_sources {
            return n;
        }
        std::env::var("DEEP_RESEARCH_SYNTHESIS_MAX_SOURCES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(8)
    }

    /// Max chars per source: JSON field → `DEEP_RESEARCH_SYNTHESIS_MAX_CHARS_PER_SOURCE` → 2500.
    pub fn resolve_max_chars_per_source(&self) -> usize {
        if let Some(n) = self.synthesis_max_chars_per_source {
            return n;
        }
        std::env::var("DEEP_RESEARCH_SYNTHESIS_MAX_CHARS_PER_SOURCE")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(2500)
    }

    /// Whether LLM synthesis is enabled: JSON `synthesis_enabled` field → `DEEP_RESEARCH_SYNTHESIS`
    /// env var (set to "0" to disable) → `true`.
    pub fn resolve_synthesis_enabled(&self) -> bool {
        if let Some(b) = self.synthesis_enabled {
            return b;
        }
        // Legacy env var: "0" means disabled
        std::env::var("DEEP_RESEARCH_SYNTHESIS")
            .map(|v| v.trim() != "0")
            .unwrap_or(true)
    }

    /// Max output tokens: JSON field → `DEEP_RESEARCH_SYNTHESIS_MAX_TOKENS` env var → 1024.
    pub fn resolve_max_tokens(&self) -> u32 {
        if let Some(n) = self.synthesis_max_tokens {
            return n;
        }
        std::env::var("DEEP_RESEARCH_SYNTHESIS_MAX_TOKENS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1024)
    }
}

/// Top-level config loaded from `cortex-scout.json`.
#[derive(serde::Deserialize, Default, Clone, Debug)]
pub struct ShadowConfig {
    pub deep_research: ShadowDeepResearchConfig,
}

/// Load `cortex-scout.json` from standard locations.
///
/// Search order (first found wins):
/// 1. `./cortex-scout.json`  (process cwd — inside the mcp-server dir during `cargo run`)
/// 2. `../cortex-scout.json` (one level up — repo root when running from `mcp-server/`)
/// 3. `CORTEX_SCOUT_CONFIG` env var path
///
/// Missing file → `ShadowConfig::default()` (silent, all env-var fallbacks apply).
/// Parse error → log a warning, return `ShadowConfig::default()`.
pub fn load_shadow_config() -> ShadowConfig {
    let candidates: Vec<std::path::PathBuf> = {
        let mut v = vec![
            std::path::PathBuf::from("cortex-scout.json"),
            std::path::PathBuf::from("../cortex-scout.json"),
        ];
        if let Ok(env_path) = std::env::var("CORTEX_SCOUT_CONFIG") {
            v.insert(0, std::path::PathBuf::from(env_path));
        }
        v
    };

    for path in &candidates {
        match std::fs::read_to_string(path) {
            Ok(contents) => {
                match serde_json::from_str::<ShadowConfig>(&contents) {
                    Ok(cfg) => {
                        tracing::info!(
                            "cortex-scout.json loaded from {}",
                            path.display()
                        );
                        return cfg;
                    }
                    Err(e) => {
                        tracing::warn!(
                            "cortex-scout.json parse error at {}: {} — using defaults",
                            path.display(),
                            e
                        );
                        return ShadowConfig::default();
                    }
                }
            }
            Err(_) => continue, // file not found at this path — try next
        }
    }

    // No config file found anywhere — silently use defaults (all env-var fallbacks will apply).
    ShadowConfig::default()
}

// ---------------------------------------------------------------------------

pub const ENV_CHROME_EXECUTABLE: &str = "CHROME_EXECUTABLE";
pub const ENV_LANCEDB_URI: &str = "LANCEDB_URI";
pub const ENV_NEUROSIPHON_ENABLED: &str = "CORTEX_SCOUT_NEUROSIPHON";
pub const ENV_MEMORY_DISABLED: &str = "CORTEX_SCOUT_MEMORY_DISABLED";

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
/// `~/.cortex-scout/lancedb` so `research_history` survives VS Code restarts.
///
/// Set `CORTEX_SCOUT_MEMORY_DISABLED=1` to disable semantic memory.
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
                home.join(".cortex-scout")
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
/// Default: enabled. Set `CORTEX_SCOUT_NEUROSIPHON=0` (or `false`/`no`) to disable.
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
