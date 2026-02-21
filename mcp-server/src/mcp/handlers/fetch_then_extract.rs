use super::common::parse_quality_mode;
use crate::extract;
use crate::mcp::{McpCallResponse, McpContent};
use crate::types::{ErrorResponse, ExtractField};
use crate::{scrape, AppState};
use axum::http::StatusCode;
use axum::response::Json;
use serde_json::Value;
use std::sync::Arc;
use tracing::error;

fn parse_extract_schema(schema_value: Option<&serde_json::Value>) -> Option<Vec<ExtractField>> {
    fn parse_field(obj: &serde_json::Map<String, serde_json::Value>) -> Option<ExtractField> {
        let name = obj
            .get("name")
            .or_else(|| obj.get("field"))
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())?;
        if name.is_empty() {
            return None;
        }

        let description = obj
            .get("description")
            .or_else(|| obj.get("desc"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| name.clone());

        let field_type = obj
            .get("field_type")
            .or_else(|| obj.get("type"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let required = obj.get("required").and_then(|v| v.as_bool());

        Some(ExtractField {
            name,
            description,
            field_type,
            required,
        })
    }

    fn parse_array(arr: &[serde_json::Value]) -> Vec<ExtractField> {
        arr.iter()
            .filter_map(|item| item.as_object().and_then(parse_field))
            .collect()
    }

    let schema_value = schema_value?;
    let parsed = match schema_value {
        serde_json::Value::Array(arr) => parse_array(arr),
        serde_json::Value::Object(obj) => {
            if let Some(arr) = obj.get("fields").and_then(|v| v.as_array()) {
                parse_array(arr)
            } else if obj.get("name").is_some() {
                parse_field(obj).into_iter().collect()
            } else {
                let mut fields = Vec::new();
                for (key, value) in obj {
                    if key.starts_with('_') {
                        continue;
                    }
                    match value {
                        serde_json::Value::String(field_type) => {
                            fields.push(ExtractField {
                                name: key.clone(),
                                description: key.clone(),
                                field_type: Some(field_type.clone()),
                                required: None,
                            });
                        }
                        serde_json::Value::Object(field_obj) => {
                            let mut field_map = field_obj.clone();
                            field_map
                                .insert("name".to_string(), serde_json::Value::String(key.clone()));
                            if let Some(field) = parse_field(&field_map) {
                                fields.push(field);
                            }
                        }
                        _ => {}
                    }
                }
                fields
            }
        }
        serde_json::Value::String(raw) => {
            let parsed_json = serde_json::from_str::<serde_json::Value>(raw).ok();
            if let Some(value) = parsed_json.as_ref() {
                return parse_extract_schema(Some(value));
            }
            Vec::new()
        }
        _ => Vec::new(),
    };

    if parsed.is_empty() {
        None
    } else {
        Some(parsed)
    }
}

/// Returns `true` when the URL points to a raw text/markdown file where schema
/// extraction is unreliable (fields typically return null, confidence is low).
fn is_raw_content_url(url: &str) -> bool {
    let path_only = url.split('?').next().unwrap_or(url).to_ascii_lowercase();
    let ext = path_only.rsplit('.').next().unwrap_or("");
    matches!(
        ext,
        "md" | "mdx" | "rst" | "txt" | "csv" | "toml" | "yaml" | "yml"
    )
}

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

    let schema_value = arguments.get("schema");
    let schema = parse_extract_schema(schema_value);

    let prompt = arguments
        .get("prompt")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let strict = arguments
        .get("strict")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let max_chars = arguments
        .get("max_chars")
        .and_then(|v| v.as_u64())
        .map(|n| n as usize)
        .unwrap_or(10000);

    let placeholder_word_threshold = arguments
        .get("placeholder_word_threshold")
        .and_then(|v| v.as_u64())
        .map(|n| n as usize);

    let placeholder_empty_ratio = arguments
        .get("placeholder_empty_ratio")
        .and_then(|v| v.as_f64());

    let use_proxy = arguments
        .get("use_proxy")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let quality_mode = parse_quality_mode(arguments)?;

    let output_format = arguments
        .get("output_format")
        .and_then(|v| v.as_str())
        .unwrap_or("json");

    let options = scrape::ScrapeUrlOptions {
        use_proxy,
        quality_mode: Some(quality_mode),
        query: None,
        strict_relevance: false,
        relevance_threshold: None,
        extract_app_state: false,
        extract_relevant_sections: false,
        section_limit: None,
        section_threshold: None,
    };

    match scrape::scrape_url_full(&state, url, options).await {
        Ok(mut content) => {
            crate::content_quality::apply_scrape_content_limit(&mut content, max_chars, false);
            // FIX #4 — Schema Validation: Warn when URL is a raw markdown/text file.
            if is_raw_content_url(url) {
                crate::content_quality::push_warning_unique(
                    &mut content.warnings,
                    "raw_markdown_url: Extraction on raw .md/.mdx/.rst/.txt files is unreliable \
                     — fields may return null and confidence will be low. \
                     Recommended: use web_fetch with output_format: clean_json for raw Markdown \
                     sources, or web_fetch (text mode) to read the raw content directly.",
                );
            }

            // Strict-mode + schema-first extraction on the already-scraped content.
            let response =
                match extract::extract_from_scrape(&content, schema, prompt, strict, max_chars, placeholder_word_threshold, placeholder_empty_ratio) {
                    Ok(r) => r,
                    Err(e) => {
                        return Ok(Json(McpCallResponse {
                            content: vec![McpContent {
                                content_type: "text".to_string(),
                                text: format!("fetch_then_extract failed: {}", e),
                            }],
                            is_error: true,
                        }))
                    }
                };

            if output_format == "text" {
                return Ok(Json(McpCallResponse {
                    content: vec![McpContent {
                        content_type: "text".to_string(),
                        text: serde_json::to_string_pretty(&response.extracted_data)
                            .unwrap_or_else(|_| "{}".to_string()),
                    }],
                    is_error: false,
                }));
            }

            let json_str = serde_json::to_string_pretty(&response)
                .unwrap_or_else(|e| format!(r#"{{\"error\": \"Failed to serialize: {}\"}}"#, e));
            Ok(Json(McpCallResponse {
                content: vec![McpContent {
                    content_type: "text".to_string(),
                    text: json_str,
                }],
                is_error: false,
            }))
        }
        Err(e) => {
            error!("fetch_then_extract scrape error: {}", e);
            Ok(Json(McpCallResponse {
                content: vec![McpContent {
                    content_type: "text".to_string(),
                    text: format!("fetch_then_extract failed: {}", e),
                }],
                is_error: true,
            }))
        }
    }
}
