use super::RustScraper;
use crate::antibot;
use anyhow::{anyhow, Result};
// no Html/Duration needed here; Browserless fetches raw HTML via HTTP
use tracing::{info, warn};

impl RustScraper {
    /// Detect domain-specific scraping strategy
    /// Returns (wait_time_ms, needs_scroll)
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

            // Social/search streams: scroll for more results
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

    /// Identify boss-level domains that need extra Browserless care
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

    fn build_browserless_endpoint(
        &self,
        base_url: &str,
        token: Option<String>,
        is_boss_domain: bool,
        path: &str,
        extra_params: &[String],
    ) -> String {
        let mut query_params = Vec::new();
        if let Some(token) = token {
            query_params.push(format!("token={}", token));
        }

        // Only add stealth/clean params for /content endpoint, not /function
        if path != "function" {
            query_params.push("stealth=true".to_string());
            if is_boss_domain {
                query_params.push("clean=true".to_string());
                query_params.push("--disable-web-security=true".to_string());
            }
            query_params.extend_from_slice(extra_params);
        }

        let query_string = if query_params.is_empty() {
            String::new()
        } else {
            format!("?{}", query_params.join("&"))
        };
        format!("{}/{}{}", base_url, path, query_string)
    }

    pub(super) fn extra_browserless_query_params(&self, domain: &Option<String>) -> Vec<String> {
        let mut params = Vec::new();
        if let Some(d) = domain {
            let d = d.to_lowercase();
            if d.contains("substack") || d.contains("medium") {
                params.push("blockAds=true".to_string());
            }
            if d.contains("zillow") {
                params.push("wait=5000".to_string());
            }
        }
        params
    }

    pub(super) fn detect_block_reason(&self, html: &str) -> Option<&'static str> {
        let lower = html.to_lowercase();
        let html_size = html.len();

        // If we got a huge HTML response (>500KB), it's probably not a simple block page
        // Block pages are typically small (< 50KB)
        if html_size > 500_000 {
            // Check if the block message is prominent (appears in first 10KB)
            let preview = &lower[..lower.len().min(10_000)];

            // Only treat as blocked if the error message is in the first 10KB (above the fold)
            if preview.contains("verify you are human") || preview.contains("please verify you") {
                return Some("Human Verification");
            }
            if preview.contains("access denied")
                || preview.contains("access to this page has been denied")
            {
                return Some("Access Denied");
            }
            if preview.contains("captcha") && preview.matches("captcha").count() > 2 {
                // Multiple captcha mentions in preview = likely blocked
                return Some("Captcha");
            }

            // If block text only appears deep in page, ignore it (might be in footer/T&C)
            warn!(
                "Block-like text detected but HTML is {}KB - treating as success",
                html_size / 1024
            );
            return None;
        }

