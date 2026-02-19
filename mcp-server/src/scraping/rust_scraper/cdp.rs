use super::RustScraper;
use crate::scraping::browser_manager;
use crate::types::ScrapeResponse;
use anyhow::{anyhow, Result};
use chrono::Utc;
use futures::StreamExt;
use scraper::Html;
use std::time::Duration;
use tracing::{error, warn};
use url::Url;

impl RustScraper {
    /// 🚀 Direct CDP Stealth Control (universal)
    pub async fn fetch_via_cdp(
        &self,
        url: &str,
        proxy_url: Option<String>,
    ) -> Result<(String, u16)> {
        let exe = browser_manager::find_chrome_executable().ok_or_else(|| {
            anyhow!("No browser found for CDP stealth mode. Install Brave, Chrome, or Chromium.")
        })?;

        warn!("🚀 Direct CDP Stealth Mode: {} (browser: {})", url, exe);

        let config =
            browser_manager::build_headless_config(&exe, proxy_url.as_deref(), 1920, 1080)?;

        let (mut browser, mut handler) = chromiumoxide::Browser::launch(config)
            .await
            .map_err(|e| anyhow!("Failed to launch browser ({}): {}", exe, e))?;

        let handle = tokio::spawn(async move {
            while let Some(event) = handler.next().await {
                if let Err(e) = event {
                    error!("CDP handler error: {}", e);
                }
            }
        });

        let page = browser
            .new_page("about:blank")
            .await
            .map_err(|e| anyhow!("Failed to create page: {}", e))?;

        warn!("💉 Injecting Universal Stealth Engine (site-agnostic)");
        let stealth_script = self.get_universal_stealth_script();
        page.execute(
            chromiumoxide::cdp::browser_protocol::page::AddScriptToEvaluateOnNewDocumentParams::new(
                stealth_script,
            ),
        )
        .await
        .map_err(|e| anyhow!("Failed to inject stealth script: {}", e))?;

        warn!("🌐 Navigating to: {}", url);
        page.goto(url)
            .await
            .map_err(|e| anyhow!("Failed to navigate: {}", e))?;

        use rand::distr::{Distribution, Uniform};
        let idle_time = {
            let mut rng = rand::rng();
            let dist = Uniform::new(1000u64, 3000).unwrap();
            dist.sample(&mut rng)
        };

        warn!(
            "⏳ Initial idle time: {}ms (simulating human reading)",
            idle_time
        );
        tokio::time::sleep(Duration::from_millis(idle_time)).await;

        let scroll_actions: Vec<(u16, u64, bool, u16)> = {
            let mut rng = rand::rng();
            let pass_dist = Uniform::new(2usize, 6).unwrap();
            let scroll_dist = Uniform::new(200u16, 700).unwrap();
            let pause_dist = Uniform::new(300u64, 1500).unwrap();
            let scroll_up_dist = Uniform::new(50u16, 200).unwrap();
            let chance_dist = Uniform::new(0u8, 5).unwrap();

            let scroll_passes = pass_dist.sample(&mut rng);
            (0..scroll_passes)
                .map(|_| {
                    let scroll_distance = scroll_dist.sample(&mut rng);
                    let read_pause = pause_dist.sample(&mut rng);
                    let should_scroll_up = chance_dist.sample(&mut rng) == 0;
                    let scroll_up = scroll_up_dist.sample(&mut rng);
                    (scroll_distance, read_pause, should_scroll_up, scroll_up)
                })
                .collect()
        };

        warn!(
            "📜 Performing {} randomized scroll passes",
            scroll_actions.len()
        );

        for (scroll_distance, read_pause, should_scroll_up, scroll_up) in scroll_actions {
            if let Err(e) = page
                .evaluate(format!(
                    "window.scrollBy({{top: {}, behavior: 'smooth'}});",
                    scroll_distance
                ))
                .await
            {
                warn!("Scroll simulation error: {}", e);
            }

            tokio::time::sleep(Duration::from_millis(read_pause)).await;

            if should_scroll_up {
                if let Err(e) = page
                    .evaluate(format!(
                        "window.scrollBy({{top: -{}, behavior: 'smooth'}});",
                        scroll_up
                    ))
                    .await
                {
                    warn!("Scroll-up simulation error: {}", e);
                }
                tokio::time::sleep(Duration::from_millis(200 + (scroll_up as u64 % 300))).await;
            }
        }

        self.simulate_mouse_movement(&page).await?;

        let final_wait = {
            let mut rng = rand::rng();
            let dist = Uniform::new(500u64, 1500).unwrap();
            dist.sample(&mut rng)
        };
        tokio::time::sleep(Duration::from_millis(final_wait)).await;

        // Smart dynamic hydration: wait for network to settle, then auto-scroll
        // to trigger lazy-loaded content before capturing HTML.
        browser_manager::wait_until_stable(&page, 1500, 8000)
            .await
            .ok();
        browser_manager::auto_scroll(&page).await.ok();

        // 🧬 Visual Noise Filter (NeuroSiphon DNA)
        // Remove DOM elements that are visually invisible or known noise before capturing HTML.
        // This strips 20-30% of token waste: cookie banners, off-screen trackers, hidden divs.
            if crate::core::config::neurosiphon_enabled() {
                let noise_filter_script = Self::visual_noise_filter_script();
                if let Err(e) = page.evaluate(noise_filter_script).await {
                    // Non-fatal: some pages block eval or are cross-origin restricted
                    warn!("⚠️ Visual noise filter script failed (non-fatal): {}", e);
                }
            }

        let content = page
            .content()
            .await
            .map_err(|e| anyhow!("Failed to get page content: {}", e))?;

        if self.detect_challenge(&content) {
            warn!("❌ CDP fetch hit challenge iframe/content signature");

            drop(page);
            browser.close().await.ok();
            handle.abort();

            return Err(anyhow!("CDP bypass failed: Challenge detected"));
        }

        if let Some(block_reason) = self.detect_block_reason(&content) {
            warn!("❌ CDP fetch still blocked: {}", block_reason);

            drop(page);
            browser.close().await.ok();
            handle.abort();

            return Err(anyhow!("CDP bypass failed: {}", block_reason));
        }

        warn!("✅ CDP fetch successful ({} chars)", content.len());

        drop(page);
        browser.close().await.ok();
        handle.abort();

        Ok((content, 200))
    }

