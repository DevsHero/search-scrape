mod browserless;
mod cdp;
mod clean;
mod jsonld;
mod metadata;
mod parse;
mod quality;
mod stealth;

use crate::antibot;
use crate::types::*;
use anyhow::{anyhow, Result};
use chrono::Utc;
use reqwest::Client;
use scraper::Html;
use std::collections::HashSet;
use std::time::Duration;
use tracing::info;
use url::Url;

/// Enhanced Rust-native web scraper with anti-bot protection
pub struct RustScraper {
    client: Client,
    quality_mode: QualityMode,
    /// When `true`, force-return embedded SPA JSON state (Next.js/Nuxt/Remix)
    /// regardless of its word-count.  When `false` (default), the SPA JSON path
    /// is only taken if it yields ≥ 100 readable words; otherwise the standard
    /// readability pipeline is used instead.
    pub extract_app_state: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QualityMode {
    Balanced,
    Aggressive,
    High,
}

impl QualityMode {
    pub fn parse_str(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "balanced" => Some(QualityMode::Balanced),
            "aggressive" => Some(QualityMode::Aggressive),
            "high" => Some(QualityMode::High),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            QualityMode::Balanced => "balanced",
            QualityMode::Aggressive => "aggressive",
            QualityMode::High => "high",
        }
    }

    pub fn from_option(value: Option<&str>) -> Self {
        value
            .and_then(Self::parse_str)
            .unwrap_or(QualityMode::Balanced)
    }
}

pub struct PreflightCheck {
    pub status_code: u16,
    pub word_count: usize,
    pub blocked_reason: Option<String>,
}

impl RustScraper {
    pub fn new() -> Self {
        Self::new_with_quality_mode(None)
    }

