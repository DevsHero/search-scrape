use super::RustScraper;
use readability::extractor;
use regex::Regex;
use scraper::{Html, Selector};
use select::{
    document::Document as SelectDoc,
    predicate::{Attr as SelAttr, Class as SelClass, Name as SelName, Predicate},
};
use tracing::{info, warn};
use url::Url;

impl RustScraper {
    /// Fallback to og:description when main content is missing or too small
    pub(super) fn apply_og_description_fallback(
        &self,
        clean_content: String,
        og_description: &Option<String>,
    ) -> String {
        let current_words = self.count_words(&clean_content);
        if current_words >= 50 && !clean_content.trim().is_empty() {
            return clean_content;
        }

        if let Some(desc) = og_description {
            let trimmed = desc.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }

        clean_content
    }

    /// Extract clean, readable content using readability, preceded by AGGRESSIVE HTML preprocessing
    /// Includes TEXT-ONLY fallback mode for high-noise sites (e.g., Reddit, social media)
    pub(super) fn extract_clean_content(&self, html: &str, base_url: &Url) -> String {
        // 🧬 Special handling for code/pre-heavy sites (NowSecure, Pastebin, etc.)
        let is_pre_priority = base_url
            .host_str()
            .map(|h| h.contains("nowsecure") || h.contains("pastebin"))
            .unwrap_or(false);

        if is_pre_priority {
            warn!(
                "🧬 Pre-formatted content priority mode for {}",
                base_url.host_str().unwrap_or("")
            );
            if let Some(pre_content) = self.extract_pre_formatted(html) {
                if !pre_content.trim().is_empty() {
                    info!("✓ Extracted {} chars from <pre> tags", pre_content.len());
                    return pre_content;
                }
            }
        }

        // 1) AGGRESSIVE Pre-clean HTML to strip nav, header, footer, forms, buttons, hidden elements
        let mut pre = self.preprocess_html(html);

        // 1.5) Domain-specific aggressive cleaning
        pre = self.domain_specific_cleaning(&pre, base_url);

        // 1a) mdBook-style extractor (e.g., Rust Book) — try focused body first
        if let Some(md_text) = self.extract_mdbook_like(&pre) {
            if md_text.len() > 30 {
                return self.post_clean_text(&md_text);
            }
        }

        // 2) Readability pass
        let readability_text = match extractor::extract(&mut pre.as_bytes(), base_url) {
            Ok(product) => {
                let text = html2md::parse_html(&product.content);
                self.post_clean_text(&text)
            }
            Err(e) => {
                warn!("Readability extraction failed: {}, will try heuristics", e);
                String::new()
            }
        };

        // 3) Heuristic main-content extraction (article/main/role=main/etc.)
        let heuristic_text = self.heuristic_main_extraction(&pre);

        // 4) Choose the better result by word count
        let rt_words = self.count_words(&readability_text);
        let ht_words = self.count_words(&heuristic_text);
        info!(
            "Extraction passes - Readability: {} words, Heuristic: {} words",
            rt_words, ht_words
        );

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

        let final_text = self.post_clean_text(&chosen);
        if self.is_high_noise_content(&final_text) {
            warn!(
                "High-noise content detected (likely JS/UI heavy site like Reddit), using TEXT-ONLY extraction"
            );
            return self.text_only_extraction(&pre);
        }

        if final_text.len() < 80 {
            let whole = html2md::parse_html(&pre);
            return self.post_clean_text(&whole);
        }
        final_text
    }