    async fn simulate_mouse_movement(&self, page: &chromiumoxide::Page) -> Result<()> {
        use rand::distr::{Distribution, Uniform};

        let moves: Vec<(i32, i32, u64)> = {
            let mut rng = rand::rng();
            let x_dist = Uniform::new(100, 800).unwrap();
            let y_dist = Uniform::new(100, 600).unwrap();
            let delay_dist = Uniform::new(0, 200).unwrap();

            (0..5)
                .map(|_| {
                    (
                        x_dist.sample(&mut rng),
                        y_dist.sample(&mut rng),
                        100 + delay_dist.sample(&mut rng),
                    )
                })
                .collect()
        };

        for (x, y, delay) in moves {
            if let Err(e) = page
                .evaluate(format!("document.elementFromPoint({}, {})", x, y))
                .await
            {
                warn!("Mouse simulation error: {}", e);
            }

            tokio::time::sleep(Duration::from_millis(delay)).await;
        }

        Ok(())
    }

    /// Process raw HTML into ScrapeResponse (for CDP-fetched content)
    pub async fn process_html(&self, html: &str, url: &str) -> Result<ScrapeResponse> {
        let parsed_url = Url::parse(url).map_err(|e| anyhow!("Invalid URL '{}': {}", url, e))?;
        let document = Html::parse_document(html);

        let title = self.extract_title(&document);
        let meta_description = self.extract_meta_description(&document);
        let meta_keywords = self.extract_meta_keywords(&document);
        let language = self.detect_language(&document, html);
        let canonical_url = self.extract_canonical(&document, &parsed_url);
        let site_name = self.extract_site_name(&document);
        let (og_title, og_description, og_image) = self.extract_open_graph(&document, &parsed_url);
        let author = self.extract_author(&document);
        let published_at = self.extract_published_time(&document);

        let code_blocks = {
            // 🧬 Rule B: infer language from URL extension for raw source files
            let url_lang_hint = RustScraper::infer_language_from_url(&parsed_url);
            let is_tutorial = RustScraper::is_tutorial_url(&parsed_url);
            self.extract_code_blocks(&document, url_lang_hint.as_deref(), is_tutorial)
        };

        // JSON-LD can be the cleanest source on modern sites; prefer it when present.
        let json_ld_content = self.extract_json_ld(&document);

        // ── 🧬 SPA fast-path (before JSON-LD): prefer embedded state blobs when present.
        // 🧬 Rule C: only commit to the SPA JSON when it yields readable content (≥ 100 words)
        // or `extract_app_state` is explicitly set on the scraper instance.
        let spa_state_content = if crate::core::config::neurosiphon_enabled()
            && crate::scraping::rust_scraper::clean::looks_like_spa(html)
        {
            self.extract_spa_json_state(html)
                .filter(|extracted| self.extract_app_state || self.count_words(extracted) >= 100)
        } else {
            None
        };

        let (mut clean_content, noise_reduction_ratio) =
            if let Some(spa_content) = spa_state_content.as_ref() {
                (self.normalize_markdown_fragments(spa_content), 0.0)
            } else if let Some(json_content) = json_ld_content.as_ref() {
                (
                    self.normalize_markdown_fragments(&html2md::parse_html(json_content)),
                    0.0,
                )
            } else {
                self.extract_clean_content_with_metrics(html, &parsed_url)
            };
        clean_content = self.normalize_markdown_fragments(&clean_content);
        clean_content = self.apply_og_description_fallback(clean_content, &og_description);
        clean_content = self.clean_noise(&clean_content);

        let headings = self.extract_headings(&document);
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

        let extraction_score =
            self.calculate_extraction_score(word_count, &published_at, &code_blocks, &headings);

        let domain = parsed_url.host_str().map(|h| h.to_string());
        Ok(ScrapeResponse {
            url: url.to_string(),
            title,
            content: html.to_string(),
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
            status_code: 200,
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
            reading_time_minutes,
            code_blocks,
            truncated: false,
            actual_chars: 0,
            max_chars_limit: None,
            extraction_score: Some(extraction_score),
            warnings,
            domain,
        })
    }

