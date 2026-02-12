use crate::extract;
use crate::mcp::{McpCallResponse, McpContent};
use crate::types::{ErrorResponse, ExtractField};
use crate::AppState;
use axum::http::StatusCode;
use axum::response::Json;
use regex::Regex;
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
                            field_map.insert("name".to_string(), serde_json::Value::String(key.clone()));
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

fn parse_schema_from_prompt(prompt: &str) -> Option<Vec<ExtractField>> {
    let trimmed = prompt.trim();
    if trimmed.is_empty() {
        return None;
    }

    let candidate = if let Some(rest) = trimmed.strip_prefix("schema:") {
        rest.trim()
    } else {
        trimmed
    };

    let json_snippet = if let (Some(start), Some(end)) = (candidate.find('['), candidate.rfind(']')) {
        candidate.get(start..=end)
    } else if candidate.starts_with('{') && candidate.ends_with('}') {
        Some(candidate)
    } else {
        None
    };

    let snippet = json_snippet?;
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(snippet) {
        return parse_extract_schema(Some(&parsed));
    }

    let normalized = snippet.replace("\\\"", "\"");
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&normalized) {
        return parse_extract_schema(Some(&parsed));
    }

    let name_re = Regex::new(r#"\bname\b[^a-zA-Z0-9_-]*([a-zA-Z0-9_-]+)"#).unwrap();
    let mut fields = Vec::new();
    for cap in name_re.captures_iter(&normalized) {
        if let Some(name) = cap.get(1).map(|m| m.as_str().to_string()) {
            fields.push(ExtractField {
                name: name.clone(),
                description: name,
                field_type: None,
                required: None,
            });
        }
    }

    if fields.is_empty() {
        None
    } else {
        Some(fields)
    }
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
    let mut schema = parse_extract_schema(schema_value);

    let mut prompt = arguments
        .get("prompt")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    if schema.is_none() {
        if let Some(prompt_text) = prompt.as_deref() {
            if let Some(schema_from_prompt) = parse_schema_from_prompt(prompt_text) {
                schema = Some(schema_from_prompt);
                prompt = None;
            }
        }
    }

    let max_chars = arguments
        .get("max_chars")
        .and_then(|v| v.as_u64())
        .map(|n| n as usize);

    let use_proxy = arguments
        .get("use_proxy")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    match extract::extract_structured(&state, url, schema, prompt, max_chars, use_proxy).await {
        Ok(response) => {
            let json_str = serde_json::to_string_pretty(&response)
                .unwrap_or_else(|e| format!(r#"{{"error": "Failed to serialize: {}"}}"#, e));
            Ok(Json(McpCallResponse {
                content: vec![McpContent {
                    content_type: "text".to_string(),
                    text: json_str,
                }],
                is_error: false,
            }))
        }
        Err(e) => {
            error!("Extract tool error: {}", e);
            Ok(Json(McpCallResponse {
                content: vec![McpContent {
                    content_type: "text".to_string(),
                    text: format!("Extract failed: {}", e),
                }],
                is_error: true,
            }))
        }
    }
}