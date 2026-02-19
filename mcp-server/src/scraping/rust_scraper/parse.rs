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

/// Extract code blocks with language hints.
    /// When content is detected as code, applies NeuroSiphon-style import nuking
    /// to strip boilerplate import / use / require lines that add token noise
    /// without contributing to the logical content of the block.
    ///
    /// `url_lang_hint`: language inferred from the URL extension (e.g. `Some("rust")` for
    /// `.rs` files).  Used as fallback when no code-fence class is found in the HTML;
    /// enables import nuking on raw source files served from GitHub/CDN.
    pub(super) fn extract_code_blocks(&self, document: &Html, url_lang_hint: Option<&str>, is_tutorial: bool) -> Vec<CodeBlock> {
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

                // If we captured a <pre> wrapper, it may not carry the language class.
                // mdBook often puts the language on an inner <code class="language-rust ...">.
                let language = language.or_else(|| {
                    if element.value().name() != "pre" {
                        return None;
                    }
                    let code_sel = Selector::parse("code").ok()?;
                    let code_el = element.select(&code_sel).next()?;
                    code_el
                        .value()
                        .attr("class")
                        .and_then(|classes| {
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
                });

                // 🧬 Rule B: fall back to the URL-inferred language when the HTML carries
                // no code-fence class.  This enables import nuking on raw source files
                // (e.g. raw.githubusercontent.com/**/*.rs) that serve plain text.
                let language = language.or_else(|| url_lang_hint.map(|s| s.to_string()));

                // 🧬 NeuroSiphon-style: nuke pure import/require/use blocks when in aggressive mode.
                // Import blocks are noisy and rarely relevant to the user's query.
                // 🧬 Task 2 (Tutorial Immunity): never strip imports on documentation / tutorial
                // sites — imports are critical context there, not noise.
                let code = if !is_tutorial && crate::core::config::neurosiphon_enabled() && self.is_aggressive_mode() {
                    nuke_import_block(&code, language.as_deref())
                } else {
                    code
                };

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

// ─────────────────────────────────────────────────────────────────────────────
// 🧬 Import Nuker — NeuroSiphon DNA Transfer
// Strips pure import/require/use header blocks from code snippets.
// Preserves TODO/FIXME comments and any line with actual logic.
// ─────────────────────────────────────────────────────────────────────────────

/// Remove leading/trailing import-only lines from a code block.
/// "Import-only" means lines that solely declare imports (no function/class body,
/// no logic statements). Lines with TODO/FIXME are always preserved.
fn nuke_import_block(code: &str, language: Option<&str>) -> String {
    let lang = language.unwrap_or("").to_ascii_lowercase();

    // For languages where we understand import syntax
    let is_import_line: Box<dyn Fn(&str) -> bool> = match lang.as_str() {
        "rust" => Box::new(|line: &str| {
            let t = line.trim();
            (t.starts_with("use ") || t.starts_with("extern crate "))
                && !contains_todo_fixme(t)
                && !t.contains("fn ")
                && !t.contains("struct ")
        }),
        "python" | "py" => Box::new(|line: &str| {
            let t = line.trim();
            (t.starts_with("import ") || t.starts_with("from "))
                && !contains_todo_fixme(t)
                && !t.contains("def ")
                && !t.contains("class ")
        }),
        "javascript" | "js" | "typescript" | "ts" | "jsx" | "tsx" => Box::new(|line: &str| {
            let t = line.trim();
            (t.starts_with("import ") || t.starts_with("const ") && t.contains("require("))
                && !contains_todo_fixme(t)
                && !t.contains("=>")
                && !t.contains("function ")
        }),
        "go" => Box::new(|line: &str| {
            let t = line.trim();
            (t.starts_with("import ") || t == "import (")
                && !contains_todo_fixme(t)
        }),
        "java" | "kotlin" | "scala" => Box::new(|line: &str| {
            let t = line.trim();
            t.starts_with("import ") && !contains_todo_fixme(t)
        }),
        "csharp" | "cs" => Box::new(|line: &str| {
            let t = line.trim();
            t.starts_with("using ") && !contains_todo_fixme(t)
        }),
        _ => {
            // Unknown language — don't touch anything
            return code.to_string();
        }
    };

    let lines: Vec<&str> = code.lines().collect();
    let total = lines.len();

    // 🛡️ Min-snippet guard: never touch blocks < 15 lines — they risk becoming
    // unusable after nuking (e.g. a 5-line snippet where 3 lines are `use` statements).
    if total < 15 {
        return code.to_string();
    }

    // Count leading import lines
    let mut leading_end = 0;
    let mut in_rust_multiline_use = false;
    for &line in &lines {
        let t = line.trim();

        // Always allow blank lines within an import header region.
        if t.is_empty() {
            leading_end += 1;
            continue;
        }

        // Rust: treat multi-line `use ... { ... };` blocks as part of the import header.
        // This makes import nuking effective on common mdBook/guide snippets.
        if lang == "rust" {
            if in_rust_multiline_use {
                leading_end += 1;
                if t.contains(';') {
                    in_rust_multiline_use = false;
                }
                continue;
            }

            if (t.starts_with("use ") || t.starts_with("extern crate ")) && !contains_todo_fixme(t) {
                leading_end += 1;
                if !t.contains(';') {
                    in_rust_multiline_use = true;
                }
                continue;
            }
        }

        if is_import_line(line) {
            leading_end += 1;
            continue;
        }

        break;
    }

    // Only nuke if imports occupy more than 30% of the block (clear import-heavy header)
    // and at least 3 import lines exist (avoid single-import helpers).
    let import_ratio = leading_end as f64 / total.max(1) as f64;
    // Aggressive mode: also strip 1-2 leading import lines when the snippet is long,
    // because they tend to be boilerplate and waste tokens.
    let should_nuke = (leading_end >= 3 && import_ratio > 0.30) || (leading_end >= 1 && total >= 12 && import_ratio > 0.10);

    if should_nuke {
        let trimmed = lines[leading_end..].join("\n");
        if !trimmed.trim().is_empty() {
            return trimmed;
        }
    }

    code.to_string()
}

fn contains_todo_fixme(s: &str) -> bool {
    let u = s.to_ascii_uppercase();
    u.contains("TODO") || u.contains("FIXME")
}