    // ─────────────────────────────────────────────────────────────────────────
    // 🧬 Visual Noise Filter — NeuroSiphon DNA Transfer
    // CDP-side JavaScript that prunes invisible / off-screen / cookie-banner
    // elements directly in the live DOM before we snapshot the HTML.
    // Estimated token saving: 20-30% on typical pages.
    // ─────────────────────────────────────────────────────────────────────────
    fn visual_noise_filter_script() -> &'static str {
        r#"
(function shadowcrawlNoiseFilter() {
    'use strict';

    var viewportW = window.innerWidth  || 1920;
    var viewportH = window.innerHeight || 1080;
    var removed   = 0;

    // Noise class / id fragments (case-insensitive substring match)
    var NOISE_PATTERNS = [
        'cookie', 'consent', 'gdpr', 'ccpa', 'privacy-banner', 'cookie-notice',
        'subscribe-modal', 'newsletter-popup', 'chat-widget', 'livechat',
        'intercom', 'drift-frame', 'hubspot-messages',
        'ad-unit', 'adsbygoogle', 'taboola', 'outbrain', 'mgid',
        'sticky-footer-ad', 'sticky-header-ad',
        'overlay-modal', 'permission-modal',
    ];

    function matchesNoise(el) {
        var id  = (el.id  || '').toLowerCase();
        var cls = (el.className && typeof el.className === 'string'
                   ? el.className : '').toLowerCase();
        for (var i = 0; i < NOISE_PATTERNS.length; i++) {
            if (id.indexOf(NOISE_PATTERNS[i]) !== -1 || cls.indexOf(NOISE_PATTERNS[i]) !== -1) {
                return true;
            }
        }
        return false;
    }

    function shouldRemove(el) {
        if (!el || el.nodeType !== 1) return false;

        // 1) Known-noise class/id patterns
        if (matchesNoise(el)) return true;

        var style = window.getComputedStyle(el);

        // 2) Completely invisible via CSS
        if (style.display === 'none') return true;
        if (style.visibility === 'hidden') return true;
        if (parseFloat(style.opacity) === 0) return true;

        // 3) 1x1 pixel trackers
        var rect = el.getBoundingClientRect();
        if (rect.width <= 1 && rect.height <= 1) return true;

        // 4) Fixed/sticky overlays covering < 10% of viewport area
        //    (typically cookie banners, chat bubbles, etc.)
        if (style.position === 'fixed' || style.position === 'sticky') {
            var area       = rect.width * rect.height;
            var viewArea   = viewportW * viewportH;
            var coverRatio = viewArea > 0 ? area / viewArea : 0;
            if (coverRatio < 0.10) return true;
        }

        // 5) Off-screen elements (more than 2 viewport-heights above/below)
        if (rect.bottom < -viewportH * 2 || rect.top > viewportH * 3) return true;

        return false;
    }

    // Walk all elements (snapshot the list first to avoid live-collection issues)
    var allEls = Array.prototype.slice.call(document.querySelectorAll(
        'div,section,aside,header,footer,nav,dialog,aside,figure,ins,iframe'
    ));

    for (var i = 0; i < allEls.length; i++) {
        var el = allEls[i];
        if (el.parentNode && shouldRemove(el)) {
            el.parentNode.removeChild(el);
            removed++;
        }
    }

    // Report how many nodes were pruned (visible in DevTools console)
    console.debug('[ShadowCrawl] Visual noise filter removed ' + removed + ' elements');
    return removed;
})();
"#
    }}