    /// Extract content from mdBook-like structures (#content, main, article) using select crate
    fn extract_mdbook_like(&self, html: &str) -> Option<String> {
        let doc = SelectDoc::from(html);

        // Try GitHub markdown body first (for GitHub README views)
        if let Some(node) = doc.find(SelClass("markdown-body")).next() {
            let inner = node.inner_html();
            let text = html2md::parse_html(&inner);
            let cleaned = self.clean_text(&text);
            let word_count = self.count_words(&cleaned);
            info!("mdBook extractor (.markdown-body): {} words", word_count);
            if word_count > 50 {
                return Some(cleaned);
            }
        }

        // Try #content first - this is mdBook's main content container
        if let Some(node) = doc.find(SelName("div").and(SelAttr("id", "content"))).next() {
            let inner = node.inner_html();
            let text = html2md::parse_html(&inner);
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
            let text = html2md::parse_html(&inner);
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
            let text = html2md::parse_html(&inner);
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

    /// 🧬 Extract pre-formatted content (for NowSecure, code sites)
    fn extract_pre_formatted(&self, html: &str) -> Option<String> {
        let document = Html::parse_document(html);
        let mut content = String::new();

        // Extract all <pre> tags
        if let Ok(pre_selector) = Selector::parse("pre") {
            for element in document.select(&pre_selector) {
                let text = element.text().collect::<Vec<_>>().join("");
                if !text.trim().is_empty() {
                    content.push_str(&text);
                    content.push_str("\n\n");
                }
            }
        }

        // Also check for <code> blocks outside <pre>
        if content.is_empty() {
            if let Ok(code_selector) = Selector::parse("code") {
                for element in document.select(&code_selector) {
                    let text = element.text().collect::<Vec<_>>().join("");
                    if text.len() > 20 {
                        content.push_str(&text);
                        content.push_str("\n");
                    }
                }
            }
        }

        if content.trim().is_empty() {
            None
        } else {
            Some(self.post_clean_text(&content))
        }
    }

    /// Fallback text extraction when readability fails
    fn fallback_text_extraction(&self, html: &str) -> String {
        let document = Html::parse_document(html);

        let mut text_parts = Vec::new();

        if let Ok(body_selector) = Selector::parse("body") {
            if let Some(body) = document.select(&body_selector).next() {
                self.extract_text_recursive(&body, &mut text_parts);
            }
        } else {
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
                if matches!(
                    tag_name,
                    "script" | "style" | "noscript" | "svg" | "canvas" | "iframe" | "form" | "header"
                        | "footer" | "nav" | "aside"
                ) {
                    continue;
                }

                let attrs = child_element.value();
                let mut skip = false;
                if let Some(id) = attrs.id() {
                    skip |= self.is_noise_identifier(id);
                }
                for class in attrs.classes() {
                    if self.is_noise_identifier(class) {
                        skip = true;
                        break;
                    }
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
    pub(super) fn clean_text(&self, text: &str) -> String {
        let re_whitespace = Regex::new(r"\s+").unwrap();
        let re_newlines = Regex::new(r"\n\s*\n").unwrap();

        let cleaned = re_whitespace.replace_all(text, " ");
        let cleaned = re_newlines.replace_all(&cleaned, "\n\n");

        cleaned.trim().to_string()
    }

    /// Final post-processing to strip boilerplate lines
    fn post_clean_text(&self, text: &str) -> String {
        let out = self.clean_text(text);
        let input_words = self.count_words(&out);

        let garbage = [
            r"(?i)^subscribe$",
            r"(?i)^sign up$",
            r"(?i)^cookie",
            r"(?i)^accept all$",
            r"(?i)^advert",
            r"(?i)^sponsor",
            r"(?i)^newsletter$",
            r"(?i)^related articles",
            r"(?i)^comments?$",
            r"(?i)^read more$",
            r"(?i)^continue reading$",
        ];
        let re_garbage = Regex::new(&garbage.join("|")).unwrap();

        let mut kept = Vec::new();
        for line in out.split('\n') {
            let line_trim = line.trim();
            if line_trim.is_empty() {
                continue;
            }
            if line_trim.len() < 2 {
                continue;
            }
            if re_garbage.is_match(line_trim) {
                continue;
            }
            kept.push(line_trim.to_string());
        }

        kept.dedup();
        let result = kept.join("\n");

        let output_words = self.count_words(&result);
        if output_words < input_words / 2 {
            warn!(
                "post_clean_text stripped >50% content ({} → {} words)",
                input_words, output_words
            );
        }

        let re_multi_nl = Regex::new(r"\n{3,}").unwrap();
        re_multi_nl.replace_all(&result, "\n\n").to_string()
    }

    /// Preprocess HTML before readability
    fn preprocess_html(&self, html: &str) -> String {
        let mut s = html.to_string();

        let re_block = Regex::new(
            r"(?is)<(?:script|style|noscript|svg|canvas|iframe)[^>]*?>.*?</(?:script|style|noscript|svg|canvas|iframe)>",
        )
        .unwrap();
        s = re_block.replace_all(&s, " ").to_string();

        let re_structural = Regex::new(r"(?is)<(?:nav|header|footer|aside)[^>]*?>.*?</(?:nav|header|footer|aside)>")
            .unwrap();
        s = re_structural.replace_all(&s, " ").to_string();

        let re_interactive =
            Regex::new(r"(?is)<(?:form|button)[^>]*?>.*?</(?:form|button)>").unwrap();
        s = re_interactive.replace_all(&s, " ").to_string();

        let re_hidden = Regex::new(
            r#"(?is)<[^>]*?(?:display:\s*none|visibility:\s*hidden|aria-hidden=\"true\")[^>]*?>.*?</[^>]+>"#,
        )
        .unwrap();
        s = re_hidden.replace_all(&s, " ").to_string();

        let re_ad_blocks = Regex::new(
            r#"(?is)<(?:div|section|article)[^>]*?(?:id|class)=(?:'|\")[^'\">]*(?:ads|advert|sponsor|promo|related|cookie|banner|modal|subscribe|newsletter|share|social|sidebar|comments|breadcrumb|pagination)[^'\">]*(?:'|\")[^>]*?>.*?</(?:div|section|article)>"#,
        )
        .unwrap();
        s = re_ad_blocks.replace_all(&s, " ").to_string();

        s
    }

    fn domain_specific_cleaning(&self, html: &str, base_url: &Url) -> String {
        let mut s = html.to_string();
        let domain = base_url.host_str().map(|h| h.to_lowercase());

        if let Some(d) = domain {
            if d.contains("amazon") {
                let re_amazon = Regex::new(
                    r#"(?is)<(?:div|section)[^>]*?(?:id|class)=(?:'|\")[^'\">]*(?:dp-ads|recommendations|also-bought|frequently-bought|similar-items|sponsored|detail-bullets-pricing|marketing-message|btf-content)[^'\">]*(?:'|\")[^>]*?>.*?</(?:div|section)>"#,
                )
                .unwrap();
                s = re_amazon.replace_all(&s, " ").to_string();
                info!("Applied Amazon-specific cleaning");
            }

            if d.contains("ebay") || d.contains("walmart") {
                let re_ecommerce = Regex::new(
                    r#"(?is)<(?:div|section)[^>]*?(?:id|class)=(?:'|\")[^'\">]*(?:recommendations|cross-sell|upsell|related-products|sponsored|carousel)[^'\">]*(?:'|\")[^>]*?>.*?</(?:div|section)>"#,
                )
                .unwrap();
                s = re_ecommerce.replace_all(&s, " ").to_string();
                info!("Applied e-commerce cleaning");
            }

            if d.contains("linkedin") {
                let re_linkedin = Regex::new(
                    r#"(?is)<(?:div|section)[^>]*?(?:id|class)=(?:'|\")[^'\">]*(?:login-wall|auth-wall|artdeco-modal|msg-overlay|scaffold-layout__sidebar|job-alert|global-nav)[^'\">]*(?:'|\")[^>]*?>.*?</(?:div|section)>"#,
                )
                .unwrap();
                s = re_linkedin.replace_all(&s, " ").to_string();
                info!("Applied LinkedIn-specific cleaning");
            }

            if d.contains("twitter") || d.contains("x.com") {
                let re_twitter = Regex::new(
                    r#"(?is)<(?:div|section)[^>]*?(?:id|class)=(?:'|\")[^'\">]*(?:login|signup|global-nav|sidebar|who-to-follow|trends|footer|modal)[^'\">]*(?:'|\")[^>]*?>.*?</(?:div|section)>"#,
                )
                .unwrap();
                s = re_twitter.replace_all(&s, " ").to_string();
                info!("Applied Twitter/X-specific cleaning");
            }

            if d.contains("zillow") || d.contains("redfin") {
                let re_realestate = Regex::new(
                    r#"(?is)<(?:div|section)[^>]*?(?:id|class)=(?:'|\")[^'\">]*(?:similar-homes|nearby-homes|agent-contact|mortgage-calculator|contact-form)[^'\">]*(?:'|\")[^>]*?>.*?</(?:div|section)>"#,
                )
                .unwrap();
                s = re_realestate.replace_all(&s, " ").to_string();
                info!("Applied real estate cleaning");
            }

            if d.contains("github") {
                let re_github = Regex::new(
                    r#"(?is)<(?:div|section)[^>]*?(?:id|class)=(?:'|\")[^'\">]*(?:file-navigation|repository-content-pjax|getting-started|trending|explore)[^'\">]*(?:'|\")[^>]*?>.*?</(?:div|section)>"#,
                )
                .unwrap();
                s = re_github.replace_all(&s, " ").to_string();
                info!("Applied GitHub-specific cleaning");
            }

            if d.contains("substack") || d.contains("medium") || d.contains("bloomberg") {
                let re_publication = Regex::new(
                    r#"(?is)<(?:div|section)[^>]*?(?:id|class)=(?:'|\")[^'\">]*(?:paywall-banner|subscription-widget|author-bio-bottom|related-posts|recommended-stories)[^'\">]*(?:'|\")[^>]*?>.*?</(?:div|section)>"#,
                )
                .unwrap();
                s = re_publication.replace_all(&s, " ").to_string();
                info!("Applied publication platform cleaning");
            }

            if d.contains("bloomberg") {
                let re_bloomberg = Regex::new(
                    r#"(?is)<(?:div|section)[^>]*?(?:id|class)=(?:'|\")[^'\">]*(?:paywall|subscribe|newsletter|modal|ad|promo|cookie|consent)[^'\">]*(?:'|\")[^>]*?>.*?</(?:div|section)>"#,
                )
                .unwrap();
                s = re_bloomberg.replace_all(&s, " ").to_string();
                info!("Applied Bloomberg-specific cleaning");
            }
        }

        s
    }

    fn is_noise_identifier(&self, ident: &str) -> bool {
        let ident = ident.to_ascii_lowercase();
        let needles = [
            "ads",
            "advert",
            "adsense",
            "adunit",
            "ad-slot",
            "ad_container",
            "adbox",
            "sponsor",
            "promo",
            "cookie",
            "consent",
            "banner",
            "modal",
            "subscribe",
            "newsletter",
            "share",
            "social",
            "sidebar",
            "comments",
            "related",
            "breadcrumb",
            "pagination",
            "nav",
            "footer",
            "header",
            "hero",
            "toolbar",
        ];
        if needles.iter().any(|n| ident.contains(n)) {
            return true;
        }
        if ident.contains("-ad") || ident.contains("ad-") || ident.contains("_ad") || ident.contains("ad_") {
            return true;
        }
        false
    }

    fn heuristic_main_extraction(&self, html: &str) -> String {
        let document = Html::parse_document(html);

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
                    let text = parts.join(" ");
                    let cleaned = self.clean_text(&text);
                    let wc = self.count_words(&cleaned);
                    if wc > best_words {
                        best_words = wc;
                        best_text = cleaned;
                    }
                }
            }
        }

        best_text
    }

    pub(super) fn count_words(&self, text: &str) -> usize {
        text.split_whitespace().count()
    }

    fn is_high_noise_content(&self, text: &str) -> bool {
        let lines: Vec<&str> = text.lines().collect();
        if lines.len() < 10 {
            return false;
        }

        let noise_keywords = [
            "share",
            "upvote",
            "downvote",
            "comment",
            "reply",
            "login",
            "sign up",
            "subscribe",
            "follow",
            "like",
            "tweet",
            "retweet",
            "menu",
            "navigation",
        ];
        let mut noise_lines = 0;
        let mut total_chars = 0;

        for line in &lines {
            let trimmed = line.trim();
            if trimmed.len() < 10 {
                noise_lines += 1;
                continue;
            }
            total_chars += trimmed.len();

            let lower = trimmed.to_lowercase();
            if noise_keywords.iter().any(|kw| lower.contains(kw)) && trimmed.len() < 40 {
                noise_lines += 1;
            }
        }

        let noise_ratio = noise_lines as f64 / lines.len() as f64;
        let avg_line_length = if !lines.is_empty() { total_chars / lines.len() } else { 0 };

        noise_ratio > 0.6 || avg_line_length < 20
    }

    fn text_only_extraction(&self, html: &str) -> String {
        let document = Html::parse_document(html);

        let content_selectors = [
            "article",
            "main",
            ".content",
            ".post-body",
            "#main",
            ".entry-content",
        ];

        for sel_str in &content_selectors {
            if let Ok(selector) = Selector::parse(sel_str) {
                for element in document.select(&selector) {
                    let mut parts = Vec::new();
                    self.extract_text_recursive(&element, &mut parts);
                    let text = parts.join(" ");
                    let cleaned = self.clean_text(&text);
                    if self.count_words(&cleaned) > 50 {
                        return cleaned;
                    }
                }
            }
        }

        let mut paragraphs = Vec::new();
        if let Ok(p_selector) = Selector::parse("p") {
            for element in document.select(&p_selector) {
                let text = element.text().collect::<String>().trim().to_string();
                if text.len() > 30 {
                    paragraphs.push(text);
                }
            }
        }

        let combined = paragraphs.join("\n\n");
        self.clean_text(&combined)
    }
}
