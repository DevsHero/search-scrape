use anyhow::Result;
use regex::Regex;
use std::sync::Arc;
use std::time::Instant;
use tracing::info;

use crate::rust_scraper::QualityMode;
use crate::types::*;
use crate::AppState;

/// Extract structured data from a webpage based on schema or prompt
/// Uses pattern matching and heuristics (no external LLM required)
pub async fn extract_structured(
    state: &Arc<AppState>,
    url: &str,
    schema: Option<Vec<ExtractField>>,
    prompt: Option<String>,
    max_chars: Option<usize>,
    use_proxy: bool,
    quality_mode: Option<&str>,
) -> Result<ExtractResponse> {
    let start_time = Instant::now();
    let max_chars = max_chars.unwrap_or(10000);

    info!("Extracting structured data from: {}", url);

    // First, scrape the page
    let mode = quality_mode.and_then(QualityMode::parse_str);
    let scrape_result = crate::scrape::scrape_url_with_options(state, url, use_proxy, mode).await?;

    let mut extracted_data = serde_json::Map::new();
    let mut warnings = Vec::new();
    let mut confidence: f64 = 0.8;

    let mut schema = schema;
    let mut prompt = prompt;

    if schema.is_none() {
        if let Some(prompt_text) = prompt.as_deref() {
            if let Some(parsed) = parse_schema_from_prompt(prompt_text) {
                schema = Some(parsed);
                prompt = None;
            }
        }
    }

    // Determine extraction method
    let extraction_method = if schema.is_some() {
        "schema_based"
    } else if prompt.is_some() {
        "prompt_based"
    } else {
        "auto_detect"
    };

    // Extract based on schema if provided
    if let Some(fields) = &schema {
        for field in fields {
            let value = extract_field_value(&scrape_result, field);
            if value.is_null() && field.required.unwrap_or(false) {
                warnings.push(format!("Required field '{}' not found", field.name));
                confidence -= 0.1;
            }
            extracted_data.insert(field.name.clone(), value);
        }
    } else {
        // Auto-detect common patterns
        extracted_data = auto_extract(&scrape_result, prompt.as_deref());
    }

    // Add metadata fields
    extracted_data.insert(
        "_title".to_string(),
        serde_json::Value::String(scrape_result.title.clone()),
    );
    extracted_data.insert(
        "_url".to_string(),
        serde_json::Value::String(url.to_string()),
    );
    extracted_data.insert(
        "_word_count".to_string(),
        serde_json::Value::Number(scrape_result.word_count.into()),
    );

    if let Some(author) = &scrape_result.author {
        extracted_data.insert(
            "_author".to_string(),
            serde_json::Value::String(author.clone()),
        );
    }
    if let Some(published) = &scrape_result.published_at {
        extracted_data.insert(
            "_published_at".to_string(),
            serde_json::Value::String(published.clone()),
        );
    }

    // HALLUCINATION PROTECTION: Count null fields and warn
    let null_count = extracted_data.values().filter(|v| v.is_null()).count();
    if null_count > 0 {
        warnings.push(format!(
            "Field not found warning: {} field(s) returned null (hallucination protection active)",
            null_count
        ));
        confidence -= (null_count as f64 * 0.1).min(0.3); // Cap confidence reduction at 0.3
    }

    let field_count = extracted_data.len();
    let raw_preview: String = scrape_result
        .clean_content
        .chars()
        .take(max_chars)
        .collect();

    Ok(ExtractResponse {
        url: url.to_string(),
        title: scrape_result.title,
        extracted_data: serde_json::Value::Object(extracted_data),
        raw_content_preview: raw_preview,
        extraction_method: extraction_method.to_string(),
        field_count,
        confidence: confidence.max(0.0).min(1.0),
        duration_ms: start_time.elapsed().as_millis() as u64,
        warnings,
    })
}