    pub fn new_with_quality_mode(quality_mode: Option<&str>) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            quality_mode: QualityMode::from_option(quality_mode),
            extract_app_state: false,
        }
    }

    /// Builder: override the `extract_app_state` default.
    pub fn with_extract_app_state(mut self, val: bool) -> Self {
        self.extract_app_state = val;
        self
    }

    pub(super) fn is_aggressive_mode(&self) -> bool {
        matches!(self.quality_mode, QualityMode::Aggressive)
    }

    /// Infer a programming language from the URL path extension.
    /// Used so raw source files (e.g. `raw.githubusercontent.com/.../*.rs`)
    /// receive a language tag even though no HTML code-fence class is present.
    /// Detect documentation / tutorial site URLs where import nuking must be disabled.
    /// On these sites imports are critical context, not noise, and must never be stripped.
    pub(super) fn is_tutorial_url(url: &Url) -> bool {
        let host = url.host_str().unwrap_or("").to_ascii_lowercase();
        let path = url.path().to_ascii_lowercase();
        host.contains("doc.rust-lang.org")
            || host.contains("docs.rs")
            || host.contains("developer.mozilla.org")
            || host.contains("docs.python.org")
            || host.contains("learn.microsoft.com")
            || host.contains("docs.microsoft.com")
            || host.contains("reactjs.org")
            || host.contains("vuejs.org")
            || host.starts_with("docs.")
            || host.starts_with("doc.")
            || path.contains("/tutorial")
            || path.contains("/guide")
            || path.contains("/docs/")
            || path.contains("/book/")
            || path.contains("/learn/")
    }

    pub(super) fn infer_language_from_url(url: &Url) -> Option<String> {
        let path = url.path();
        let ext = path.rsplit('.').next()?.to_ascii_lowercase();
        match ext.as_str() {
            "rs"              => Some("rust".to_string()),
            "py" | "pyw"      => Some("python".to_string()),
            "js" | "mjs" | "cjs" => Some("javascript".to_string()),
            "ts" | "mts" | "cts" => Some("typescript".to_string()),
            "go"              => Some("go".to_string()),
            "java"            => Some("java".to_string()),
            "kt"              => Some("kotlin".to_string()),
            "cs"              => Some("csharp".to_string()),
            "rb"              => Some("ruby".to_string()),
            _                 => None,
        }
    }

    pub async fn preflight_check(&self, url: &str) -> Result<PreflightCheck> {
        let user_agent = antibot::get_random_user_agent();
        let mut request_builder = self
            .client
            .get(url)
            .header("User-Agent", user_agent)
            .timeout(Duration::from_secs(5));

        for (header_name, header_value) in antibot::get_stealth_headers() {
            request_builder = request_builder.header(header_name, header_value);
        }

        let response = request_builder
            .send()
            .await
            .map_err(|e| anyhow!("Preflight request failed: {}", e))?;

        let status_code = response.status().as_u16();
        let html = response
            .text()
            .await
            .map_err(|e| anyhow!("Preflight read failed: {}", e))?;

        let blocked_reason = self.detect_block_reason(&html).map(|s| s.to_string());
        let clean_content = html2md::parse_html(&html);
        let word_count = self.count_words(&clean_content);

        Ok(PreflightCheck {
            status_code,
            word_count,
            blocked_reason,
        })
    }

    /// Scrape a URL with enhanced content extraction and anti-bot protection
    pub async fn scrape_url(&self, url: &str) -> Result<ScrapeResponse> {
        info!("Scraping URL with Rust-native scraper: {}", url);

        // Validate URL
        let parsed_url = Url::parse(url).map_err(|e| anyhow!("Invalid URL '{}': {}", url, e))?;

        if parsed_url.scheme() != "http" && parsed_url.scheme() != "https" {
            return Err(anyhow!("URL must use HTTP or HTTPS protocol"));
        }

        // Apply anti-bot delay before request
        antibot::apply_request_delay().await;

        // Make HTTP request with anti-bot protection (random User-Agent + stealth headers)
        let user_agent = antibot::get_random_user_agent();
        let mut request_builder = self.client.get(url).header("User-Agent", user_agent);

        // Apply stealth headers to avoid bot detection
        for (header_name, header_value) in antibot::get_stealth_headers() {
            request_builder = request_builder.header(header_name, header_value);
        }

        let response = request_builder
            .send()
            .await
            .map_err(|e| anyhow!("Failed to fetch URL: {}", e))?;

        let status_code = response.status().as_u16();
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("text/html")
            .to_string();

        // Get response body
        let html = response
            .text()
            .await
            .map_err(|e| anyhow!("Failed to read response body: {}", e))?;

        // Parse HTML
        let document = Html::parse_document(&html);

        // Extract basic metadata
        let title = self.extract_title(&document);
        let meta_description = self.extract_meta_description(&document);
        let meta_keywords = self.extract_meta_keywords(&document);
        let language = self.detect_language(&document, &html);
        let canonical_url = self.extract_canonical(&document, &parsed_url);
        let site_name = self.extract_site_name(&document);
        let (og_title, og_description, og_image) = self.extract_open_graph(&document, &parsed_url);
        let author = self.extract_author(&document);
        let published_at = self.extract_published_time(&document);

        // Extract code blocks BEFORE html2text conversion (Priority 1 fix)
        // 🧬 Rule B: infer language from the URL extension so raw source files
        // (e.g. raw.githubusercontent.com/.../*.rs) receive import nuking even
        // though they carry no HTML code-fence class attributes.
        let url_lang_hint = Self::infer_language_from_url(&parsed_url);
        let is_tutorial = Self::is_tutorial_url(&parsed_url);
        let code_blocks = self.extract_code_blocks(&document, url_lang_hint.as_deref(), is_tutorial);

        // JSON-LD can be the cleanest source on modern sites; prefer it when present.
        let json_ld_content = self.extract_json_ld(&document);

        // ── 🧬 SPA fast-path (before JSON-LD): prefer embedded state blobs when present.
        // Many SPAs include thin JSON-LD that omits the real page content; embedded state is often richer.
        let spa_state_content = if crate::core::config::neurosiphon_enabled()
            && crate::scraping::rust_scraper::clean::looks_like_spa(&html)
        {
            // 🧬 Rule C: only commit to the SPA JSON fast-path when it yields enough
            // readable content (≥ 100 words) OR the caller explicitly requested raw
            // app state via `extract_app_state = true`.
            self.extract_spa_json_state(&html).filter(|extracted| {
                self.extract_app_state || self.count_words(extracted) >= 100
            })
        } else {
            None
        };

        // Extract readable content using readability (fallback)
        let (mut clean_content, noise_reduction_ratio) =
            if let Some(spa_content) = spa_state_content.as_ref() {
                (self.normalize_markdown_fragments(spa_content), 0.0)
            } else if let Some(json_content) = json_ld_content.as_ref() {
                (
                    self.normalize_markdown_fragments(&html2md::parse_html(json_content)),
                    0.0,
                )
            } else {
                self.extract_clean_content_with_metrics(&html, &parsed_url)
            };
        clean_content = self.normalize_markdown_fragments(&clean_content);
        clean_content = self.apply_og_description_fallback(clean_content, &og_description);
        clean_content = self.clean_noise(&clean_content);

        // Extract structured data
        let headings = self.extract_headings(&document);
        // Smart link extraction: prefer content links over all document links
        let links = self.extract_content_links(&document, &parsed_url);
        let images = self.extract_images(&document, &parsed_url);

        // 🧬 Task 3: When extract_app_state=true and SPA hydration JSON was found, discard
        // all DOM-derived content (code_blocks, links, images, headings).  The hydration
        // JSON IS the content; DOM scaffolding is pure token waste in this mode.
        let spa_forced = self.extract_app_state && spa_state_content.is_some();
        let code_blocks = if spa_forced { vec![] } else { code_blocks };
        let links      = if spa_forced { vec![] } else { links };
        let images     = if spa_forced { vec![] } else { images };
        let headings   = if spa_forced { vec![] } else { headings };

        let mut embedded_data_sources = self.collect_embedded_data_sources(&document);
        let mut embedded_state_json = embedded_data_sources
            .iter()
            .max_by_key(|s| s.content.len())
            .map(|s| s.content.clone())
            .or_else(|| self.extract_embedded_state_json(&document));
        let mut warnings = Vec::new();
        const MAX_STATE_JSON_CHARS: usize = 200_000;
        for src in embedded_data_sources.iter_mut() {
            if src.content.len() > MAX_STATE_JSON_CHARS {
                src.content = src.content.chars().take(MAX_STATE_JSON_CHARS).collect();
                warnings.push("embedded_data_sources_truncated".to_string());
            }
        }
        if let Some(state) = embedded_state_json.as_ref() {
            if state.len() > MAX_STATE_JSON_CHARS {
                embedded_state_json = Some(state.chars().take(MAX_STATE_JSON_CHARS).collect());
                warnings.push("embedded_state_json_truncated".to_string());
            }
        }

        let hydration_status = crate::types::HydrationStatus {
            json_found: !embedded_data_sources.is_empty() || embedded_state_json.is_some(),
            settle_time_ms: None,
            noise_reduction_ratio,
        };

        clean_content = self.append_image_context_markdown(clean_content, &images, &title);
        let word_count = self.count_words(&clean_content);
        let reading_time_minutes = Some(((word_count as f64 / 200.0).ceil() as u32).max(1));

        // Calculate extraction quality score (Priority 1 fix)
        let extraction_score =
            self.calculate_extraction_score(word_count, &published_at, &code_blocks, &headings);

        // Extract domain from URL (Priority 2 enhancement)
        let domain = parsed_url.host_str().map(|h| h.to_string());

        let result = ScrapeResponse {
            url: url.to_string(),
            title,
            content: html,
            clean_content,
            embedded_state_json,
            embedded_data_sources,
            hydration_status,
            meta_description,
            meta_keywords,
            headings,
            links,
            images,
            timestamp: Utc::now().to_rfc3339(),
            status_code,
            content_type,
            word_count,
            language,
            canonical_url,
            site_name,
            author,
            published_at,
            og_title,
            og_description,
            og_image,
            reading_time_minutes,
            // New Priority 1 fields
            code_blocks,
            truncated: false,      // Will be set by caller based on max_chars
            actual_chars: 0,       // Will be set by caller
            max_chars_limit: None, // Will be set by caller
            extraction_score: Some(extraction_score),
            warnings,
            domain,
        };

        info!(
            "Successfully scraped: {} ({} words, score: {:.2})",
            result.title, result.word_count, extraction_score
        );
        Ok(result)
    }

    /// Scrape a URL using the native headless browser (chromiumoxide) for JS-heavy sites.
    /// This is used as a fallback when static scraping returns poor quality.
    pub async fn scrape_with_browserless(&self, url: &str) -> Result<ScrapeResponse> {
        self.scrape_with_browserless_advanced(url, None).await
    }

    /// Fetch raw HTML via the native headless browser without running the full
    /// extraction pipeline.  Used for lightweight cases like SERP fetching.
    pub async fn fetch_html_with_browserless(
        &self,
        url: &str,
        custom_wait: Option<u32>,
    ) -> Result<(u16, String)> {
        use crate::scraping::browser_manager;

        antibot::apply_request_delay().await;

        let domain = url::Url::parse(url)
            .ok()
            .and_then(|u| u.host_str().map(|s| s.to_lowercase()));
        let (wait_time, needs_scroll) = self.detect_domain_strategy(&domain);
        let is_boss_domain = self.is_boss_domain(&domain);

        let mut effective_wait = custom_wait.unwrap_or(wait_time);
        if needs_scroll {
            effective_wait = effective_wait.saturating_add(2000);
        }
        if is_boss_domain {
            effective_wait =
                effective_wait.saturating_add(antibot::boss_domain_post_load_delay_ms() as u32);
        }

        let (status, html) = browser_manager::fetch_html_native(url, Some(effective_wait)).await?;

        // Mobile Safari retry if blocked
        if self.detect_block_reason(&html).is_some() {
            info!("Native fetch blocked, retrying with mobile profile");
            if let Ok((retry_status, retry_html)) = browser_manager::fetch_html_native_mobile(
                url,
                Some(effective_wait.saturating_add(1500)),
            )
            .await
            {
                return Ok((retry_status, retry_html));
            }
        }

        Ok((status, html))
    }

    /// Advanced Browserless scraping with custom actions and domain detection
    pub async fn scrape_with_browserless_advanced(
        &self,
        url: &str,
        custom_wait: Option<u32>,
    ) -> Result<ScrapeResponse> {
        self.scrape_with_browserless_advanced_with_proxy(url, custom_wait, None)
            .await
    }

    /// Advanced native-CDP scraping with optional proxy support.
    /// This is the v2.3.0+ replacement for the legacy HTTP Browserless path.
    pub async fn scrape_with_browserless_advanced_with_proxy(
        &self,
        url: &str,
        _custom_wait: Option<u32>,
        proxy_url: Option<String>,
    ) -> Result<ScrapeResponse> {
        info!(
            "Scraping with native CDP: {}{}",
            url,
            if proxy_url.is_some() {
                " [PROXY MODE]"
            } else {
                ""
            }
        );

        // Use the full stealth CDP path (scroll simulation, mouse, stealth injection)
        let (html, _status) = self.fetch_via_cdp(url, proxy_url).await?;
        let mut result = self.process_html(&html, url).await?;
        result.warnings.push("native_cdp_rendered".to_string());
        Ok(result)
    }
}

