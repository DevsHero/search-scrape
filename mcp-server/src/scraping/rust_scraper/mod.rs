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
use tracing::{info, warn};
use url::Url;

/// Enhanced Rust-native web scraper with anti-bot protection
pub struct RustScraper {
    client: Client,
    quality_mode: QualityMode,
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
        }
    }

    pub(super) fn is_aggressive_mode(&self) -> bool {
        matches!(self.quality_mode, QualityMode::Aggressive)
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
        let code_blocks = self.extract_code_blocks(&document);

        // JSON-LD can be the cleanest source on modern sites; prefer it when present.
        let json_ld_content = self.extract_json_ld(&document);

        // Extract readable content using readability (fallback)
        let (mut clean_content, noise_reduction_ratio) =
            if let Some(json_content) = json_ld_content.as_ref() {
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

    /// Scrape a URL using Browserless headless browser for JS-heavy sites
    /// This is used as a fallback when static scraping returns poor quality
    /// Supports custom actions (scroll, wait) for dynamic content
    pub async fn scrape_with_browserless(&self, url: &str) -> Result<ScrapeResponse> {
        self.scrape_with_browserless_advanced(url, None).await
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

    /// Advanced Browserless scraping with optional proxy support
    pub async fn scrape_with_browserless_advanced_with_proxy(
        &self,
        url: &str,
        custom_wait: Option<u32>,
        proxy_url: Option<String>,
    ) -> Result<ScrapeResponse> {
        info!(
            "Scraping URL with Advanced Browserless: {}{}",
            url,
            if proxy_url.is_some() {
                " [PROXY MODE]"
            } else {
                ""
            }
        );

        let browserless_url = std::env::var("BROWSERLESS_URL")
            .unwrap_or_else(|_| "http://localhost:3000".to_string());
        let browserless_token = std::env::var("BROWSERLESS_TOKEN").ok();

        // Domain detection for smart configuration
        let domain = url::Url::parse(url)
            .ok()
            .and_then(|u| u.host_str().map(|s| s.to_lowercase()));

        let (wait_time, needs_scroll) = self.detect_domain_strategy(&domain);
        let final_wait = custom_wait.unwrap_or(wait_time);
        let is_boss_domain = self.is_boss_domain(&domain);

        // UNIVERSAL APPROACH: No site-specific checks - CDP handles all complex cases
        info!(
            "Domain strategy: wait={}ms, scroll={}",
            final_wait, needs_scroll
        );

        // Apply anti-bot delay
        antibot::apply_request_delay().await;

        // Standard UA rotation for Browserless /content endpoint
        let (primary_user_agent, mobile_config) = (antibot::get_browserless_user_agent(), None);

        let extra_params = self.extra_browserless_query_params(&domain);

        // UNIVERSAL APPROACH: Always use /content endpoint (site-agnostic)
        // CDP handles complex cases, Browserless /content is fallback only
        let mut extended_wait = if needs_scroll {
            final_wait + 3000
        } else {
            final_wait
        };

        if is_boss_domain {
            let extra_wait = antibot::boss_domain_post_load_delay_ms() as u32;
            extended_wait = extended_wait.saturating_add(extra_wait);
            info!("Boss domain extra wait: {}ms", extra_wait);
        }

        let (mut html, mut status_code) = self
            .fetch_browserless_content(
                url,
                extended_wait,
                &browserless_url,
                browserless_token.clone(),
                is_boss_domain,
                primary_user_agent,
                mobile_config.as_ref(),
                &extra_params,
                proxy_url.clone(),
            )
            .await?;

        // UNIVERSAL RETRY STRATEGY: Content-based detection, not domain-specific
        if let Some(reason) = self.detect_block_reason(&html) {
            warn!("Pass 1 BLOCKED ({}): {}", reason, url);

            // Pass 2: Try Mobile Safari profile (universal fallback)
            info!("Pass 2: Trying Mobile Safari profile (universal)");
            let mobile = antibot::get_mobile_stealth_config();

            let retry2 = self
                .fetch_browserless_content(
                    url,
                    final_wait + 2000, // Even longer wait on retry
                    &browserless_url,
                    browserless_token.clone(),
                    is_boss_domain,
                    mobile.user_agent,
                    Some(&mobile),
                    &extra_params,
                    proxy_url.clone(),
                )
                .await;

            if let Ok((retry_html, retry_status)) = retry2 {
                if let Some(reason2) = self.detect_block_reason(&retry_html) {
                    warn!(
                        "Pass 2 BLOCKED ({}): {}. Word count: {}",
                        reason2,
                        url,
                        self.count_words(&retry_html)
                    );

                    // Pass 3: Try native scraper as last resort (universal fallback)
                    warn!("Pass 3: Falling back to native scraper (universal)");
                    match self.scrape_url(url).await {
                        Ok(native_response)
                            if native_response.word_count > self.count_words(&retry_html) =>
                        {
                            info!("Pass 3 SUCCESS: Native scraper got {} words vs {} from Browserless",
                                native_response.word_count, self.count_words(&retry_html));
                            return Ok(native_response);
                        }
                        Ok(_) | Err(_) => {
                            warn!("Pass 3 FAILED: Native scraper didn't improve results, using Pass 2");
                        }
                    }

                    html = retry_html;
                    status_code = retry_status;
                } else {
                    info!("Pass 2 SUCCESS: Block cleared");
                    html = retry_html;
                    status_code = retry_status;
                }
            } else {
                warn!("Pass 2 FAILED: Network error, using Pass 1 result");
            }
        }

        info!("Browserless rendered {} bytes of HTML", html.len());

        // Parse the rendered HTML with our standard extraction logic
        let parsed_url = Url::parse(url).map_err(|e| anyhow!("Invalid URL: {}", e))?;
        let document = Html::parse_document(&html);

        // NEW: Extract JSON-LD structured data (Schema.org)
        let json_ld_content = self.extract_json_ld(&document);

        // Extract all content using existing methods
        let title = self.extract_title(&document);
        let meta_description = self.extract_meta_description(&document);
        let meta_keywords = self.extract_meta_keywords(&document);
        let (og_title, og_description, og_image) = self.extract_open_graph(&document, &parsed_url);

        // Priority order: JSON-LD > Normal extraction (universal)
        let (mut clean_content, noise_reduction_ratio) = if let Some(json_content) = json_ld_content
        {
            info!("Using JSON-LD extraction (universal structured data)");
            (
                self.normalize_markdown_fragments(&html2md::parse_html(&json_content)),
                0.0,
            )
        } else {
            self.extract_clean_content_with_metrics(&html, &parsed_url)
        };

        clean_content = self.normalize_markdown_fragments(&clean_content);
        clean_content = self.apply_og_description_fallback(clean_content, &og_description);
        clean_content = self.clean_noise(&clean_content);
        let language = self.detect_language(&document, &html);
        let canonical_url = self.extract_canonical(&document, &parsed_url);
        let site_name = self.extract_site_name(&document);
        let author = self.extract_author(&document);
        let published_at = self.extract_published_time(&document);
        let code_blocks = self.extract_code_blocks(&document);
        let headings = self.extract_headings(&document);
        let links = self.extract_content_links(&document, &parsed_url);
        let images = self.extract_images(&document, &parsed_url);

        let mut embedded_data_sources = self.collect_embedded_data_sources(&document);
        let mut embedded_state_json = embedded_data_sources
            .iter()
            .max_by_key(|s| s.content.len())
            .map(|s| s.content.clone())
            .or_else(|| self.extract_embedded_state_json(&document));

        clean_content = self.append_image_context_markdown(clean_content, &images, &title);
        let word_count = self.count_words(&clean_content);
        let reading_time_minutes = (word_count as f64 / 200.0).ceil() as usize;

        let extraction_score =
            self.calculate_extraction_score(word_count, &published_at, &code_blocks, &headings);

        let domain = parsed_url.host_str().map(|h| h.to_string());
        let mut warnings = vec!["browserless_rendered".to_string()];
        if let Some(reason) = self.detect_block_reason(&html) {
            warnings.push(format!("browserless_blocked: {}", reason));
        }
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

        let result = ScrapeResponse {
            url: url.to_string(),
            title,
            content: html.clone(),
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
            content_type: "text/html".to_string(),
            word_count,
            language,
            canonical_url,
            site_name,
            author,
            published_at,
            og_title,
            og_description,
            og_image,
            reading_time_minutes: Some(reading_time_minutes as u32),
            code_blocks,
            truncated: false,
            actual_chars: 0,
            max_chars_limit: None,
            extraction_score: Some(extraction_score),
            warnings,
            domain,
        };

        info!(
            "Browserless scrape successful: {} ({} words, score: {:.2})",
            result.title, result.word_count, extraction_score
        );
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