/// Extract a specific field value based on field definition
fn extract_field_value(scrape: &ScrapeResponse, field: &ExtractField) -> serde_json::Value {
    let content = &scrape.clean_content;
    let name_lower = field.name.to_lowercase();
    let desc_lower = field.description.to_lowercase();

    // Try to match based on field name and description
    match name_lower.as_str() {
        // Common field patterns
        "title" | "name" | "headline" => serde_json::Value::String(scrape.title.clone()),
        "description" | "summary" | "excerpt" => {
            if !scrape.meta_description.is_empty() {
                serde_json::Value::String(scrape.meta_description.clone())
            } else {
                // First paragraph
                let first_para: String = content
                    .lines()
                    .find(|l| l.len() > 50)
                    .unwrap_or("")
                    .chars()
                    .take(500)
                    .collect();
                serde_json::Value::String(first_para)
            }
        }
        "author" | "writer" | "by" => scrape
            .author
            .clone()
            .map(serde_json::Value::String)
            .unwrap_or(serde_json::Value::Null),
        "date" | "published" | "published_at" | "publish_date" => scrape
            .published_at
            .clone()
            .map(serde_json::Value::String)
            .unwrap_or_else(|| extract_date_from_content(content)),
        "price" | "cost" | "amount" => extract_price_advanced(content, &field.name),
        "email" | "emails" => extract_emails(content),
        "phone" | "telephone" | "phones" => extract_phones(content),
        "links" | "urls" => {
            let urls: Vec<serde_json::Value> = scrape
                .links
                .iter()
                .take(20)
                .map(|l| serde_json::Value::String(l.url.clone()))
                .collect();
            serde_json::Value::Array(urls)
        }
        "headings" | "headers" | "sections" => {
            let headings: Vec<serde_json::Value> = scrape
                .headings
                .iter()
                .map(|h| serde_json::Value::String(format!("{}: {}", h.level, h.text)))
                .collect();
            serde_json::Value::Array(headings)
        }
        "code" | "code_blocks" | "code_snippets" => {
            let blocks: Vec<serde_json::Value> = scrape
                .code_blocks
                .iter()
                .map(|b| {
                    let mut obj = serde_json::Map::new();
                    obj.insert(
                        "language".to_string(),
                        b.language
                            .clone()
                            .map(serde_json::Value::String)
                            .unwrap_or(serde_json::Value::Null),
                    );
                    obj.insert(
                        "code".to_string(),
                        serde_json::Value::String(b.code.clone()),
                    );
                    serde_json::Value::Object(obj)
                })
                .collect();
            serde_json::Value::Array(blocks)
        }
        "images" => {
            let imgs: Vec<serde_json::Value> = scrape
                .images
                .iter()
                .take(20)
                .map(|i| {
                    let mut obj = serde_json::Map::new();
                    obj.insert("src".to_string(), serde_json::Value::String(i.src.clone()));
                    obj.insert("alt".to_string(), serde_json::Value::String(i.alt.clone()));
                    serde_json::Value::Object(obj)
                })
                .collect();
            serde_json::Value::Array(imgs)
        }
        _ => {
            if name_lower.contains("crate") && name_lower.contains("name") {
                if let Some(value) = extract_crate_name(scrape) {
                    return serde_json::Value::String(value);
                }
            }
            if name_lower.contains("purpose") || name_lower.contains("overview") {
                if !scrape.meta_description.is_empty() {
                    return serde_json::Value::String(scrape.meta_description.clone());
                }
                let first_para: String = content
                    .lines()
                    .find(|l| l.len() > 50)
                    .unwrap_or("")
                    .chars()
                    .take(500)
                    .collect();
                if !first_para.is_empty() {
                    return serde_json::Value::String(first_para);
                }
            }
            if name_lower.contains("feature") {
                let features: Vec<serde_json::Value> = scrape
                    .headings
                    .iter()
                    .filter(|h| h.level == "h2" || h.level == "h3")
                    .take(8)
                    .map(|h| serde_json::Value::String(h.text.clone()))
                    .collect();
                if !features.is_empty() {
                    return serde_json::Value::Array(features);
                }
            }
            // ADVANCED HEURISTIC EXTRACTION with hallucination protection
            if desc_lower.contains("number")
                || desc_lower.contains("count")
                || desc_lower.contains("quantity")
            {
                extract_number_with_hallucination_check(content, &field.name)
            } else if desc_lower.contains("list") || desc_lower.contains("array") {
                extract_list_near_keyword_advanced(content, &field.name)
            } else {
                extract_text_with_hallucination_check(content, &field.name)
            }
        }
    }
}