impl RustScraper {
    pub(super) fn append_image_context_markdown(
        &self,
        clean_content: String,
        images: &[Image],
        page_title: &str,
    ) -> String {
        let max_images = std::env::var("MAX_PREVIEW_IMAGE_HINTS")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(3)
            .max(1);

        let mut seen = HashSet::new();
        let mut image_lines = Vec::new();

        for image in images.iter() {
            if image.src.trim().is_empty() {
                continue;
            }
            if !seen.insert(image.src.clone()) {
                continue;
            }

            let mut label = image.alt.trim().to_string();
            if label.is_empty() {
                label = image.title.trim().to_string();
            }
            if label.is_empty() {
                label = if page_title.trim().is_empty() {
                    "image".to_string()
                } else {
                    page_title.trim().to_string()
                };
            }

            image_lines.push(format!("![{}]({})", label, image.src));
            if image_lines.len() >= max_images {
                break;
            }
        }

        if image_lines.is_empty() {
            return clean_content;
        }

        let mut out = clean_content;
        out.push_str("\n\n### Image Context\n");
        out.push_str(&image_lines.join("\n"));
        out
    }
}

impl Default for RustScraper {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rust_scraper() {
        let scraper = RustScraper::new();

        // Test with a simple HTML page
        match scraper.scrape_url("https://httpbin.org/html").await {
            Ok(content) => {
                assert!(!content.title.is_empty(), "Title should not be empty");
                assert!(
                    !content.clean_content.is_empty(),
                    "Content should not be empty"
                );
                assert_eq!(content.status_code, 200, "Status code should be 200");
                assert!(
                    content.word_count > 0,
                    "Word count should be greater than 0"
                );
            }
            Err(e) => {
                tracing::warn!("Rust scraper test failed: {}", e);
            }
        }
    }

    #[test]
    fn test_clean_text() {
        let scraper = RustScraper::new();
        let text = "  This   is    \n\n\n   some    text   \n\n  ";
        let cleaned = scraper.clean_text(text);
        assert_eq!(cleaned, "This is some text");
    }

    #[test]
    fn test_word_count() {
        let scraper = RustScraper::new();
        let text = "This is a test with five words";
        assert_eq!(scraper.count_words(text), 7);
    }
}
