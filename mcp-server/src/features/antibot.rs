use std::env;
use tracing::info;

#[derive(Debug, Clone)]
pub struct BrowserProfile {
    pub user_agent: String,
    pub sec_ch_ua: String,
    pub sec_ch_ua_mobile: String,
    pub sec_ch_ua_platform: String,
    pub viewport_width: u32,
    pub viewport_height: u32,
}

pub const USER_AGENTS: &[&str] = &[
    // Chrome Desktop (Windows, macOS, Linux) - Latest 2026 versions
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/132.0.0.0 Safari/537.36",

    // Firefox Desktop
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:121.0) Gecko/20100101 Firefox/121.0",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 14.2; rv:122.0) Gecko/20100101 Firefox/122.0",
    "Mozilla/5.0 (X11; Linux x86_64; rv:121.0) Gecko/20100101 Firefox/121.0",

    // Safari Desktop
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.1 Safari/605.1.15",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_2_1) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.2 Safari/605.1.15",

    // Edge Desktop
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36 Edg/120.0.0.0",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/121.0.0.0 Safari/537.36 Edg/121.0.0.0",

    // Mobile Safari (iPhone)
    "Mozilla/5.0 (iPhone; CPU iPhone OS 17_2 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.2 Mobile/15E148 Safari/604.1",
    "Mozilla/5.0 (iPhone; CPU iPhone OS 17_1_1 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.1 Mobile/15E148 Safari/604.1",
    "Mozilla/5.0 (iPad; CPU OS 17_2 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.2 Mobile/15E148 Safari/604.1",

    // Mobile Chrome (Android)
    "Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36",
    "Mozilla/5.0 (Linux; Android 13; SM-S918B) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/121.0.0.0 Mobile Safari/537.36",
    "Mozilla/5.0 (Linux; Android 14; Pixel 8 Pro) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.6099.210 Mobile Safari/537.36",
];

/// Get a random user agent string for stealth
pub fn get_random_user_agent() -> &'static str {
    use rand::prelude::*;
    let mut rng = rand::rng();
    let index = rng.random_range(0..USER_AGENTS.len());
    USER_AGENTS[index]
}

/// Get a complete browser profile with matching fingerprint (UA + sec-ch-ua headers)
pub fn get_random_browser_profile() -> BrowserProfile {
    use rand::prelude::*;
    let mut rng = rand::rng();
    let profiles = vec![
        // Chrome 131 on Windows 10
        BrowserProfile {
            user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36".to_string(),
            sec_ch_ua: r#""Chromium";v="131", "Not_A Brand";v="24", "Google Chrome";v="131""#.to_string(),
            sec_ch_ua_mobile: "?0".to_string(),
            sec_ch_ua_platform: "\"Windows\"".to_string(),
            viewport_width: 1920,
            viewport_height: 1080,
        },
        // Chrome 131 on macOS
        BrowserProfile {
            user_agent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36".to_string(),
            sec_ch_ua: r#""Chromium";v="131", "Not_A Brand";v="24", "Google Chrome";v="131""#.to_string(),
            sec_ch_ua_mobile: "?0".to_string(),
            sec_ch_ua_platform: "\"macOS\"".to_string(),
            viewport_width: 1440,
            viewport_height: 900,
        },
        // Edge 131 on Windows 11
        BrowserProfile {
            user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36 Edg/131.0.0.0".to_string(),
            sec_ch_ua: r#""Chromium";v="131", "Not_A Brand";v="24", "Microsoft Edge";v="131""#.to_string(),
            sec_ch_ua_mobile: "?0".to_string(),
            sec_ch_ua_platform: "\"Windows\"".to_string(),
            viewport_width: 1920,
            viewport_height: 1080,
        },
        // Chrome 130 on Linux
        BrowserProfile {
            user_agent: "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36".to_string(),
            sec_ch_ua: r#""Chromium";v="130", "Not_A Brand";v="24", "Google Chrome";v="130""#.to_string(),
            sec_ch_ua_mobile: "?0".to_string(),
            sec_ch_ua_platform: "\"Linux\"".to_string(),
            viewport_width: 1920,
            viewport_height: 1080,
        },
        // Mobile Safari on iPhone 15 Pro
        BrowserProfile {
            user_agent: "Mozilla/5.0 (iPhone; CPU iPhone OS 17_2 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.2 Mobile/15E148 Safari/604.1".to_string(),
            sec_ch_ua: r#""Not_A Brand";v="8", "Chromium";v="120""#.to_string(),
            sec_ch_ua_mobile: "?1".to_string(),
            sec_ch_ua_platform: "\"iOS\"".to_string(),
            viewport_width: 393,
            viewport_height: 852,
        },
    ];
    let index = rng.random_range(0..profiles.len());
    profiles[index].clone()
}