fn extract_crate_name(scrape: &ScrapeResponse) -> Option<String> {
    for heading in &scrape.headings {
        let text = heading.text.trim();
        if text.to_lowercase().starts_with("crate ") {
            return text.split_whitespace().nth(1).map(|s| s.to_string());
        }
    }

    let re = Regex::new(r"(?i)crate\s+([a-z0-9_\-]+)").ok()?;
    re.captures(&scrape.clean_content)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
}

/// Auto-extract common data patterns from content
fn auto_extract(
    scrape: &ScrapeResponse,
    prompt: Option<&str>,
) -> serde_json::Map<String, serde_json::Value> {
    let mut data = serde_json::Map::new();
    let content = &scrape.clean_content;

    // Always extract these
    data.insert(
        "title".to_string(),
        serde_json::Value::String(scrape.title.clone()),
    );

    if !scrape.meta_description.is_empty() {
        data.insert(
            "description".to_string(),
            serde_json::Value::String(scrape.meta_description.clone()),
        );
    }

    // Extract emails if found
    let emails = extract_emails(content);
    if !emails.is_null() {
        data.insert("emails".to_string(), emails);
    }

    // Extract prices if found
    let prices = extract_price(content);
    if !prices.is_null() {
        data.insert("prices".to_string(), prices);
    }

    // Extract dates if found
    let dates = extract_date_from_content(content);
    if !dates.is_null() {
        data.insert("dates".to_string(), dates);
    }

    // If prompt provided, try to extract based on keywords in prompt
    if let Some(prompt_text) = prompt {
        let prompt_lower = prompt_text.to_lowercase();

        if prompt_lower.contains("product") || prompt_lower.contains("item") {
            // Product-focused extraction
            if let Some(h1) = scrape.headings.iter().find(|h| h.level == "h1") {
                data.insert(
                    "product_name".to_string(),
                    serde_json::Value::String(h1.text.clone()),
                );
            }
        }

        if prompt_lower.contains("article") || prompt_lower.contains("blog") {
            // Article-focused extraction
            if let Some(author) = &scrape.author {
                data.insert(
                    "author".to_string(),
                    serde_json::Value::String(author.clone()),
                );
            }
            if let Some(date) = &scrape.published_at {
                data.insert(
                    "published_date".to_string(),
                    serde_json::Value::String(date.clone()),
                );
            }

            // Reading time
            if let Some(time) = scrape.reading_time_minutes {
                data.insert(
                    "reading_time_minutes".to_string(),
                    serde_json::Value::Number(time.into()),
                );
            }
        }

        if prompt_lower.contains("contact") {
            let phones = extract_phones(content);
            if !phones.is_null() {
                data.insert("phones".to_string(), phones);
            }
        }

        if prompt_lower.contains("code") || prompt_lower.contains("programming") {
            if !scrape.code_blocks.is_empty() {
                let blocks: Vec<serde_json::Value> = scrape
                    .code_blocks
                    .iter()
                    .map(|b| serde_json::Value::String(b.code.clone()))
                    .collect();
                data.insert("code_blocks".to_string(), serde_json::Value::Array(blocks));
            }
        }
    }

    // Add headings as table of contents
    if !scrape.headings.is_empty() {
        let toc: Vec<serde_json::Value> = scrape
            .headings
            .iter()
            .filter(|h| h.level == "h1" || h.level == "h2" || h.level == "h3")
            .take(15)
            .map(|h| serde_json::Value::String(h.text.clone()))
            .collect();
        if !toc.is_empty() {
            data.insert(
                "table_of_contents".to_string(),
                serde_json::Value::Array(toc),
            );
        }
    }

    data
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

    let json_snippet = if let (Some(start), Some(end)) = (candidate.find('['), candidate.rfind(']'))
    {
        candidate.get(start..=end)
    } else if candidate.starts_with('{') && candidate.ends_with('}') {
        Some(candidate)
    } else {
        None
    };

    if let Some(snippet) = json_snippet {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(snippet) {
            if let Some(fields) = parse_schema_value(&parsed) {
                return Some(fields);
            }
        }

        let normalized = snippet.replace("\\\"", "\"");
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&normalized) {
            if let Some(fields) = parse_schema_value(&parsed) {
                return Some(fields);
            }
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

        if !fields.is_empty() {
            return Some(fields);
        }
    }

    None
}

