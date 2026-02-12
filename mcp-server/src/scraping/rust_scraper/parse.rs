use super::RustScraper;
use crate::types::{CodeBlock, Heading, Image, Link};
use scraper::{Html, Selector};
use std::collections::HashSet;
use tracing::info;
use url::Url;

impl RustScraper {
    /// Extract headings (h1-h6)
    pub(super) fn extract_headings(&self, document: &Html) -> Vec<Heading> {
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
    pub(super) fn extract_content_links(&self, document: &Html, base_url: &Url) -> Vec<Link> {
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
                    info!(
                        "Extracted {} links from main content using selector: {}",
                        links.len(),
                        content_sel
                    );
                    return links;
                }
            }
        }

        // Fallback to all links if no main content found
        info!("No main content area found, using all document links");
        self.extract_links(document, base_url)
    }

    /// Helper to extract links from a specific selector
    fn extract_links_from_selector(
        &self,
        document: &Html,
        base_url: &Url,
        selector_str: &str,
    ) -> Vec<Link> {
        let mut links = Vec::new();
        let mut seen_urls = HashSet::new();

        if let Ok(selector) = Selector::parse(selector_str) {
            for element in document.select(&selector) {
                if let Some(href) = element.value().attr("href") {
                    // Skip anchor links, javascript, and common non-content patterns
                    if href.starts_with('#')
                        || href.starts_with("javascript:")
                        || href.starts_with("mailto:")
                    {
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
    pub(super) fn extract_images(&self, document: &Html, base_url: &Url) -> Vec<Image> {
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

                        images.push(Image { src: absolute_src, alt, title });
                    }
                }
            }
        }

        images
    }

    /// Extract code blocks with language hints (Priority 1 fix)
    pub(super) fn extract_code_blocks(&self, document: &Html) -> Vec<CodeBlock> {
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
                let language = element
                    .value()
                    .attr("class")
                    .and_then(|classes| {
                        // Look for patterns like "language-rust", "lang-python", "rust", etc.
                        classes
                            .split_whitespace()
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
                    start_char: None,
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
}
