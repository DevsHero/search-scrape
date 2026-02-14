use super::RustScraper;
use crate::types::EmbeddedDataSource;
use scraper::{Html, Selector};
use std::collections::HashSet;
use url::Url;
use whatlang::{detect, Lang};

impl RustScraper {
    /// Extract page title with fallback to h1
    pub(super) fn extract_title(&self, document: &Html) -> String {
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
    pub(super) fn extract_meta_description(&self, document: &Html) -> String {
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
    pub(super) fn extract_meta_keywords(&self, document: &Html) -> String {
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
    pub(super) fn extract_canonical(&self, document: &Html, base: &Url) -> Option<String> {
        if let Ok(selector) = Selector::parse("link[rel=\"canonical\"]") {
            if let Some(el) = document.select(&selector).next() {
                if let Some(href) = el.value().attr("href") {
                    return base
                        .join(href)
                        .ok()
                        .map(|u| u.to_string())
                        .or_else(|| Some(href.to_string()));
                }
            }
        }
        None
    }

    /// Extract site name (OpenGraph fallback)
    pub(super) fn extract_site_name(&self, document: &Html) -> Option<String> {
        if let Ok(selector) = Selector::parse("meta[property=\"og:site_name\"]") {
            if let Some(el) = document.select(&selector).next() {
                if let Some(content) = el.value().attr("content") {
                    let v = content.trim();
                    if !v.is_empty() {
                        return Some(v.to_string());
                    }
                }
            }
        }
        None
    }

    /// Extract OpenGraph basic fields
    pub(super) fn extract_open_graph(
        &self,
        document: &Html,
        base: &Url,
    ) -> (Option<String>, Option<String>, Option<String>) {
        let og_title = if let Ok(sel) = Selector::parse("meta[property=\"og:title\"]") {
            document
                .select(&sel)
                .next()
                .and_then(|e| e.value().attr("content"))
                .map(|s| s.trim().to_string())
        } else {
            None
        };
        let og_description = if let Ok(sel) = Selector::parse("meta[property=\"og:description\"]") {
            document
                .select(&sel)
                .next()
                .and_then(|e| e.value().attr("content"))
                .map(|s| s.trim().to_string())
        } else {
            None
        };
        let og_image = if let Ok(sel) = Selector::parse("meta[property=\"og:image\"]") {
            document
                .select(&sel)
                .next()
                .and_then(|e| e.value().attr("content"))
                .and_then(|s| {
                    base.join(s)
                        .ok()
                        .map(|u| u.to_string())
                        .or_else(|| Some(s.to_string()))
                })
        } else {
            None
        };
        (og_title, og_description, og_image)
    }

    /// Extract author
    pub(super) fn extract_author(&self, document: &Html) -> Option<String> {
        // Meta author
        if let Ok(sel) = Selector::parse("meta[name=\"author\"]") {
            if let Some(el) = document.select(&sel).next() {
                if let Some(content) = el.value().attr("content") {
                    return Some(content.trim().to_string());
                }
            }
        }
        // Article author
        if let Ok(sel) = Selector::parse("meta[property=\"article:author\"]") {
            if let Some(el) = document.select(&sel).next() {
                if let Some(content) = el.value().attr("content") {
                    return Some(content.trim().to_string());
                }
            }
        }
        None
    }

    /// Extract published time
    pub(super) fn extract_published_time(&self, document: &Html) -> Option<String> {
        if let Ok(sel) = Selector::parse("meta[property=\"article:published_time\"]") {
            if let Some(el) = document.select(&sel).next() {
                if let Some(content) = el.value().attr("content") {
                    return Some(content.trim().to_string());
                }
            }
        }
        None
    }

    /// Detect language from HTML attributes and content
    pub(super) fn detect_language(&self, document: &Html, html: &str) -> String {
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

    /// Extract embedded application state from SPA pages.
    ///
    /// Universal approach: find the *largest* JSON-looking blob inside <script> tags.
    ///
    /// Examples this catches:
    /// - <script type="application/ld+json"> ... </script>
    /// - <script type="application/json" id="__NEXT_DATA__"> ... </script>
    /// - Hydration/state blobs where the script text is raw JSON (common on modern SPAs)
    pub(super) fn extract_embedded_state_json(&self, document: &Html) -> Option<String> {
        let Ok(sel) = Selector::parse("script") else {
            return None;
        };

        let mut best: Option<String> = None;
        let mut best_len: usize = 0;

        for script in document.select(&sel) {
            let t = script.value().attr("type").unwrap_or("");
            let id = script.value().attr("id").unwrap_or("");

            // Grab the script text in the most robust way.
            let raw = script.text().collect::<Vec<_>>().join("");
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                continue;
            }

            // Only consider script bodies that look like pure JSON.
            let looks_like_json = trimmed.starts_with('{') || trimmed.starts_with('[');
            if !looks_like_json {
                continue;
            }

            // Prefer known JSON-bearing types/ids, but don't require them.
            let is_json_ld = t == "application/ld+json";
            let is_common_state_id = id.eq_ignore_ascii_case("__NEXT_DATA__")
                || id.to_ascii_lowercase().contains("__initial_state")
                || id.to_ascii_lowercase().contains("initial")
                || id.to_ascii_lowercase().contains("state")
                || id.to_ascii_lowercase().contains("bootstrap")
                || id.to_ascii_lowercase().contains("deferred");

            let hinted = matches!(t, "application/json" | "application/ld+json")
                || id.eq_ignore_ascii_case("__NEXT_DATA__")
                || is_common_state_id
                || trimmed.contains("niobe")
                || trimmed.contains("__INITIAL_STATE__")
                || trimmed.contains("__APOLLO_STATE__");

            let len = trimmed.len();

            // Avoid capturing tiny JSON config fragments; allow smaller JSON-LD and common state ids.
            let min_len = if is_json_ld || is_common_state_id {
                200
            } else {
                800
            };
            if len < min_len {
                continue;
            }

            if hinted && len > best_len {
                best_len = len;
                best = Some(trimmed.to_string());
                continue;
            }

            // If no hinted candidates yet, still pick the largest blob.
            if best.is_none() && len > best_len {
                best_len = len;
                best = Some(trimmed.to_string());
            }
        }

        best
    }

    pub(super) fn collect_embedded_data_sources(&self, document: &Html) -> Vec<EmbeddedDataSource> {
        let mut out: Vec<EmbeddedDataSource> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        // Collect all JSON-LD scripts (even if small).
        if let Ok(sel) = Selector::parse("script[type='application/ld+json']") {
            for script in document.select(&sel) {
                let json_text = script.inner_html();
                let trimmed = json_text.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if !seen.insert(trimmed.to_string()) {
                    continue;
                }
                out.push(EmbeddedDataSource {
                    source_type: "jsonld".to_string(),
                    content: trimmed.to_string(),
                });
            }
        }

        // Collect large JSON blobs from any <script> tag.
        let Ok(sel) = Selector::parse("script") else {
            return out;
        };

        for script in document.select(&sel) {
            let t = script.value().attr("type").unwrap_or("");
            let id = script.value().attr("id").unwrap_or("");
            let raw = script.text().collect::<Vec<_>>().join("");
            let trimmed = raw.trim();

            // Only keep JSON-like payloads above the requested threshold.
            if trimmed.len() <= 1000 {
                continue;
            }
            if !(trimmed.starts_with('{') || trimmed.starts_with('[')) {
                continue;
            }

            if !seen.insert(trimmed.to_string()) {
                continue;
            }

            let id_lower = id.to_ascii_lowercase();
            let source_type = if t == "application/ld+json" {
                "jsonld"
            } else if id.eq_ignore_ascii_case("__NEXT_DATA__") {
                "next_data"
            } else if id_lower.contains("initial")
                || id_lower.contains("state")
                || id_lower.contains("bootstrap")
            {
                "initial_state"
            } else if trimmed.contains("niobe") {
                "niobe"
            } else {
                "json"
            };

            out.push(EmbeddedDataSource {
                source_type: source_type.to_string(),
                content: trimmed.to_string(),
            });
        }

        out
    }
}