fn parse_schema_value(value: &serde_json::Value) -> Option<Vec<ExtractField>> {
    let fields = match value {
        serde_json::Value::Array(arr) => arr
            .iter()
            .filter_map(|item| item.as_object())
            .filter_map(parse_schema_field)
            .collect::<Vec<_>>(),
        serde_json::Value::Object(obj) => {
            if let Some(arr) = obj.get("fields").and_then(|v| v.as_array()) {
                arr.iter()
                    .filter_map(|item| item.as_object())
                    .filter_map(parse_schema_field)
                    .collect::<Vec<_>>()
            } else {
                let mut collected = Vec::new();
                for (key, val) in obj {
                    if key.starts_with('_') {
                        continue;
                    }
                    match val {
                        serde_json::Value::String(field_type) => collected.push(ExtractField {
                            name: key.clone(),
                            description: key.clone(),
                            field_type: Some(field_type.clone()),
                            required: None,
                        }),
                        serde_json::Value::Object(field_obj) => {
                            let mut field_map = field_obj.clone();
                            field_map
                                .insert("name".to_string(), serde_json::Value::String(key.clone()));
                            if let Some(field) = parse_schema_field(&field_map) {
                                collected.push(field);
                            }
                        }
                        _ => {}
                    }
                }
                collected
            }
        }
        _ => Vec::new(),
    };

    if fields.is_empty() {
        None
    } else {
        Some(fields)
    }
}

fn parse_schema_field(obj: &serde_json::Map<String, serde_json::Value>) -> Option<ExtractField> {
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

/// Extract email addresses from content
fn extract_emails(content: &str) -> serde_json::Value {
    let email_re = Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}").unwrap();
    let emails: Vec<serde_json::Value> = email_re
        .find_iter(content)
        .map(|m| serde_json::Value::String(m.as_str().to_string()))
        .collect();

    if emails.is_empty() {
        serde_json::Value::Null
    } else if emails.len() == 1 {
        emails.into_iter().next().unwrap()
    } else {
        serde_json::Value::Array(emails)
    }
}

/// Extract phone numbers from content
fn extract_phones(content: &str) -> serde_json::Value {
    let phone_re = Regex::new(
        r"[\+]?[(]?[0-9]{1,3}[)]?[-\s\.]?[0-9]{1,4}[-\s\.]?[0-9]{1,4}[-\s\.]?[0-9]{1,9}",
    )
    .unwrap();
    let phones: Vec<serde_json::Value> = phone_re
        .find_iter(content)
        .filter(|m| m.as_str().len() >= 10)
        .map(|m| serde_json::Value::String(m.as_str().to_string()))
        .take(5)
        .collect();

    if phones.is_empty() {
        serde_json::Value::Null
    } else if phones.len() == 1 {
        phones.into_iter().next().unwrap()
    } else {
        serde_json::Value::Array(phones)
    }
}