/// Additional stealth headers to avoid bot detection
pub fn get_stealth_headers() -> Vec<(String, String)> {
    vec![
        (
            "Accept".to_string(),
            "text/html,application/xhtml+xml,application/xml;q=0.9,image/webp,*/*;q=0.8"
                .to_string(),
        ),
        ("Accept-Language".to_string(), "en-US,en;q=0.9".to_string()),
        (
            "Accept-Encoding".to_string(),
            "gzip, deflate, br".to_string(),
        ),
        ("DNT".to_string(), "1".to_string()),
        ("Connection".to_string(), "keep-alive".to_string()),
        ("Upgrade-Insecure-Requests".to_string(), "1".to_string()),
        ("Sec-Fetch-Dest".to_string(), "document".to_string()),
        ("Sec-Fetch-Mode".to_string(), "navigate".to_string()),
        ("Sec-Fetch-Site".to_string(), "none".to_string()),
        ("Cache-Control".to_string(), "max-age=0".to_string()),
    ]
}

pub struct MobileStealthConfig {
    pub user_agent: &'static str,
    pub viewport_width: u32,
    pub viewport_height: u32,
    pub is_mobile: bool,
    pub has_touch: bool,
}

/// Mobile Safari config for bot-sensitive sites (e.g., Zillow)
pub fn get_mobile_stealth_config() -> MobileStealthConfig {
    MobileStealthConfig {
        user_agent: "Mozilla/5.0 (iPhone; CPU iPhone OS 17_2 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.2 Mobile/15E148 Safari/604.1",
        viewport_width: 390,
        viewport_height: 844,
        is_mobile: true,
        has_touch: true,
    }
}

/// Human-like delay after page load to reduce bot signals
pub fn boss_domain_post_load_delay_ms() -> u64 {
    use rand::prelude::*;
    let mut rng = rand::rng();
    rng.random_range(2000..=4000)
}

/// Request delay configuration for polite scraping
#[derive(Debug, Clone, Copy)]
pub struct RequestDelay {
    /// Minimum delay in milliseconds between requests
    pub min_ms: u64,
    /// Maximum delay in milliseconds between requests
    pub max_ms: u64,
}

impl RequestDelay {
    pub fn new(min_ms: u64, max_ms: u64) -> Self {
        Self { min_ms, max_ms }
    }

    /// Get random delay within configured range with jitter
    pub fn random_delay(&self) -> u64 {
        use rand::prelude::*;
        let mut rng = rand::rng();
        let base_delay = rng.random_range(self.min_ms..=self.max_ms);

        // Add ±20% jitter to avoid pattern detection
        let jitter_range = (base_delay as f64 * 0.2) as i64;
        let jitter = rng.random_range(-jitter_range..=jitter_range);

        (base_delay as i64 + jitter).max(self.min_ms as i64) as u64
    }

    /// Default polite delay: 500ms-1500ms
    pub fn default_polite() -> Self {
        Self {
            min_ms: 500,
            max_ms: 1500,
        }
    }

    /// Aggressive delay: 100ms-500ms (for trusted APIs)
    pub fn aggressive() -> Self {
        Self {
            min_ms: 100,
            max_ms: 500,
        }
    }

    /// Conservative delay: 1000ms-3000ms (for protected sites)
    pub fn conservative() -> Self {
        Self {
            min_ms: 1000,
            max_ms: 3000,
        }
    }
}