        // For smaller responses, be strict
        if lower.contains("verify you are human") || lower.contains("please verify you") {
            return Some("Human Verification");
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

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn fetch_browserless_content(
        &self,
        url: &str,
        wait_ms: u32,
        browserless_url: &str,
        browserless_token: Option<String>,
        is_boss_domain: bool,
        _user_agent: &str,
        mobile_config: Option<&antibot::MobileStealthConfig>,
        extra_params: &[String],
        proxy_url: Option<String>,
    ) -> Result<(String, u16)> {
        // Detect special domains
        let is_substack = url.contains("substack.com");
        let is_nowsecure = url.contains("nowsecure.nl");
        let is_zillow = url.contains("zillow.com");
        let is_god_domain = is_nowsecure
            || url.contains("tiktok.com")
            || url.contains("opensea.io")
            || url.contains("bloomberg.com");

        // 👹 God Level: Extended wait for Cloudflare/Turnstile
        let effective_wait = if is_nowsecure {
            wait_ms.max(15000) // NowSecure needs 15s+ for Turnstile
        } else if is_god_domain {
            wait_ms.max(10000) // Other God domains get 10s
        } else {
            wait_ms
        };

        let mut wait_override = effective_wait;

        // Get random browser profile for better fingerprinting
        let profile = antibot::get_random_browser_profile();
        info!(
            "Using browser profile: {} ({}x{})",
            profile
                .user_agent
                .split_whitespace()
                .last()
                .unwrap_or("unknown"),
            profile.viewport_width,
            profile.viewport_height
        );

        let mut params = serde_json::json!({
            "url": url,
            "gotoOptions": {
                "waitUntil": if is_substack { "load" } else { "networkidle2" },
                "timeout": if is_god_domain { 90000 } else if is_substack { 30000 } else { 60000 }
            }
        });

        // Substack Strict Diet: block heavy resources to prevent crash
        if is_substack {
            params["rejectResourceTypes"] =
                serde_json::json!(["image", "font", "media", "stylesheet"]);
        }

        // 🖱️ Human Motion Simulation - Adjust wait times for lazy-loaded content
        if is_god_domain || is_zillow {
            warn!("🖱️ Human Motion Simulation: Extended wait for lazy-load");

            // For Zillow and God domains, extend wait to allow content to fully load
            if is_nowsecure {
                // For Turnstile, wait additional time after page load for challenge to complete
                wait_override = 20000;
                warn!("🕐 NowSecure: Extending wait to 20s for Turnstile settlement");
            } else if is_zillow {
                // ⚠️ Zillow blocks any scroll automation (PerimeterX detection)
                // Using JSON extraction only (no scroll) = stable 332 words
                // 1000+ words requires Browserless v2 /function or real browser
                wait_override = effective_wait.saturating_add(5000);
                warn!(
                    "🏠 Zillow: Extended wait for initial JSON (NO SCROLL - PerimeterX triggers on automation)"
                );
            }
        }

        // 👹 God Level: Inject Canvas/WebGL spoofing script
        if is_god_domain {
            warn!("👹 God Level domain detected: Injecting Canvas/WebGL spoofing");
            let spoof_script = self.get_canvas_spoof_script();
            params["addScriptTag"] = serde_json::json!([{ "content": spoof_script }]);
        }

        // UNIVERSAL: Inject stealth script for ALL sites via Browserless
        warn!("💉 Injecting Universal Stealth Engine via Browserless addScriptTag");
        let stealth_script = self.get_universal_stealth_script();
        let mut scripts = params["addScriptTag"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        scripts.push(serde_json::json!({ "content": stealth_script }));
        params["addScriptTag"] = serde_json::Value::Array(scripts);

        // Use random profile UA and sec-ch-ua headers
        if let Some(cfg) = mobile_config {
            params["userAgent"] = serde_json::json!({
                "userAgent": cfg.user_agent,
                "acceptLanguage": "en-US,en;q=0.9",
                "platform": "iOS"
            });
        } else {
            params["userAgent"] = serde_json::json!({
                "userAgent": profile.user_agent,
                "acceptLanguage": "en-US,en;q=0.9",
                "platform": profile.sec_ch_ua_platform.trim_matches('"')
            });
        }

        // Add sec-ch-ua headers for better fingerprinting
        if mobile_config.is_some() {
            params["extraHeaders"] = serde_json::json!({
                "sec-ch-ua": "\"Not_A Brand\";v=\"8\", \"Safari\";v=\"17\"",
                "sec-ch-ua-mobile": "?1",
                "sec-ch-ua-platform": "\"iOS\"",
            });
        } else {
            params["extraHeaders"] = serde_json::json!({
                "sec-ch-ua": profile.sec_ch_ua,
                "sec-ch-ua-mobile": profile.sec_ch_ua_mobile,
                "sec-ch-ua-platform": profile.sec_ch_ua_platform,
            });
        }

        // Use profile viewport or mobile config override
        if let Some(cfg) = mobile_config {
            params["viewport"] = serde_json::json!({
                "width": cfg.viewport_width,
                "height": cfg.viewport_height,
                "isMobile": cfg.is_mobile,
                "hasTouch": cfg.has_touch
            });
        } else {
            params["viewport"] = serde_json::json!({
                "width": profile.viewport_width,
                "height": profile.viewport_height,
                "isMobile": false,
                "hasTouch": false
            });
        }

        // 🔀 PROXY SUPPORT: Add proxy if provided
        if let Some(proxy) = &proxy_url {
            params["proxyServer"] = serde_json::json!(proxy);
            warn!("🔀 Using proxy for Browserless /content: {}", {
                if let Ok(parsed) = url::Url::parse(proxy) {
                    format!(
                        "{}://{}@{}:{}",
                        parsed.scheme(),
                        parsed.username(),
                        parsed.host_str().unwrap_or("unknown"),
                        parsed.port().map(|p| p.to_string()).unwrap_or_default()
                    )
                } else {
                    "invalid-proxy-url".to_string()
                }
            });
        }

        let mut extra_params_all = extra_params.to_vec();
        if !extra_params_all
            .iter()
            .any(|param| param.starts_with("wait="))
        {
            extra_params_all.push(format!("wait={}", wait_override));
        }

        let endpoint = self.build_browserless_endpoint(
            browserless_url,
            browserless_token,
            is_boss_domain,
            "content",
            &extra_params_all,
        );

        let response = self
            .client
            .post(endpoint)
            .header("Content-Type", "application/json")
            .json(&params)
            .send()
            .await
            .map_err(|e| anyhow!("Browserless request failed: {}", e))?;

        let status_code = response.status().as_u16();
        if !response.status().is_success() {
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read error body".to_string());
            return Err(anyhow!(
                "Browserless returned status {}. Response: {}",
                status_code,
                error_body
            ));
        }

        let html = response
            .text()
            .await
            .map_err(|e| anyhow!("Failed to read Browserless response: {}", e))?;

        Ok((html, status_code))
    }
}
