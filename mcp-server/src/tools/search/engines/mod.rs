pub mod bing;
pub mod brave;
pub mod duckduckgo;
pub mod google;

use anyhow::Result;
use reqwest::StatusCode;

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