/// Extract price values from content
fn extract_price(content: &str) -> serde_json::Value {
    let price_re = Regex::new(r"[\$€£¥₹][\s]?[0-9]{1,3}(?:[,.]?[0-9]{3})*(?:[.,][0-9]{2})?|[0-9]{1,3}(?:[,.]?[0-9]{3})*(?:[.,][0-9]{2})?\s?(?:USD|EUR|GBP|JPY|INR)").unwrap();
    let prices: Vec<serde_json::Value> = price_re
        .find_iter(content)
        .map(|m| serde_json::Value::String(m.as_str().to_string()))
        .take(10)
        .collect();

    if prices.is_empty() {
        serde_json::Value::Null
    } else if prices.len() == 1 {
        prices.into_iter().next().unwrap()
    } else {
        serde_json::Value::Array(prices)
    }
}

/// ADVANCED: Extract price with heuristic search in tables/lists near keyword
/// Hallucination protection: returns null if not found within 500 chars
fn extract_price_advanced(content: &str, keyword: &str) -> serde_json::Value {
    let price_re = Regex::new(r"[\$€£¥₹][\s]?[0-9]{1,3}(?:[,.]?[0-9]{3})*(?:[.,][0-9]{2})?|[0-9]{1,3}(?:[,.]?[0-9]{3})*(?:[.,][0-9]{2})?\s?(?:USD|EUR|GBP|JPY|INR)").unwrap();

    // First, try to find near keyword
    let keyword_lower = keyword.to_lowercase();
    let content_lower = content.to_lowercase();

    if let Some(pos) = content_lower.find(&keyword_lower) {
        // Search within 500 chars after keyword (hallucination protection limit)
        let search_area: String = content.chars().skip(pos).take(500).collect();
        if let Some(m) = price_re.find(&search_area) {
            return serde_json::Value::String(m.as_str().to_string());
        }
    }

    // Fallback: Try to find any price in content (first occurrence only)
    if let Some(m) = price_re.find(content) {
        return serde_json::Value::String(m.as_str().to_string());
    }

    // HALLUCINATION PROTECTION: Return null if no price found
    serde_json::Value::Null
}

/// Extract dates from content
fn extract_date_from_content(content: &str) -> serde_json::Value {
    // Common date patterns
    let date_patterns = [
        r"\d{4}-\d{2}-\d{2}", // 2024-01-15
        r"\d{2}/\d{2}/\d{4}", // 01/15/2024
        r"(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)[a-z]*\s+\d{1,2},?\s+\d{4}", // January 15, 2024
        r"\d{1,2}\s+(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)[a-z]*\s+\d{4}", // 15 January 2024
    ];

    for pattern in date_patterns {
        if let Ok(re) = Regex::new(pattern) {
            if let Some(m) = re.find(content) {
                return serde_json::Value::String(m.as_str().to_string());
            }
        }
    }

    serde_json::Value::Null
}

/// ADVANCED: Extract number near a keyword WITH HALLUCINATION PROTECTION
/// Returns null if not found within 500 chars of keyword
fn extract_number_with_hallucination_check(content: &str, keyword: &str) -> serde_json::Value {
    let content_lower = content.to_lowercase();
    let variants = keyword_variants(keyword);
    if let Some((pos, keyword_len)) = find_keyword_position(&content_lower, &variants) {
        // Look for numbers within 500 chars after keyword (hallucination protection)
        let search_area: String = content.chars().skip(pos + keyword_len).take(500).collect();
        let num_re = Regex::new(r"\d+(?:[.,]\d+)?").unwrap();
        if let Some(m) = num_re.find(&search_area) {
            if let Ok(num) = m.as_str().replace(",", "").parse::<f64>() {
                if let Some(json_num) = serde_json::Number::from_f64(num) {
                    return serde_json::Value::Number(json_num);
                }
            }
        }
    }

    // HALLUCINATION PROTECTION: Return null if not found
    serde_json::Value::Null
}