pub fn request_delay_from_env() -> RequestDelay {
    let preset = env::var("SCRAPE_DELAY_PRESET")
        .ok()
        .map(|v| v.to_lowercase());
    let (default_min, default_max) = match preset.as_deref() {
        Some("fast") => (100, 500),
        Some("conservative") => (1000, 3000),
        _ => (500, 1500),
    };
    let min_ms = env::var("SCRAPE_DELAY_MIN_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(default_min);
    let max_ms = env::var("SCRAPE_DELAY_MAX_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(default_max);
    let (min_ms, max_ms) = if min_ms > max_ms {
        (max_ms, min_ms)
    } else {
        (min_ms, max_ms)
    };
    RequestDelay::new(min_ms, max_ms)
}

pub async fn apply_request_delay() {
    let delay = request_delay_from_env().random_delay();
    if delay > 0 {
        info!("Applying request delay: {}ms", delay);
        tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
    }
}

/// Anti-bot protection manager
pub struct AntiBot {
    delay_config: RequestDelay,
    last_request_time: std::sync::atomic::AtomicU64,
}

/// Proxy rotation manager for enhanced stealth
/// Set PROXY_LIST env var with comma-separated proxies (e.g., "http://proxy1:8080,socks5://proxy2:1080")
#[derive(Debug, Clone)]
pub struct ProxyRotator {
    proxies: Vec<String>,
    current_idx: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl ProxyRotator {
    /// Create a new proxy rotator from comma-separated proxy list
    pub fn from_env() -> Option<Self> {
        let proxy_list = std::env::var("PROXY_LIST").ok()?;
        let proxies: Vec<String> = proxy_list
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        if proxies.is_empty() {
            None
        } else {
            tracing::info!("Loaded {} proxies for rotation", proxies.len());
            Some(Self {
                proxies,
                current_idx: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            })
        }
    }

    /// Get the next proxy URL in rotation
    pub fn next_proxy(&self) -> Option<String> {
        if self.proxies.is_empty() {
            return None;
        }
        let idx = self
            .current_idx
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            % self.proxies.len();
        Some(self.proxies[idx].clone())
    }

    /// Get the number of available proxies
    pub fn count(&self) -> usize {
        self.proxies.len()
    }
}

impl AntiBot {
    pub fn new(delay_config: RequestDelay) -> Self {
        Self {
            delay_config,
            last_request_time: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Wait appropriate delay before next request
    pub async fn wait_for_next_request(&self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let last = self
            .last_request_time
            .load(std::sync::atomic::Ordering::Relaxed);
        let elapsed = now.saturating_sub(last);
        let delay = self.delay_config.random_delay();

        if elapsed < delay {
            let wait_ms = delay - elapsed;
            info!("Waiting {}ms before next request (rate limiting)", wait_ms);
            tokio::time::sleep(tokio::time::Duration::from_millis(wait_ms)).await;
        }

        self.last_request_time
            .store(now + delay, std::sync::atomic::Ordering::Relaxed);
    }

    /// Reset rate limiting
    pub fn reset(&self) {
        self.last_request_time
            .store(0, std::sync::atomic::Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_agents() {
        assert!(USER_AGENTS[0].contains("Mozilla"));
    }

    #[test]
    fn test_delay_configs() {
        let polite = RequestDelay::default_polite();
        assert_eq!(polite.min_ms, 500);
        assert_eq!(polite.max_ms, 1500);

        let aggressive = RequestDelay::aggressive();
        assert_eq!(aggressive.min_ms, 100);

        let conservative = RequestDelay::conservative();
        assert_eq!(conservative.min_ms, 1000);
    }

    #[tokio::test]
    async fn test_anti_bot_delay() {
        let anti_bot = AntiBot::new(RequestDelay::new(50, 100));
        // First call initializes last_request_time
        anti_bot.wait_for_next_request().await;

        let start = std::time::Instant::now();
        // Second call should wait for the delay
        anti_bot.wait_for_next_request().await;
        let elapsed = start.elapsed();
        assert!(elapsed.as_millis() >= 40); // Allow slight timing variance
    }
}
