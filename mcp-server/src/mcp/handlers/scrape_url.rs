use super::common::parse_quality_mode;
use crate::mcp::{McpCallResponse, McpContent};
use crate::rust_scraper::QualityMode;
use crate::types::{
    AuthWallBlocked, CodeBlock, ErrorResponse, SniperCodeBlock, SniperMetadata, SniperOutput,
};
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

    // 🧬 Semantic Shaving parameters
    let query = arguments
        .get("query")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let strict_relevance = arguments
        .get("strict_relevance")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let relevance_threshold = arguments
        .get("relevance_threshold")
        .and_then(|v| v.as_f64())
        .map(|f| f as f32);

    // Optional: short, query-matched output (section-only)
    let extract_relevant_sections = arguments
        .get("extract_relevant_sections")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let section_limit = arguments
        .get("section_limit")
        .and_then(|v| v.as_u64())
        .map(|n| n as usize);
    let section_threshold = arguments
        .get("section_threshold")
        .and_then(|v| v.as_f64())
        .map(|f| f as f32);

    // 🧬 Rule C: by default the SPA JSON fast-path falls back to readability when
    // content is too sparse.  Set `extract_app_state=true` to force-return the
    // raw embedded JSON (Next.js __NEXT_DATA__, Nuxt __NUXT_DATA__, etc.).
    let extract_app_state = arguments
        .get("extract_app_state")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let options = scrape::ScrapeUrlOptions {
        use_proxy,
        quality_mode: Some(quality_mode),
        query,
        strict_relevance,
        relevance_threshold,
        extract_app_state,
        extract_relevant_sections,
        section_limit,
        section_threshold,
    };

    match scrape::scrape_url_full(&state, url, options).await {
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

            // Dynamic thresholds — agents can tune per-task instead of using fixed values.
            let short_content_threshold = arguments
                .get("short_content_threshold")
                .and_then(|v| v.as_u64())
                .map(|n| n as usize)
                .unwrap_or(50);
            let extraction_score_threshold = arguments
                .get("extraction_score_threshold")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.4);
            // Dynamic output-shaping for text mode.
            let max_headings = arguments
                .get("max_headings")
                .and_then(|v| v.as_u64())
                .map(|n| n as usize)
                .unwrap_or(10);
            let max_images = arguments
                .get("max_images")
                .and_then(|v| v.as_u64())
                .map(|n| n as usize)
                .unwrap_or(3);

            crate::content_quality::apply_scrape_content_limit(&mut content, max_chars, false);
            if content.word_count < short_content_threshold {
                crate::content_quality::push_warning_unique(&mut content.warnings, "short_content");
            }
            if content.extraction_score.map(|s| s < extraction_score_threshold).unwrap_or(false) {
                crate::content_quality::push_warning_unique(
                    &mut content.warnings,
                    "low_extraction_score",
                );
            }

            let output_format = arguments
                .get("output_format")
                .and_then(|v| v.as_str())
                .unwrap_or("text");

            // ────────────────────────────────────────────────────────────────────────
            // 🔒 Auth-Wall Early Exit — Feature 2 + HITL Integration
            // When an auth-wall is confirmed, NEVER return a garbled/empty page.
            // Instead surface a structured response or an agent-ready HITL prompt.
            // ────────────────────────────────────────────────────────────────────────
            let is_auth_walled = content.auth_wall_reason.is_some()
                || content.warnings.iter().any(|w| w == "content_restricted");

            if is_auth_walled {
                let reason = content
                    .auth_wall_reason
                    .as_deref()
                    .unwrap_or("Auth-Wall detected (login page returned HTTP 200)")
                    .to_string();

                // Auto-compute GitHub raw URL hint for blob pages.
                // NOTE: rewrite_url_for_clean_content in scrape.rs already rewrites
                // /blob/ → raw before the scrape, so this is an informational hint only.
                let github_raw_url = if url.contains("github.com") && url.contains("/blob/") {
                    let raw = url
                        .replace("github.com/", "raw.githubusercontent.com/")
                        .replacen("/blob/", "/", 1);
                    Some(raw)
                } else {
                    None
                };

                if output_format == "text" {
                    // 💬 HITL Escalation Message — agent-friendly interactive prompt
                    let hitl_text = format!(
                        concat!(
                            "🔒 **Auth-Wall Detected**\n\n",
                            "**URL:** {url}\n",
                            "**Reason:** {reason}\n",
                            "{raw_hint}",
                            "\n---\n",
                            "💬 **Agent Recommendation**\n\n",
                            "Found an auth-wall on `{url_short}`. ",
                            "Should I escalate to HITL (Human-In-The-Loop) to bypass this?\n\n",
                            "• Use the `non_robot_search` tool to open a real browser and log in manually\n",
                            "• Set credentials via environment variables (e.g. `GITHUB_TOKEN` for GitHub)\n",
                            "• Retry with `use_proxy: true` if the site geo-blocks your IP\n",
                            "• For GitHub private repos: authenticate via SSH or a Personal Access Token"
                        ),
                        url = url,
                        reason = reason,
                        raw_hint = github_raw_url
                            .as_deref()
                            .map(|r| format!("**GitHub Raw URL (attempted):** {}\n", r))
                            .unwrap_or_default(),
                        url_short = url,
                    );
                    return Ok(Json(McpCallResponse {
                        content: vec![McpContent {
                            content_type: "text".to_string(),
                            text: hitl_text,
                        }],
                        is_error: true,
                    }));
                } else {
                    // 🎯 Structured blocked_by_auth JSON — for json / clean_json modes
                    let blocked = AuthWallBlocked {
                        status: "NEED_HITL".to_string(),
                        reason,
                        url: url.to_string(),
                        suggested_action: "non_robot_search".to_string(),
                        github_raw_url,
                    };
                    let json_str = serde_json::to_string_pretty(&blocked).unwrap_or_else(|e| {
                        format!(r#"{{"error": "Failed to serialize: {}"}}"#, e)
                    });
                    return Ok(Json(McpCallResponse {
                        content: vec![McpContent {
                            content_type: "text".to_string(),
                            text: json_str,
                        }],
                        is_error: true,
                    }));
                }
            }

            if output_format == "json" {
                let mut include_raw_html = arguments
                    .get("include_raw_html")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                // 🧬 Task 1: Force-override — never return raw HTML when NeuroSiphon is
                // active or quality_mode is aggressive.  Returning raw HTML under these
                // modes is a massive token leak that defeats the token-saving architecture.
                if crate::core::config::neurosiphon_enabled()
                    || quality_mode == QualityMode::Aggressive
                {
                    include_raw_html = false;
                }

                let mut json_content = content.clone();
                if !include_raw_html {
                    json_content.content.clear();
                    crate::content_quality::push_warning_unique(
                        &mut json_content.warnings,
                        "raw_html_omitted_in_scrape_url_json",
                    );
                }

                let mut json_str = serde_json::to_string_pretty(&json_content)
                    .unwrap_or_else(|e| format!(r#"{{"error": "Failed to serialize: {}"}}"#, e));
                // FIX #1 — max_chars caps the TOTAL serialized JSON payload, not just the text
                // extraction field. A CDP-rendered page can balloon to 93KB even with a 3000-char
                // text limit because links[], images[], code_blocks[] are NOT bounded by text cap.
                if json_str.len() > max_chars {
                    let full_kb = json_str.len() / 1024;
                    json_str.truncate(max_chars);
                    json_str.push_str(&format!(
                        "\n// \u{26a0}\u{fe0f} JSON_PAYLOAD_TRUNCATED: full response was ~{}KB, capped at {} chars. \
                         Use output_format: clean_json for token-efficient output, or increase max_chars.",
                        full_kb, max_chars
                    ));
                }
                return Ok(Json(McpCallResponse {
                    content: vec![McpContent {
                        content_type: "text".to_string(),
                        text: json_str,
                    }],
                    is_error: false,
                }));
            }

            // 🎯 Sniper Mode — Token-optimised clean JSON output.
            // Returns only title, substantive paragraphs, code blocks, and metadata.
            // Strips 100 % of headers, footers, nav menus, and boilerplate noise.
            if output_format == "clean_json" {
                // FIX #2 — Media-Aware Auto-detection for raw .md / .mdx / .rst / .txt URLs.
                // HTML extraction on raw text produces duplicate frontmatter appearing in BOTH
                // key_paragraphs AND key_code_blocks. Skip the noisy HTML pipeline entirely.
                if is_raw_content_url(url) {
                    let para_budget = max_chars.saturating_sub(1000).max(500);
                    let mut raw_para_total = 0usize;
                    let key_paragraphs: Vec<String> = content
                        .clean_content
                        .split("\n\n")
                        .map(|p| p.trim().to_string())
                        .filter(|p| !p.is_empty())
                        .filter(|p| {
                            if raw_para_total >= para_budget {
                                return false;
                            }
                            raw_para_total += p.len() + 4;
                            true
                        })
                        .collect();
                    let mut raw_warnings = content.warnings.clone();
                    crate::content_quality::push_warning_unique(
                        &mut raw_warnings,
                        "raw_markdown_url: HTML extraction skipped — content returned as-is \
                         to avoid duplication. For schema extraction use extract_structured.",
                    );
                    let sniper = SniperOutput {
                        title: content.title.clone(),
                        key_points: vec![
                            "Raw text/markdown file — returned as-is (HTML extraction skipped)."
                                .to_string(),
                        ],
                        key_paragraphs,
                        key_code_blocks: vec![],
                        metadata: SniperMetadata {
                            url: content.url.clone(),
                            author: content.author.clone(),
                            published_at: content.published_at.clone(),
                            word_count: content.word_count,
                            extraction_score: content.extraction_score,
                            warnings: raw_warnings,
                        },
                    };
                    let json_str = serde_json::to_string_pretty(&sniper)
                        .unwrap_or_else(|e| format!(r#"{{"error": "Failed to serialize: {}"}}"#, e));
                    return Ok(Json(McpCallResponse {
                        content: vec![McpContent {
                            content_type: "text".to_string(),
                            text: json_str,
                        }],
                        is_error: false,
                    }));
                }

                let noise_terms: &[&str] = &[
                    "terms of service",
                    "privacy policy",
                    "cookie policy",
                    "all rights reserved",
                    "copyright ©",
                    "© ",
                    "follow us on",
                    "subscribe to our",
                    "unsubscribe",
                    "powered by",
                ];

                // BUG-3a: On short pages (< 200 words — API index, reference stubs, etc.)
                // the standard 8-word minimum discards everything.  Lower to 3 so that
                // brief item descriptions on docs.rs module indexes are preserved.
                let para_min_words: usize = if content.word_count < 200 { 3 } else { 8 };

                let key_paragraphs_all: Vec<String> = content
                    .clean_content
                    .split("\n\n")
                    .filter_map(|para| {
                        let trimmed = para.trim();
                        let lower = trimmed.to_lowercase();
                        // Skip very short paragraphs (nav stubs, orphan headings, etc.)
                        if trimmed.split_whitespace().count() < para_min_words {
                            return None;
                        }
                        // Skip paragraphs dominated by boilerplate keywords
                        if noise_terms.iter().any(|t| lower.contains(*t)) {
                            return None;
                        }
                        Some(trimmed.to_string())
                    })
                    .collect();

                // BUG-3b: Apply max_chars budget to key_paragraphs to prevent unbounded
                // serialisation on large pages (was spilling to workspace storage at 33KB).
                // Reserve ~2 KB for metadata, code blocks, and JSON framing.
                let para_budget = max_chars.saturating_sub(2000).max(500);
                let mut para_total = 0usize;
                let mut paragraphs_truncated = false;
                let key_paragraphs: Vec<String> = key_paragraphs_all
                    .into_iter()
                    .filter(|p| {
                        if para_total >= para_budget {
                            paragraphs_truncated = true;
                            return false;
                        }
                        para_total += p.len() + 4; // +4 for JSON comma/quote overhead
                        true
                    })
                    .collect();

                let key_code_blocks =
                    extract_contextual_code_blocks(&content.clean_content, &content.code_blocks);

                // Build key_points: first sentence of each key_paragraph.
                // Ultra-compact overview — agents read this before the full paragraphs.
                let key_points: Vec<String> = key_paragraphs
                    .iter()
                    .filter_map(|para| {
                        let sentence_end = para
                            .char_indices()
                            .find(|(_, c)| matches!(*c, '.' | '!' | '?'))
                            .map(|(i, c)| i + c.len_utf8());
                        let point = match sentence_end {
                            Some(end) => para[..end].trim().to_string(),
                            None => {
                                let s: String = para.chars().take(120).collect();
                                if s.len() < para.len() {
                                    format!("{}\u{2026}", s.trim())
                                } else {
                                    s.trim().to_string()
                                }
                            }
                        };
                        if point.split_whitespace().count() >= 4 {
                            Some(point)
                        } else {
                            None
                        }
                    })
                    .collect();

                let mut sniper_warnings = content.warnings.clone();
                if paragraphs_truncated {
                    crate::content_quality::push_warning_unique(
                        &mut sniper_warnings,
                        &format!(
                            "clean_json_truncated: key_paragraphs limited to ~{} chars; increase max_chars for full output",
                            max_chars
                        ),
                    );
                }

                let sniper = SniperOutput {
                    title: content.title.clone(),
                    key_points,
                    key_paragraphs,
                    key_code_blocks,
                    metadata: SniperMetadata {
                        url: content.url.clone(),
                        author: content.author.clone(),
                        published_at: content.published_at.clone(),
                        word_count: content.word_count,
                        extraction_score: content.extraction_score,
                        warnings: sniper_warnings,
                    },
                };

                let mut json_str = serde_json::to_string_pretty(&sniper)
                    .unwrap_or_else(|e| format!(r#"{{"error": "Failed to serialize: {}"}}"#, e));
                // FIX #1 (clean_json): Also apply max_chars to total serialized payload.
                if json_str.len() > max_chars {
                    let full_kb = json_str.len() / 1024;
                    json_str.truncate(max_chars);
                    json_str.push_str(&format!(
                        "\n// \u{26a0}\u{fe0f} CLEAN_JSON_PAYLOAD_TRUNCATED: ~{}KB \u{2192} capped at {} chars. \
                         Increase max_chars for full output.",
                        full_kb, max_chars,
                    ));
                }
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
                    max_images,
                );

                let headings = content
                    .headings
                    .iter()
                    .take(max_headings)
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

/// Returns `true` when the URL points to a raw text/data file where HTML extraction
/// is unhelpful and produces noisy/duplicate content (Fix #2 — Media-Aware Auto-detection).
fn is_raw_content_url(url: &str) -> bool {
    let path_only = url.split('?').next().unwrap_or(url).to_ascii_lowercase();
    let ext = path_only.rsplit('.').next().unwrap_or("");
    matches!(
        ext,
        "md" | "mdx" | "rst" | "txt" | "csv" | "toml" | "yaml" | "yml"
    )
}

/// Extract code blocks from Markdown with surrounding prose as context.
///
/// **Pass 1** — fenced ` ``` ` blocks: `context` = last non-heading prose line above the
/// opening fence.
/// **Pass 2** — inline / raw `CodeBlock` objects not already captured in Pass 1:
/// `context` = the Markdown line hosting the code (backtick-delimited or bare).
fn extract_contextual_code_blocks(
    markdown: &str,
    raw_blocks: &[CodeBlock],
) -> Vec<SniperCodeBlock> {
    use std::collections::HashSet;
    let mut seen: HashSet<String> = HashSet::new();
    let mut results: Vec<SniperCodeBlock> = Vec::new();
    let lines: Vec<&str> = markdown.lines().collect();

    // ── Pass 1: fenced code blocks ─────────────────────────────────────────────
    let mut i = 0;
    while i < lines.len() {
        let t = lines[i].trim();
        let delim = if t.starts_with("```") {
            "```"
        } else if t.starts_with("~~~") {
            "~~~"
        } else {
            i += 1;
            continue;
        };
        let lang_str = t[delim.len()..].trim().to_string();
        let language = if lang_str.is_empty() {
            None
        } else {
            Some(lang_str)
        };
        // Context = last non-empty non-heading non-fence prose line before this fence.
        let context = (0..i)
            .rev()
            .map(|j| lines[j].trim())
            .find(|l| {
                !l.is_empty()
                    && !l.starts_with('#')
                    && !l.starts_with('`')
                    && !l.starts_with('~')
                    && !l.starts_with("---")
            })
            .map(prose_tail);
        // Gather code body until the matching closing fence.
        i += 1;
        let mut body: Vec<&str> = Vec::new();
        while i < lines.len() && !lines[i].trim().starts_with(delim) {
            body.push(lines[i]);
            i += 1;
        }
        let code = body.join("\n").trim().to_string();
        if code.len() >= 3 && seen.insert(code.clone()) {
            results.push(SniperCodeBlock {
                language,
                context,
                code,
            });
        }
        i += 1;
    }

    // ── Pass 2: inline / raw code objects not already captured ──────────────────
    for cb in raw_blocks {
        let code = cb.code.trim().to_string();
        if code.len() < 2 || !seen.insert(code.clone()) {
            continue;
        }
        // Find the Markdown line hosting this code snippet.
        let context = lines.iter().find_map(|line| {
            if line.contains(code.as_str()) {
                let stripped = line
                    .trim()
                    .replace(&format!("`{}`", code), code.as_str())
                    .replace(&format!("``{}``", code), code.as_str());
                let ctx = stripped.trim().to_string();
                // Only surface context when the line carries more than just the code itself.
                if ctx.len() > code.len() + 2 && ctx.split_whitespace().count() >= 3 {
                    Some(ctx)
                } else {
                    None
                }
            } else {
                None
            }
        });
        results.push(SniperCodeBlock {
            language: cb.language.clone(),
            context,
            code,
        });
    }

    results
}

/// Return the final sentence of `s` (text after `". "`), or the full string capped at 180 chars.
fn prose_tail(s: &str) -> String {
    if let Some(pos) = s.rfind(". ") {
        let after = s[pos + 2..].trim();
        if after.split_whitespace().count() >= 3 {
            return after.to_string();
        }
    }
    let s = s.trim();
    if s.len() <= 180 {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(180).collect();
        format!("{}\u{2026}", truncated.trim_end())
    }
}
