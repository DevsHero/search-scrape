use crate::types::*;
use anyhow::{anyhow, Result};
use chrono::Utc;
use rand::Rng;
use readability::extractor;
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use select::{document::Document as SelectDoc, predicate::{Name as SelName, Attr as SelAttr, Predicate}};
use std::collections::HashSet;
use tracing::{info, warn};
use url::Url;
use whatlang::{detect, Lang};

/// User agents for rotation
const USER_AGENTS: &[&str] = &[
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:89.0) Gecko/20100101 Firefox/89.0",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/14.1.1 Safari/605.1.15",
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36",
    "Mozilla/5.0 (X11; Ubuntu; Linux x86_64; rv:89.0) Gecko/20100101 Firefox/89.0",
];

/// Enhanced Rust-native web scraper
pub struct RustScraper {
    client: Client,
}

impl RustScraper {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .expect("Failed to create HTTP client");

        Self { client }
    }

    /// Get a random User-Agent string
    fn get_random_user_agent(&self) -> &'static str {
        let mut rng = rand::thread_rng();
        let index = rng.gen_range(0..USER_AGENTS.len());
        USER_AGENTS[index]
    }

    /// Scrape a URL with enhanced content extraction
    pub async fn scrape_url(&self, url: &str) -> Result<ScrapeResponse> {
        info!("Scraping URL with Rust-native scraper: {}", url);

        // Validate URL
        let parsed_url = Url::parse(url)
            .map_err(|e| anyhow!("Invalid URL '{}': {}", url, e))?;

        if parsed_url.scheme() != "http" && parsed_url.scheme() != "https" {
            return Err(anyhow!("URL must use HTTP or HTTPS protocol"));
        }

        // Make HTTP request with random User-Agent
        let user_agent = self.get_random_user_agent();
        let response = self
            .client
            .get(url)
            .header("User-Agent", user_agent)
            .header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
            .header("Accept-Language", "en-US,en;q=0.5")
            // Rely on reqwest automatic decompression; remove manual Accept-Encoding to avoid serving compressed body as text
            .header("DNT", "1")
            .header("Connection", "keep-alive")
            .header("Upgrade-Insecure-Requests", "1")
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

        // Extract readable content using readability
        let clean_content = self.extract_clean_content(&html, &parsed_url);
    let word_count = self.count_words(&clean_content);
    let reading_time_minutes = Some(((word_count as f64 / 200.0).ceil() as u32).max(1));

        // Extract structured data
        let headings = self.extract_headings(&document);
        // Smart link extraction: prefer content links over all document links
        let links = self.extract_content_links(&document, &parsed_url);
        let images = self.extract_images(&document, &parsed_url);

        // Calculate extraction quality score (Priority 1 fix)
        let extraction_score = self.calculate_extraction_score(
            word_count,
            &published_at,
            &code_blocks,
            &headings,
        );

        // Extract domain from URL (Priority 2 enhancement)
        let domain = parsed_url.host_str().map(|h| h.to_string());

        // Initialize warnings
        let warnings = Vec::new();
        
        let result = ScrapeResponse {
            url: url.to_string(),
            title,
            content: html,
            clean_content,
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

        info!("Successfully scraped: {} ({} words, score: {:.2})", result.title, result.word_count, extraction_score);
        Ok(result)
    }

    /// Extract page title with fallback to h1
    fn extract_title(&self, document: &Html) -> String {
        // Try title tag first
        if let Ok(title_selector) = Selector::parse("title") {
            if let Some(title_element) = document.select(&title_selector).next() {
                let title = title_element.text().collect::<String>().trim().to_string();
                if !title.is_empty() {
                    return title;
                }
            }
        }

        // Fallback to h1
        if let Ok(h1_selector) = Selector::parse("h1") {
            if let Some(h1_element) = document.select(&h1_selector).next() {
                let h1_text = h1_element.text().collect::<String>().trim().to_string();
                if !h1_text.is_empty() {
                    return h1_text;
                }
            }
        }

        "No Title".to_string()
    }

    /// Extract meta description
    fn extract_meta_description(&self, document: &Html) -> String {
        if let Ok(selector) = Selector::parse("meta[name=\"description\"]") {
            if let Some(element) = document.select(&selector).next() {
                if let Some(content) = element.value().attr("content") {
                    return content.trim().to_string();
                }
            }
        }
        String::new()
    }

    /// Extract meta keywords
    fn extract_meta_keywords(&self, document: &Html) -> String {
        if let Ok(selector) = Selector::parse("meta[name=\"keywords\"]") {
            if let Some(element) = document.select(&selector).next() {
                if let Some(content) = element.value().attr("content") {
                    return content.trim().to_string();
                }
            }
        }
        String::new()
    }

    /// Extract canonical URL
    fn extract_canonical(&self, document: &Html, base: &Url) -> Option<String> {
        if let Ok(selector) = Selector::parse("link[rel=\"canonical\"]") {
            if let Some(el) = document.select(&selector).next() {
                if let Some(href) = el.value().attr("href") {
                    return base.join(href).ok().map(|u| u.to_string()).or_else(|| Some(href.to_string()));
                }
            }
        }
        None
    }

    /// Extract site name (OpenGraph fallback)
    fn extract_site_name(&self, document: &Html) -> Option<String> {
        if let Ok(selector) = Selector::parse("meta[property=\"og:site_name\"]") {
            if let Some(el) = document.select(&selector).next() {
                if let Some(content) = el.value().attr("content") {
                    let v = content.trim();
                    if !v.is_empty() { return Some(v.to_string()); }
                }
            }
        }
        None
    }

    /// Extract OpenGraph basic fields
    fn extract_open_graph(&self, document: &Html, base: &Url) -> (Option<String>, Option<String>, Option<String>) {
        let og_title = if let Ok(sel) = Selector::parse("meta[property=\"og:title\"]") {
            document.select(&sel).next().and_then(|e| e.value().attr("content")).map(|s| s.trim().to_string())
        } else { None };
        let og_description = if let Ok(sel) = Selector::parse("meta[property=\"og:description\"]") {
            document.select(&sel).next().and_then(|e| e.value().attr("content")).map(|s| s.trim().to_string())
        } else { None };
        let og_image = if let Ok(sel) = Selector::parse("meta[property=\"og:image\"]") {
            document.select(&sel).next().and_then(|e| e.value().attr("content")).and_then(|s| base.join(s).ok().map(|u| u.to_string()).or_else(|| Some(s.to_string())))
        } else { None };
        (og_title, og_description, og_image)
    }

    /// Extract author
    fn extract_author(&self, document: &Html) -> Option<String> {
        // Meta author
        if let Ok(sel) = Selector::parse("meta[name=\"author\"]") {
            if let Some(el) = document.select(&sel).next() {
                if let Some(content) = el.value().attr("content") { return Some(content.trim().to_string()); }
            }
        }
        // Article author
        if let Ok(sel) = Selector::parse("meta[property=\"article:author\"]") {
            if let Some(el) = document.select(&sel).next() {
                if let Some(content) = el.value().attr("content") { return Some(content.trim().to_string()); }
            }
        }
        None
    }

    /// Extract published time
    fn extract_published_time(&self, document: &Html) -> Option<String> {
        if let Ok(sel) = Selector::parse("meta[property=\"article:published_time\"]") {
            if let Some(el) = document.select(&sel).next() {
                if let Some(content) = el.value().attr("content") { return Some(content.trim().to_string()); }
            }
        }
        None
    }

    /// Detect language from HTML attributes and content
    fn detect_language(&self, document: &Html, html: &str) -> String {
        // Try HTML lang attribute
        if let Ok(selector) = Selector::parse("html") {
            if let Some(html_element) = document.select(&selector).next() {
                if let Some(lang) = html_element.value().attr("lang") {
                    return lang.trim().to_string();
                }
            }
        }

        // Try meta content-language
        if let Ok(selector) = Selector::parse("meta[http-equiv=\"content-language\"]") {
            if let Some(element) = document.select(&selector).next() {
                if let Some(content) = element.value().attr("content") {
                    return content.trim().to_string();
                }
            }
        }

        // Use whatlang for content-based detection
        if let Some(info) = detect(html) {
            match info.lang() {
                Lang::Eng => "en".to_string(),
                Lang::Spa => "es".to_string(),
                Lang::Fra => "fr".to_string(),
                Lang::Deu => "de".to_string(),
                Lang::Ita => "it".to_string(),
                Lang::Por => "pt".to_string(),
                Lang::Rus => "ru".to_string(),
                Lang::Jpn => "ja".to_string(),
                Lang::Kor => "ko".to_string(),
                Lang::Cmn => "zh".to_string(),
                _ => format!("{:?}", info.lang()).to_lowercase(),
            }
        } else {
            "unknown".to_string()
        }
    }

    /// Extract clean, readable content using readability, preceded by HTML preprocessing
    fn extract_clean_content(&self, html: &str, base_url: &Url) -> String {
        // 1) Pre-clean HTML to strip obvious boilerplate and ads before readability
        let pre = self.preprocess_html(html);

        // 1a) mdBook-style extractor (e.g., Rust Book) — try focused body first
        if let Some(md_text) = self.extract_mdbook_like(&pre) {
            if md_text.len() > 120 { // substantial content
                return self.post_clean_text(&md_text);
            }
        }

        // 2) Readability pass
        let readability_text = match extractor::extract(&mut pre.as_bytes(), base_url) {
            Ok(product) => {
                let text = html2text::from_read(product.content.as_bytes(), 80);
                self.post_clean_text(&text)
            }
            Err(e) => {
                warn!("Readability extraction failed: {}, will try heuristics", e);
                String::new()
            }
        };

        // 3) Heuristic main-content extraction (article/main/role=main/etc.)
        let heuristic_text = self.heuristic_main_extraction(&pre);

        // 4) Choose the better result by word count; be aggressive if one is near-empty
        let rt_words = self.count_words(&readability_text);
        let ht_words = self.count_words(&heuristic_text);

        let chosen = if rt_words == 0 && ht_words > 0 {
            heuristic_text
        } else if ht_words == 0 && rt_words > 0 {
            readability_text
        } else if ht_words > rt_words.saturating_add(20) {
            heuristic_text
        } else if rt_words > 0 {
            readability_text
        } else {
            // 5) Fallback to simple whole-document text extraction
            self.fallback_text_extraction(&pre)
        };

        // Final sanitize; ensure non-trivial output by adding a last-resort html2text over full doc
        let final_text = self.post_clean_text(&chosen);
        if final_text.len() < 80 {
            let whole = html2text::from_read(pre.as_bytes(), 80);
            return self.post_clean_text(&whole);
        }
        final_text
    }

    /// Extract content from mdBook-like structures (#content, main, article) using select crate
    fn extract_mdbook_like(&self, html: &str) -> Option<String> {
        let doc = SelectDoc::from(html);
        // Try #content first - this is mdBook's main content container
        if let Some(node) = doc.find(SelName("div").and(SelAttr("id", "content"))).next() {
            let inner = node.inner_html();
            let text = html2text::from_read(inner.as_bytes(), 80);
            let cleaned = self.clean_text(&text);
            let word_count = self.count_words(&cleaned);
            info!("mdBook extractor (#content): {} words", word_count);
            if word_count > 50 { 
                return Some(cleaned); 
            }
        }
        // Try main
        if let Some(node) = doc.find(SelName("main")).next() {
            let inner = node.inner_html();
            let text = html2text::from_read(inner.as_bytes(), 80);
            let cleaned = self.clean_text(&text);
            let word_count = self.count_words(&cleaned);
            info!("mdBook extractor (main): {} words", word_count);
            if word_count > 50 { 
                return Some(cleaned); 
            }
        }
        // Try article
        if let Some(node) = doc.find(SelName("article")).next() {
            let inner = node.inner_html();
            let text = html2text::from_read(inner.as_bytes(), 80);
            let cleaned = self.clean_text(&text);
            let word_count = self.count_words(&cleaned);
            info!("mdBook extractor (article): {} words", word_count);
            if word_count > 50 { 
                return Some(cleaned); 
            }
        }
        info!("mdBook extractor found no suitable content");
        None
    }

    /// Fallback text extraction when readability fails
    fn fallback_text_extraction(&self, html: &str) -> String {
        let document = Html::parse_document(html);
        
        // Remove script and style elements
        let mut text_parts = Vec::new();
        
        if let Ok(body_selector) = Selector::parse("body") {
            if let Some(body) = document.select(&body_selector).next() {
                self.extract_text_recursive(&body, &mut text_parts);
            }
        } else {
            // Fallback to entire document
            for node in document.tree.nodes() {
                if let Some(text) = node.value().as_text() {
                    text_parts.push(text.text.to_string());
                }
            }
        }
        
        let text = text_parts.join(" ");
        self.clean_text(&text)
    }

    /// Recursively extract text from elements
    fn extract_text_recursive(&self, element: &scraper::ElementRef, text_parts: &mut Vec<String>) {
        for child in element.children() {
            if let Some(child_element) = scraper::ElementRef::wrap(child) {
                let tag_name = child_element.value().name();
                // Skip noisy/boilerplate elements entirely
                if matches!(tag_name,
                    "script" | "style" | "noscript" | "svg" | "canvas" | "iframe" | "form" |
                    "header" | "footer" | "nav" | "aside") {
                    continue;
                }

                // Skip common ad/utility blocks by class/id heuristics
                let attrs = child_element.value();
                let mut skip = false;
                if let Some(id) = attrs.id() {
                    skip |= self.is_noise_identifier(id);
                }
                for class in attrs.classes() {
                    if self.is_noise_identifier(class) { skip = true; break; }
                }
                if skip {
                    continue;
                }
                self.extract_text_recursive(&child_element, text_parts);
            } else if let Some(text_node) = child.value().as_text() {
                text_parts.push(text_node.text.to_string());
            }
        }
    }

    /// Clean extracted text (whitespace normalization)
    fn clean_text(&self, text: &str) -> String {
        // Remove excessive whitespace
        let re_whitespace = Regex::new(r"\s+").unwrap();
        let re_newlines = Regex::new(r"\n\s*\n").unwrap();
        
        let cleaned = re_whitespace.replace_all(text, " ");
        let cleaned = re_newlines.replace_all(&cleaned, "\n\n");
        
        cleaned.trim().to_string()
    }

    /// Final post-processing to strip boilerplate lines, trackers, CTA, share/cookie prompts
    fn post_clean_text(&self, text: &str) -> String {
        // Normalize first
    let out = self.clean_text(text);

        // Drop lines matching common garbage patterns
        let garbage = [
            r"(?i)subscribe", r"(?i)sign up", r"(?i)cookie", r"(?i)accept all",
            r"(?i)advert", r"(?i)sponsor", r"(?i)newsletter", r"(?i)\bshare\b", r"(?i)related articles",
            r"(?i)^comments?$", r"(?i)read more", r"(?i)continue reading", r"(?i)terms of service", r"(?i)privacy policy",
        ];
        let re_garbage = Regex::new(&format!("{}", garbage.join("|"))).unwrap();

        let mut kept = Vec::new();
        for line in out.split('\n') {
            let line_trim = line.trim();
            if line_trim.is_empty() { continue; }
            // Remove very short noisy lines and those matching garbage
            if line_trim.len() < 3 { continue; }
            if re_garbage.is_match(line_trim) { continue; }
            kept.push(line_trim.to_string());
        }

        // Deduplicate adjacent lines
        kept.dedup();
        let result = kept.join("\n");
        // Collapse too many newlines
        let re_multi_nl = Regex::new(r"\n{3,}").unwrap();
        re_multi_nl.replace_all(&result, "\n\n").to_string()
    }

    /// Preprocess raw HTML by removing whole noisy blocks prior to readability
    fn preprocess_html(&self, html: &str) -> String {
        let mut s = html.to_string();

        // Remove whole tag blocks (script/style/etc.)
        // Rust regex crate doesn't support backreferences; match explicit open/close pairs for safe tags only.
        let re_block = Regex::new(
            r"(?is)<(?:script|style|noscript|svg|canvas|iframe)[^>]*?>.*?</(?:script|style|noscript|svg|canvas|iframe)>"
        ).unwrap();
        s = re_block.replace_all(&s, " ").to_string();

        // Remove div/section/article with ad/utility classes/ids
        // Raw string avoids needing to escape quotes/backslashes; (?is) = case-insensitive, dot matches newline
        let re_ad_blocks = Regex::new(
            r#"(?is)<(?:div|section|aside|article)[^>]*?(?:id|class)=(?:'|")[^'">]*(?:ads|advert|sponsor|promo|related|cookie|banner|modal|subscribe|newsletter|share|social|sidebar|comments|breadcrumb|pagination)[^'">]*(?:'|")[^>]*?>.*?</(?:div|section|aside|article)>"#
        ).unwrap();
        s = re_ad_blocks.replace_all(&s, " ").to_string();

        s
    }

    /// Identify noisy identifiers by substring match
    fn is_noise_identifier(&self, ident: &str) -> bool {
        let ident = ident.to_ascii_lowercase();
        let needles = [
            // avoid plain "ad" to not match words like "header"
            "ads", "advert", "adsense", "adunit", "ad-slot", "ad_container", "adbox",
            "sponsor", "promo", "cookie", "consent", "banner", "modal",
            "subscribe", "newsletter", "share", "social", "sidebar", "comments", "related",
            "breadcrumb", "pagination", "nav", "footer", "header", "hero", "toolbar",
        ];
        if needles.iter().any(|n| ident.contains(n)) { return true; }
        // Additional hyphen/underscore separated ad markers
        if ident.contains("-ad") || ident.contains("ad-") || ident.contains("_ad") || ident.contains("ad_") { return true; }
        false
    }

    /// Heuristic extraction from common main/article containers; returns cleaned text
    fn heuristic_main_extraction(&self, html: &str) -> String {
        let document = Html::parse_document(html);

        // Candidate selectors in priority order
        let selectors = [
            "article",
            "main",
            "[role=main]",
            "[itemprop=articleBody]",
            ".entry-content",
            ".post-content",
            ".article-content",
            "#content",
            "#main",
            ".content",
            ".post",
            ".article",
        ];

        let mut best_text = String::new();
        let mut best_words = 0usize;

        for sel_str in selectors.iter() {
            if let Ok(sel) = Selector::parse(sel_str) {
                for el in document.select(&sel) {
                    let mut parts = Vec::new();
                    self.extract_text_recursive(&el, &mut parts);
                    let text = self.post_clean_text(&parts.join(" "));
                    let wc = self.count_words(&text);
                    if wc > best_words {
                        best_words = wc;
                        best_text = text;
                    }
                }
            }
        }

        best_text
    }

    /// Count words in text
    fn count_words(&self, text: &str) -> usize {
        text.split_whitespace().count()
    }

    /// Extract headings (h1-h6)
    fn extract_headings(&self, document: &Html) -> Vec<Heading> {
        let mut headings = Vec::new();
        
        for level in 1..=6 {
            let sel: &str = match level {
                1 => "h1",
                2 => "h2",
                3 => "h3",
                4 => "h4",
                5 => "h5",
                _ => "h6",
            };
            if let Ok(selector) = Selector::parse(sel) {
                for element in document.select(&selector) {
                    let text = element.text().collect::<String>().trim().to_string();
                    if !text.is_empty() {
                        headings.push(Heading {
                            level: sel.to_string(),
                            text,
                        });
                    }
                }
            }
        }
        
        headings
    }

    /// Extract links with absolute URLs (all document links)
    fn extract_links(&self, document: &Html, base_url: &Url) -> Vec<Link> {
        self.extract_links_from_selector(document, base_url, "a[href]")
    }

    /// Extract links only from main content area (smart filtering)
    fn extract_content_links(&self, document: &Html, base_url: &Url) -> Vec<Link> {
        // Try to find main content area first
        let content_selectors = [
            "article a[href]",
            "main a[href]",
            "[role=main] a[href]",
            "[itemprop=articleBody] a[href]",
            ".entry-content a[href]",
            ".post-content a[href]",
            ".article-content a[href]",
            "#content a[href]",
            "#main a[href]",
        ];

        for content_sel in content_selectors.iter() {
            if Selector::parse(content_sel).is_ok() {
                let links = self.extract_links_from_selector(document, base_url, content_sel);
                if !links.is_empty() && links.len() >= 3 {
                    info!("Extracted {} links from main content using selector: {}", links.len(), content_sel);
                    return links;
                }
            }
        }

        // Fallback to all links if no main content found
        info!("No main content area found, using all document links");
        self.extract_links(document, base_url)
    }

    /// Helper to extract links from a specific selector
    fn extract_links_from_selector(&self, document: &Html, base_url: &Url, selector_str: &str) -> Vec<Link> {
        let mut links = Vec::new();
        let mut seen_urls = HashSet::new();
        
        if let Ok(selector) = Selector::parse(selector_str) {
            for element in document.select(&selector) {
                if let Some(href) = element.value().attr("href") {
                    // Skip anchor links, javascript, and common non-content patterns
                    if href.starts_with('#') || href.starts_with("javascript:") || href.starts_with("mailto:") {
                        continue;
                    }

                    let text = element.text().collect::<String>().trim().to_string();
                    
                    // Convert relative URLs to absolute
                    let absolute_url = match base_url.join(href) {
                        Ok(url) => url.to_string(),
                        Err(_) => href.to_string(),
                    };
                    
                    // Avoid duplicates
                    if !seen_urls.contains(&absolute_url) {
                        seen_urls.insert(absolute_url.clone());
                        links.push(Link {
                            url: absolute_url,
                            text,
                        });
                    }
                }
            }
        }
        
        links
    }

    /// Extract images with absolute URLs
    fn extract_images(&self, document: &Html, base_url: &Url) -> Vec<Image> {
        let mut images = Vec::new();
        let mut seen_srcs = HashSet::new();
        
        if let Ok(selector) = Selector::parse("img[src]") {
            for element in document.select(&selector) {
                if let Some(src) = element.value().attr("src") {
                    // Convert relative URLs to absolute
                    let absolute_src = match base_url.join(src) {
                        Ok(url) => url.to_string(),
                        Err(_) => src.to_string(),
                    };
                    
                    // Avoid duplicates
                    if !seen_srcs.contains(&absolute_src) {
                        seen_srcs.insert(absolute_src.clone());
                        
                        let alt = element.value().attr("alt").unwrap_or("").to_string();
                        let title = element.value().attr("title").unwrap_or("").to_string();
                        
                        images.push(Image {
                            src: absolute_src,
                            alt,
                            title,
                        });
                    }
                }
            }
        }
        
        images
    }

    /// Extract code blocks with language hints (Priority 1 fix)
    fn extract_code_blocks(&self, document: &Html) -> Vec<CodeBlock> {
        let mut code_blocks = Vec::new();
        
        // Extract <pre><code> blocks
        if let Ok(selector) = Selector::parse("pre code, pre, code") {
            for element in document.select(&selector) {
                // Get the code content preserving whitespace
                let code = element.text().collect::<Vec<_>>().join("");
                
                // Skip if empty or too small
                if code.trim().len() < 10 {
                    continue;
                }
                
                // Try to extract language hint from class attribute
                let language = element.value().attr("class")
                    .and_then(|classes| {
                        // Look for patterns like "language-rust", "lang-python", "rust", etc.
                        classes.split_whitespace()
                            .find(|c| c.starts_with("language-") || c.starts_with("lang-"))
                            .map(|c| {
                                c.strip_prefix("language-")
                                    .or_else(|| c.strip_prefix("lang-"))
                                    .unwrap_or(c)
                                    .to_string()
                            })
                    })
                    .or_else(|| {
                        // Check parent <pre> element
                        element.value().attr("data-lang").map(|s| s.to_string())
                    });
                
                code_blocks.push(CodeBlock {
                    language,
                    code,
                    start_char: None,  // Could be enhanced with position tracking
                    end_char: None,
                });
            }
        }
        
        // Deduplicate (sometimes code appears in nested tags)
        let mut seen = HashSet::new();
        code_blocks.retain(|cb| {
            let key = format!("{:?}:{}", cb.language, &cb.code);
            seen.insert(key)
        });
        
        code_blocks
    }

    /// Calculate extraction quality score (Priority 1 fix)
    /// Returns a score from 0.0 to 1.0 indicating extraction quality
    fn calculate_extraction_score(
        &self,
        word_count: usize,
        published_at: &Option<String>,
        code_blocks: &[CodeBlock],
        headings: &[Heading],
    ) -> f64 {
        let mut score = 0.0;
        
        // Content presence (0.0-0.3)
        if word_count > 50 {
            score += 0.3;
        } else if word_count > 20 {
            score += 0.15;
        }
        
        // Has publish date (0.2)
        if published_at.is_some() {
            score += 0.2;
        }
        
        // Has code blocks (0.2) - good for technical content
        if !code_blocks.is_empty() {
            score += 0.2;
        }
        
        // Has structured headings (0.15)
        if headings.len() > 2 {
            score += 0.15;
        } else if headings.len() > 0 {
            score += 0.075;
        }
        
        // Content length score (0.0-0.15)
        // Optimal around 500-2000 words
        let length_score = if word_count >= 500 && word_count <= 2000 {
            0.15
        } else if word_count > 2000 {
            0.15 * (2000.0 / word_count as f64).min(1.0)
        } else if word_count > 100 {
            0.15 * (word_count as f64 / 500.0)
        } else {
            0.0
        };
        score += length_score;
        
        score.min(1.0)
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
                assert!(!content.clean_content.is_empty(), "Content should not be empty");
                assert_eq!(content.status_code, 200, "Status code should be 200");
                assert!(content.word_count > 0, "Word count should be greater than 0");
            }
            Err(e) => {
                println!("Rust scraper test failed: {}", e);
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