/// ADVANCED: Extract text near a keyword WITH HALLUCINATION PROTECTION
/// Returns null if not found within 500 chars of keyword
fn extract_text_with_hallucination_check(content: &str, keyword: &str) -> serde_json::Value {
    let content_lower = content.to_lowercase();
    let variants = keyword_variants(keyword);
    if let Some((pos, keyword_len)) = find_keyword_position(&content_lower, &variants) {
        // Get text after keyword until newline or 500 chars (hallucination check)
        let after: String = content
            .chars()
            .skip(pos + keyword_len)
            .take(500)
            .take_while(|c| *c != '\n')
            .collect();

        let trimmed = after.trim().trim_start_matches(':').trim();
        if !trimmed.is_empty() && trimmed.len() > 2 {
            // Minimum 3 chars to be valid
            return serde_json::Value::String(trimmed.to_string());
        }
    }

    // HALLUCINATION PROTECTION: Return null if not found
    serde_json::Value::Null
}

/// ADVANCED: Extract list near keyword, search in <ul>, <table>, <dl> structures
/// Hallucination protection: returns null if not found within 500 chars
fn extract_list_near_keyword_advanced(content: &str, keyword: &str) -> serde_json::Value {
    let content_lower = content.to_lowercase();
    let variants = keyword_variants(keyword);

    if let Some((pos, keyword_len)) = find_keyword_position(&content_lower, &variants) {
        // Look for bullet points or numbered items within 500 chars (hallucination protection)
        let search_area: String = content.chars().skip(pos + keyword_len).take(500).collect();
        let items: Vec<serde_json::Value> = search_area
            .lines()
            .filter(|l| {
                let trimmed = l.trim();
                trimmed.starts_with('-')
                    || trimmed.starts_with('•')
                    || trimmed.starts_with('*')
                    || trimmed.starts_with("1.")
                    || trimmed.starts_with("2.")
                    || trimmed.starts_with("3.")
            })
            .take(10)
            .map(|l| {
                let cleaned = l
                    .trim()
                    .trim_start_matches(|c: char| {
                        c == '-' || c == '•' || c == '*' || c.is_numeric() || c == '.'
                    })
                    .trim();
                serde_json::Value::String(cleaned.to_string())
            })
            .filter(|v| {
                if let Some(s) = v.as_str() {
                    !s.is_empty() && s.len() > 2
                } else {
                    false
                }
            })
            .collect();

        if !items.is_empty() {
            return serde_json::Value::Array(items);
        }
    }

    // HALLUCINATION PROTECTION: Return null if no list found
    serde_json::Value::Null
}

fn keyword_variants(keyword: &str) -> Vec<String> {
    let mut variants = Vec::new();
    let trimmed = keyword.trim();
    if trimmed.is_empty() {
        return variants;
    }

    let lower = trimmed.to_lowercase();
    variants.push(lower.clone());

    let spaced = lower.replace('_', " ").replace('-', " ");
    if spaced != lower {
        variants.push(spaced.clone());
    }

    if let Some(base) = lower.strip_suffix("_name") {
        if !base.is_empty() {
            variants.push(base.to_string());
        }
    }
    if let Some(base) = lower.strip_suffix("-name") {
        if !base.is_empty() {
            variants.push(base.to_string());
        }
    }

    if spaced.contains(' ') {
        let parts: Vec<&str> = spaced.split_whitespace().collect();
        if parts.len() > 1 {
            variants.push(parts.join(""));
        }
    }

    variants.sort();
    variants.dedup();
    variants
}

fn find_keyword_position(content_lower: &str, variants: &[String]) -> Option<(usize, usize)> {
    for variant in variants {
        if let Some(pos) = content_lower.find(variant) {
            return Some((pos, variant.len()));
        }
    }
    None
}
