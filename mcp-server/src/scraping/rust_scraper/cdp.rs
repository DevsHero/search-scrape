use super::RustScraper;
use crate::types::ScrapeResponse;
use anyhow::{anyhow, Result};
use chrono::Utc;
use chromiumoxide::browser::BrowserConfig;
use chromiumoxide::handler::viewport::Viewport;
use chromiumoxide::Browser;
use futures::StreamExt;
use scraper::Html;
use std::time::Duration;
use tracing::{error, warn};
use url::Url;

impl RustScraper {
    /// 🚀 Direct CDP Stealth Control (universal)
    pub async fn fetch_via_cdp(&self, url: &str, proxy_url: Option<String>) -> Result<(String, u16)> {
        warn!("🚀 Using Direct CDP Stealth Mode for: {}", url);

        let browserless_url = std::env::var("BROWSERLESS_URL")
            .unwrap_or_else(|_| "http://localhost:3000".to_string());
        let ws_url = browserless_url
            .replace("http://", "ws://")
            .replace("https://", "wss://");

        warn!("🔌 Connecting to CDP endpoint: {}", ws_url);

        let mut config = BrowserConfig::builder()
            .chrome_executable("/usr/bin/chromium")
            .viewport(Viewport {
                width: 1920,
                height: 1080,
                device_scale_factor: Some(1.0),
                emulating_mobile: false,
                is_landscape: false,
                has_touch: false,
            })
            .window_size(1920, 1080);

        if let Some(proxy) = proxy_url {
            config = config.arg(format!("--proxy-server={}", proxy));
        }

        let (mut browser, mut handler) =
            Browser::launch(config.build().map_err(|e| anyhow!("Failed to build browser config: {}", e))?)
                .await
                .map_err(|e| anyhow!("Failed to launch browser: {}", e))?;

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

        warn!("⏳ Initial idle time: {}ms (simulating human reading)", idle_time);
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

        warn!("📜 Performing {} randomized scroll passes", scroll_actions.len());

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
            if let Err(e) = page.evaluate(format!("document.elementFromPoint({}, {})", x, y)).await {
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

        let code_blocks = self.extract_code_blocks(&document);

        let mut clean_content = self.extract_clean_content(html, &parsed_url);
        clean_content = self.normalize_markdown_fragments(&clean_content);
        clean_content = self.apply_og_description_fallback(clean_content, &og_description);

        let headings = self.extract_headings(&document);
        let links = self.extract_content_links(&document, &parsed_url);
        let images = self.extract_images(&document, &parsed_url);

        clean_content = self.append_image_context_markdown(clean_content, &images, &title);
        let word_count = self.count_words(&clean_content);
        let reading_time_minutes = Some(((word_count as f64 / 200.0).ceil() as u32).max(1));

        let extraction_score =
            self.calculate_extraction_score(word_count, &published_at, &code_blocks, &headings);

        let domain = parsed_url.host_str().map(|h| h.to_string());
        let warnings = Vec::new();

        Ok(ScrapeResponse {
            url: url.to_string(),
            title,
            content: html.to_string(),
            clean_content,
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
}
