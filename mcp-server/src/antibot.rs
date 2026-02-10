use tracing::info;

/// Anti-bot protection features for web scraping
/// Includes request delay, proxy rotation, and advanced stealth headers

/// Collection of realistic user agents for rotation
pub const USER_AGENTS: &[&str] = &[
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:121.0) Gecko/20100101 Firefox/121.0",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.1 Safari/605.1.15",
    "Mozilla/5.0 (X11; Linux x86_64; rv:121.0) Gecko/20100101 Firefox/121.0",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36 Edg/120.0.0.0",
    "Mozilla/5.0 (iPhone; CPU iPhone OS 17_2 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.2 Mobile/15E148 Safari/604.1",
    "Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36",
];

/// Get a random user agent string for stealth
pub fn get_random_user_agent() -> &'static str {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let index = rng.gen_range(0..USER_AGENTS.len());
    USER_AGENTS[index]
}

/// Additional stealth headers to avoid bot detection
pub fn get_stealth_headers() -> Vec<(String, String)> {
    vec![
        ("Accept".to_string(), "text/html,application/xhtml+xml,application/xml;q=0.9,image/webp,*/*;q=0.8".to_string()),
        ("Accept-Language".to_string(), "en-US,en;q=0.9".to_string()),
        ("Accept-Encoding".to_string(), "gzip, deflate, br".to_string()),
        ("DNT".to_string(), "1".to_string()),
        ("Connection".to_string(), "keep-alive".to_string()),
        ("Upgrade-Insecure-Requests".to_string(), "1".to_string()),
        ("Sec-Fetch-Dest".to_string(), "document".to_string()),
        ("Sec-Fetch-Mode".to_string(), "navigate".to_string()),
        ("Sec-Fetch-Site".to_string(), "none".to_string()),
        ("Cache-Control".to_string(), "max-age=0".to_string()),
    ]
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

    /// Get random delay within configured range
    pub fn random_delay(&self) -> u64 {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        rng.gen_range(self.min_ms..=self.max_ms)
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

/// Anti-bot protection manager
pub struct AntiBot {
    delay_config: RequestDelay,
    last_request_time: std::sync::atomic::AtomicU64,
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

        let last = self.last_request_time.load(std::sync::atomic::Ordering::Relaxed);
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
        assert!(!USER_AGENTS.is_empty());
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
        let start = std::time::Instant::now();
        anti_bot.wait_for_next_request().await;
        let elapsed = start.elapsed();
        assert!(elapsed.as_millis() >= 50);
    }
}
