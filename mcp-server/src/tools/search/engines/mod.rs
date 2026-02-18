pub mod bing;
pub mod brave;
pub mod duckduckgo;
pub mod google;

use anyhow::Result;
use reqwest::StatusCode;
use tracing::warn;

#[derive(Debug)]
pub enum EngineError {
    Blocked { reason: String },
    Transient(String),
    Fatal(String),
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EngineError::Blocked { reason } => write!(f, "blocked: {}", reason),
            EngineError::Transient(e) => write!(f, "transient: {}", e),
            EngineError::Fatal(e) => write!(f, "fatal: {}", e),
        }
    }
}

impl std::error::Error for EngineError {}

pub fn detect_block_reason(status: StatusCode, body: &str) -> Option<String> {
    if status == StatusCode::TOO_MANY_REQUESTS {
        return Some("http_429".to_string());
    }
    if status == StatusCode::FORBIDDEN {
        return Some("http_403".to_string());
    }
    if status == StatusCode::SERVICE_UNAVAILABLE {
        return Some("http_503".to_string());
    }

    let lower = body.to_lowercase();
    let maybe = [
        // DuckDuckGo anomaly / bot-check flow often returns HTTP 200 with no SERP markup.
        ("duckduckgo.com/anomaly.js", "captcha"),
        ("/anomaly.js", "captcha"),
        ("anomaly-modal", "captcha"),
        // Cloudflare / Turnstile / PerimeterX style blocks (often HTTP 200)
        ("cf-chl-", "cloudflare"),
        ("cf-turnstile", "cloudflare"),
        ("turnstile", "cloudflare"),
        ("perimeterx", "captcha"),
        ("px-captcha", "captcha"),
        ("unusual traffic", "unusual_traffic"),
        (
            "our systems have detected unusual traffic",
            "unusual_traffic",
        ),
        (
            "sorry, but your computer or network may be sending automated queries",
            "captcha",
        ),
        ("captcha", "captcha"),
        ("recaptcha", "captcha"),
        ("hcaptcha", "captcha"),
        ("verify you are human", "captcha"),
        ("enable javascript", "js_required"),
        ("access denied", "access_denied"),
    ];

    for (needle, label) in maybe {
        if lower.contains(needle) {
            return Some(label.to_string());
        }
    }

    // Heuristic: tiny HTML + any block-ish token
    if body.len() < 3500 && (lower.contains("captcha") || lower.contains("blocked")) {
        return Some("block_page".to_string());
    }

    None
}

fn env_truthy(key: &str, default: bool) -> bool {
    match std::env::var(key) {
        Ok(v) => {
            let v = v.trim().to_ascii_lowercase();
            !(v.is_empty() || v == "0" || v == "false" || v == "no" || v == "off")
        }
        Err(_) => default,
    }
}

fn cdp_fallback_enabled() -> bool {
    // Preferred name (native CDP). Keep the legacy env var as an alias for backwards compatibility.
    let enabled = match std::env::var("SEARCH_CDP_FALLBACK") {
        Ok(_) => env_truthy("SEARCH_CDP_FALLBACK", true),
        Err(_) => env_truthy("SEARCH_BROWSERLESS_FALLBACK", true),
    };

    enabled && crate::scraping::browser_manager::native_browser_available()
}

fn should_simulate_block(engine: &str) -> bool {
    let Ok(v) = std::env::var("SEARCH_SIMULATE_BLOCK") else {
        return false;
    };
    let v = v.trim().to_ascii_lowercase();
    if v.is_empty() {
        return false;
    }
    if v == "all" || v == "*" {
        return true;
    }
    v.split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .any(|s| s == engine)
}

pub async fn fetch_serp_html(
    client: &reqwest::Client,
    url: reqwest::Url,
    engine: &'static str,
) -> Result<(StatusCode, String), EngineError> {
    if should_simulate_block(engine) {
        warn!("Simulating blocked engine: {}", engine);
        if !cdp_fallback_enabled() {
            return Err(EngineError::Blocked {
                reason: "simulated_block".to_string(),
            });
        }
    }

    let direct = fetch_html(client, url.clone())
        .await
        .map_err(|e| EngineError::Transient(e.to_string()))?;

    let direct_block = detect_block_reason(direct.0, &direct.1)
        .or_else(|| should_simulate_block(engine).then_some("simulated_block".to_string()));

    if let Some(reason) = direct_block {
        if cdp_fallback_enabled() {
            warn!(
                "Engine {} looks blocked ({}); trying native CDP fallback",
                engine, reason
            );
            let scraper = crate::scraping::rust_scraper::RustScraper::new();
            let (status_u16, html) = scraper
                .fetch_html_with_browserless(url.as_str(), None)
                .await
                .map_err(|e| EngineError::Transient(e.to_string()))?;

            let status = StatusCode::from_u16(status_u16).unwrap_or(StatusCode::OK);
            if let Some(reason2) = detect_block_reason(status, &html) {
                return Err(EngineError::Blocked {
                    reason: format!("{}; cdp:{}", reason, reason2),
                });
            }

            return Ok((status, html));
        }

        return Err(EngineError::Blocked { reason });
    }

    Ok(direct)
}

pub async fn fetch_html(
    client: &reqwest::Client,
    url: reqwest::Url,
) -> Result<(StatusCode, String)> {
    let user_agent = crate::antibot::get_random_user_agent();
    let mut req = client
        .get(url)
        .header("User-Agent", user_agent)
        .header("Accept", "text/html,application/xhtml+xml")
        .header(
            "Accept-Language",
            std::env::var("SEARCH_ACCEPT_LANGUAGE").unwrap_or_else(|_| "en-US,en;q=0.9".into()),
        );

    for (k, v) in crate::antibot::get_stealth_headers() {
        req = req.header(k, v);
    }

    let resp = req.send().await?;

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    Ok((status, body))
}
