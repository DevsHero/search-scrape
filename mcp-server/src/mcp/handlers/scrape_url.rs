use super::common::parse_quality_mode;
use crate::mcp::{McpCallResponse, McpContent};
use crate::types::ErrorResponse;
use crate::{scrape, AppState};
use axum::http::StatusCode;
use axum::response::Json;
use serde_json::Value;
use std::sync::Arc;
use tracing::error;

pub async fn handle(
    state: Arc<AppState>,
    arguments: &Value,
) -> Result<Json<McpCallResponse>, (StatusCode, Json<ErrorResponse>)> {
    let url = arguments
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "Missing required parameter: url".to_string(),
                }),
            )
        })?;

    let use_proxy = arguments
        .get("use_proxy")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let quality_mode = parse_quality_mode(arguments)?;

    match scrape::scrape_url_with_options(&state, url, use_proxy, Some(quality_mode)).await {
        Ok(mut content) => {
            let max_chars = arguments
                .get("max_chars")
                .and_then(|v| v.as_u64())
                .map(|n| n as usize)
                .or_else(|| {
                    std::env::var("MAX_CONTENT_CHARS")
                        .ok()
                        .and_then(|s| s.parse().ok())
                })
                .unwrap_or(10000);

            crate::content_quality::apply_scrape_content_limit(&mut content, max_chars, false);
            if content.word_count < 50 {
                crate::content_quality::push_warning_unique(&mut content.warnings, "short_content");
            }
            if content.extraction_score.map(|s| s < 0.4).unwrap_or(false) {
                crate::content_quality::push_warning_unique(
                    &mut content.warnings,
                    "low_extraction_score",
                );
            }

            let output_format = arguments
                .get("output_format")
                .and_then(|v| v.as_str())
                .unwrap_or("text");

            if output_format == "json" {
                let include_raw_html = arguments
                    .get("include_raw_html")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let mut json_content = content.clone();
                if !include_raw_html {
                    json_content.content.clear();
                    crate::content_quality::push_warning_unique(
                        &mut json_content.warnings,
                        "raw_html_omitted_in_scrape_url_json",
                    );
                }

                let json_str = serde_json::to_string_pretty(&json_content)
                    .unwrap_or_else(|e| format!(r#"{{"error": "Failed to serialize: {}"}}"#, e));
                return Ok(Json(McpCallResponse {
                    content: vec![McpContent {
                        content_type: "text".to_string(),
                        text: json_str,
                    }],
                    is_error: false,
                }));
            }

            let content_text = {
                let content_preview = if content.clean_content.is_empty() {
                    "[No content extracted]\n\n**Possible reasons:**\n\
                    • Page is JavaScript-heavy (requires browser execution)\n\
                    • Content is behind authentication/paywall\n\
                    • Site blocks automated access\n\n\
                    **Suggestion:** For JS-heavy sites, install Brave/Chrome/Chromium and set `CHROME_EXECUTABLE` if auto-discovery fails. For bot walls, use `non_robot_search` (HITL) and retry with `use_proxy: true` if needed."
                        .to_string()
                } else if content.word_count < 10 {
                    format!(
                        "{}\n\n⚠️ **Very short content** ({} words). Page may be mostly dynamic/JS-based.",
                        content.clean_content.chars().take(max_chars).collect::<String>(),
                        content.word_count
                    )
                } else {
                    let preview = content
                        .clean_content
                        .chars()
                        .take(max_chars)
                        .collect::<String>();
                    if content.clean_content.len() > max_chars {
                        format!(
                            "{}\n\n[Content truncated: {}/{} chars shown. Increase max_chars parameter to see more]",
                            preview,
                            max_chars,
                            content.clean_content.len()
                        )
                    } else {
                        preview
                    }
                };

                let image_preview_section = crate::content_quality::build_image_markdown_hints(
                    &content.images,
                    &content.title,
                    3,
                );

                let headings = content
                    .headings
                    .iter()
                    .take(10)
                    .map(|h| format!("- {} {}", h.level.to_uppercase(), h.text))
                    .collect::<Vec<_>>()
                    .join("\n");

                let sources_section = if content.links.is_empty() {
                    String::new()
                } else {
                    let mut sources = String::from("\n\nSources:\n");
                    let max_sources = arguments
                        .get("max_links")
                        .and_then(|v| v.as_u64())
                        .map(|n| n as usize)
                        .or_else(|| std::env::var("MAX_LINKS").ok().and_then(|s| s.parse().ok()))
                        .unwrap_or(100);
                    let link_count = content.links.len();
                    for (i, link) in content.links.iter().take(max_sources).enumerate() {
                        if !link.text.is_empty() {
                            sources.push_str(&format!("[{}]: {} ({})", i + 1, link.url, link.text));
                        } else {
                            sources.push_str(&format!("[{}]: {}", i + 1, link.url));
                        }
                        sources.push('\n');
                    }
                    if link_count > max_sources {
                        sources.push_str(&format!(
                            "\n(Showing {} of {} total links)\n",
                            max_sources, link_count
                        ));
                    }
                    sources
                };

                format!(
                    "{}\nURL: {}\nCanonical: {}\nWord Count: {} ({}m)\nLanguage: {}\nSite: {}\nAuthor: {}\nPublished: {}\n\nDescription: {}\nOG Image: {}\n\nHeadings:\n{}\n\nLinks: {}  Images: {}\n\nPreview:\n{}{}{}",
                    content.title,
                    content.url,
                    content.canonical_url.as_deref().unwrap_or("-"),
                    content.word_count,
                    content
                        .reading_time_minutes
                        .unwrap_or(((content.word_count as f64 / 200.0).ceil() as u32).max(1)),
                    content.language,
                    content.site_name.as_deref().unwrap_or("-"),
                    content.author.as_deref().unwrap_or("-"),
                    content.published_at.as_deref().unwrap_or("-"),
                    content.meta_description,
                    content.og_image.as_deref().unwrap_or("-"),
                    headings,
                    content.links.len(),
                    content.images.len(),
                    content_preview,
                    image_preview_section,
                    sources_section
                )
            };

            Ok(Json(McpCallResponse {
                content: vec![McpContent {
                    content_type: "text".to_string(),
                    text: content_text,
                }],
                is_error: false,
            }))
        }
        Err(e) => {
            error!("Scrape tool error: {}", e);
            Ok(Json(McpCallResponse {
                content: vec![McpContent {
                    content_type: "text".to_string(),
                    text: format!("Scraping failed: {}", e),
                }],
                is_error: true,
            }))
        }
    }
}
