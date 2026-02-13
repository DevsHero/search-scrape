use crate::types::{Image, ScrapeResponse, SearchResult};
use std::collections::HashSet;

pub fn push_warning_unique(warnings: &mut Vec<String>, warning: &str) {
    if !warnings.iter().any(|w| w == warning) {
        warnings.push(warning.to_string());
    }
}

pub fn apply_scrape_content_limit(
    content: &mut ScrapeResponse,
    max_chars: usize,
    truncate_clean_content: bool,
) {
    content.actual_chars = content.clean_content.len();
    content.max_chars_limit = Some(max_chars);

    if content.actual_chars > max_chars {
        content.truncated = true;
        if truncate_clean_content {
            content.clean_content = content.clean_content.chars().take(max_chars).collect();
        }
        push_warning_unique(&mut content.warnings, "content_truncated");
    } else {
        content.truncated = false;
    }
}

pub fn dedupe_search_result_indexes(results: &[SearchResult], snippet_chars: usize) -> (Vec<usize>, usize) {
    let mut deduped_indexes = Vec::new();
    let mut seen_signature = HashSet::new();

    for (index, result) in results.iter().enumerate() {
        let title_key = result.title.to_ascii_lowercase().trim().to_string();
        let snippet_key = result
            .content
            .chars()
            .take(snippet_chars)
            .collect::<String>()
            .to_ascii_lowercase()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let signature = format!("{}|{}", title_key, snippet_key);
        if seen_signature.insert(signature) {
            deduped_indexes.push(index);
        }
    }

    let duplicate_removed = results.len().saturating_sub(deduped_indexes.len());
    (deduped_indexes, duplicate_removed)
}

pub fn build_image_markdown_hints(images: &[Image], title_fallback: &str, max_images: usize) -> String {
    if images.is_empty() {
        return String::new();
    }

    let mut hints = Vec::new();
    for image in images.iter().take(max_images.max(1)) {
        let label = if !image.alt.trim().is_empty() {
            image.alt.trim().to_string()
        } else if !image.title.trim().is_empty() {
            image.title.trim().to_string()
        } else if !title_fallback.trim().is_empty() {
            title_fallback.to_string()
        } else {
            "image".to_string()
        };
        hints.push(format!("![{}]({})", label, image.src));
    }

    if hints.is_empty() {
        String::new()
    } else {
        format!("\n\nImage Markdown Hints:\n{}", hints.join("\n"))
    }
}
