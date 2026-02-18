use super::RustScraper;
use tracing::warn;

impl RustScraper {
    /// Detect domain-specific scraping strategy.
    /// Returns `(wait_time_ms, needs_scroll)`.
    pub(super) fn detect_domain_strategy(&self, domain: &Option<String>) -> (u32, bool) {
        if let Some(d) = domain {
            let d = d.to_lowercase();

            // E-commerce sites: longer wait, scroll for reviews/specs
            if d.contains("amazon") || d.contains("ebay") || d.contains("walmart") {
                return (3000, true);
            }

            // Job sites: scroll for full description
            if d.contains("linkedin") || d.contains("indeed") || d.contains("glassdoor") {
                return (2500, true);
            }

            // Real estate: wait for data hydration
            if d.contains("zillow") || d.contains("redfin") || d.contains("realtor") {
                return (3000, false);
            }

            // Publication platforms: scroll for full article
            if d.contains("substack")
                || d.contains("medium")
                || d.contains("dev.to")
                || d.contains("bloomberg")
            {
                return (2000, true);
            }

            // Social / search streams: scroll for more results
            if d.contains("twitter") || d.contains("x.com") {
                return (2500, true);
            }

            // GitHub: careful with rate limits
            if d.contains("github") {
                return (1500, false);
            }
        }

        // Default: moderate wait, no scroll
        (1000, false)
    }

    /// Returns `true` for domains known to be particularly aggressive about
    /// blocking automated scraping (extra stealth care is warranted).
    pub(super) fn is_boss_domain(&self, domain: &Option<String>) -> bool {
        if let Some(d) = domain {
            let d = d.to_lowercase();
            return d.contains("linkedin")
                || d.contains("zillow")
                || d.contains("redfin")
                || d.contains("trulia")
                || d.contains("substack")
                || d.contains("medium")
                || d.contains("bloomberg")
                || d.contains("instagram")
                || d.contains("twitter")
                || d.contains("x.com");
        }
        false
    }

    /// Inspect HTML response body and return a human-readable block reason,
    /// or `None` when the page appears to be legitimate content.
    pub(super) fn detect_block_reason(&self, html: &str) -> Option<&'static str> {
        let lower = html.to_lowercase();
        let html_size = html.len();

        // If we got a huge HTML response (>500 KB), it's probably not a simple
        // block page — block pages are typically small (<50 KB).
        if html_size > 500_000 {
            let preview = &lower[..lower.len().min(10_000)];

            if preview.contains("verify you are human") || preview.contains("please verify you") {
                return Some("Human Verification");
            }
            if preview.contains("access denied")
                || preview.contains("access to this page has been denied")
            {
                return Some("Access Denied");
            }
            if preview.contains("captcha") && preview.matches("captcha").count() > 2 {
                return Some("Captcha");
            }

            warn!(
                "Block-like text detected but HTML is {}KB - treating as success",
                html_size / 1024
            );
            return None;
        }

        // For smaller responses be strict
        if lower.contains("verify you are human") || lower.contains("please verify you") {
            return Some("Human Verification");
        }
        if lower.contains("duckduckgo.com/anomaly.js")
            || lower.contains("/anomaly.js")
            || lower.contains("anomaly-modal")
        {
            return Some("DuckDuckGo Anomaly");
        }
        if lower.contains("access denied") || lower.contains("access to this page has been denied")
        {
            return Some("Access Denied");
        }
        if lower.contains("captcha")
            || lower.contains("are you human")
            || lower.contains("prove you're human")
        {
            return Some("Captcha");
        }
        if lower.contains("bot detected")
            || lower.contains("unusual traffic")
            || lower.contains("automated request")
        {
            return Some("Bot Detected");
        }
        if lower.contains("cf-chl-")
            || lower.contains("cf-turnstile")
            || lower.contains("turnstile")
        {
            return Some("Cloudflare");
        }
        if lower.contains("perimeterx") || lower.contains("px-captcha") {
            return Some("PerimeterX");
        }
        if lower.contains("page crashed") || lower.contains("crashed!") {
            return Some("JS Crash");
        }
        if lower.contains("zillow group is committed to ensuring digital accessibility")
            && html.len() < 5000
        {
            return Some("Zillow Accessibility Block");
        }

        None
    }
}
