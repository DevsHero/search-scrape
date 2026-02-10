use anyhow::Result;
use regex::Regex;
use std::sync::Arc;
use std::time::Instant;
use tracing::info;

use crate::scrape::scrape_url;
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
) -> Result<ExtractResponse> {
    let start_time = Instant::now();
    let max_chars = max_chars.unwrap_or(10000);

    info!("Extracting structured data from: {}", url);

    // First, scrape the page
    let scrape_result = scrape_url(state, url).await?;

    let mut extracted_data = serde_json::Map::new();
    let mut warnings = Vec::new();
    let mut confidence: f64 = 0.8;

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
    extracted_data.insert("_title".to_string(), serde_json::Value::String(scrape_result.title.clone()));
    extracted_data.insert("_url".to_string(), serde_json::Value::String(url.to_string()));
    extracted_data.insert("_word_count".to_string(), serde_json::Value::Number(scrape_result.word_count.into()));

    if let Some(author) = &scrape_result.author {
        extracted_data.insert("_author".to_string(), serde_json::Value::String(author.clone()));
    }
    if let Some(published) = &scrape_result.published_at {
        extracted_data.insert("_published_at".to_string(), serde_json::Value::String(published.clone()));
    }

    let field_count = extracted_data.len();
    let raw_preview: String = scrape_result.clean_content.chars().take(max_chars).collect();

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
        "title" | "name" | "headline" => {
            serde_json::Value::String(scrape.title.clone())
        }
        "description" | "summary" | "excerpt" => {
            if !scrape.meta_description.is_empty() {
                serde_json::Value::String(scrape.meta_description.clone())
            } else {
                // First paragraph
                let first_para: String = content.lines()
                    .find(|l| l.len() > 50)
                    .unwrap_or("")
                    .chars()
                    .take(500)
                    .collect();
                serde_json::Value::String(first_para)
            }
        }
        "author" | "writer" | "by" => {
            scrape.author.clone()
                .map(serde_json::Value::String)
                .unwrap_or(serde_json::Value::Null)
        }
        "date" | "published" | "published_at" | "publish_date" => {
            scrape.published_at.clone()
                .map(serde_json::Value::String)
                .unwrap_or_else(|| extract_date_from_content(content))
        }
        "price" | "cost" | "amount" => {
            extract_price(content)
        }
        "email" | "emails" => {
            extract_emails(content)
        }
        "phone" | "telephone" | "phones" => {
            extract_phones(content)
        }
        "links" | "urls" => {
            let urls: Vec<serde_json::Value> = scrape.links.iter()
                .take(20)
                .map(|l| serde_json::Value::String(l.url.clone()))
                .collect();
            serde_json::Value::Array(urls)
        }
        "headings" | "headers" | "sections" => {
            let headings: Vec<serde_json::Value> = scrape.headings.iter()
                .map(|h| serde_json::Value::String(format!("{}: {}", h.level, h.text)))
                .collect();
            serde_json::Value::Array(headings)
        }
        "code" | "code_blocks" | "code_snippets" => {
            let blocks: Vec<serde_json::Value> = scrape.code_blocks.iter()
                .map(|b| {
                    let mut obj = serde_json::Map::new();
                    obj.insert("language".to_string(),
                        b.language.clone().map(serde_json::Value::String).unwrap_or(serde_json::Value::Null));
                    obj.insert("code".to_string(), serde_json::Value::String(b.code.clone()));
                    serde_json::Value::Object(obj)
                })
                .collect();
            serde_json::Value::Array(blocks)
        }
        "images" => {
            let imgs: Vec<serde_json::Value> = scrape.images.iter()
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
            // Try to find pattern in content based on description
            if desc_lower.contains("number") || desc_lower.contains("count") || desc_lower.contains("quantity") {
                extract_number_near_keyword(content, &field.name)
            } else if desc_lower.contains("list") || desc_lower.contains("array") {
                extract_list_near_keyword(content, &field.name)
            } else {
                extract_text_near_keyword(content, &field.name)
            }
        }
    }
}

/// Auto-extract common data patterns from content
fn auto_extract(scrape: &ScrapeResponse, prompt: Option<&str>) -> serde_json::Map<String, serde_json::Value> {
    let mut data = serde_json::Map::new();
    let content = &scrape.clean_content;

    // Always extract these
    data.insert("title".to_string(), serde_json::Value::String(scrape.title.clone()));

    if !scrape.meta_description.is_empty() {
        data.insert("description".to_string(), serde_json::Value::String(scrape.meta_description.clone()));
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
                data.insert("product_name".to_string(), serde_json::Value::String(h1.text.clone()));
            }
        }

        if prompt_lower.contains("article") || prompt_lower.contains("blog") {
            // Article-focused extraction
            if let Some(author) = &scrape.author {
                data.insert("author".to_string(), serde_json::Value::String(author.clone()));
            }
            if let Some(date) = &scrape.published_at {
                data.insert("published_date".to_string(), serde_json::Value::String(date.clone()));
            }

            // Reading time
            if let Some(time) = scrape.reading_time_minutes {
                data.insert("reading_time_minutes".to_string(), serde_json::Value::Number(time.into()));
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
                let blocks: Vec<serde_json::Value> = scrape.code_blocks.iter()
                    .map(|b| serde_json::Value::String(b.code.clone()))
                    .collect();
                data.insert("code_blocks".to_string(), serde_json::Value::Array(blocks));
            }
        }
    }

    // Add headings as table of contents
    if !scrape.headings.is_empty() {
        let toc: Vec<serde_json::Value> = scrape.headings.iter()
            .filter(|h| h.level == "h1" || h.level == "h2" || h.level == "h3")
            .take(15)
            .map(|h| serde_json::Value::String(h.text.clone()))
            .collect();
        if !toc.is_empty() {
            data.insert("table_of_contents".to_string(), serde_json::Value::Array(toc));
        }
    }

    data
}

/// Extract email addresses from content
fn extract_emails(content: &str) -> serde_json::Value {
    let email_re = Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}").unwrap();
    let emails: Vec<serde_json::Value> = email_re.find_iter(content)
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
    let phone_re = Regex::new(r"[\+]?[(]?[0-9]{1,3}[)]?[-\s\.]?[0-9]{1,4}[-\s\.]?[0-9]{1,4}[-\s\.]?[0-9]{1,9}").unwrap();
    let phones: Vec<serde_json::Value> = phone_re.find_iter(content)
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
    let prices: Vec<serde_json::Value> = price_re.find_iter(content)
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

/// Extract dates from content
fn extract_date_from_content(content: &str) -> serde_json::Value {
    // Common date patterns
    let date_patterns = [
        r"\d{4}-\d{2}-\d{2}",  // 2024-01-15
        r"\d{2}/\d{2}/\d{4}",  // 01/15/2024
        r"(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)[a-z]*\s+\d{1,2},?\s+\d{4}",  // January 15, 2024
        r"\d{1,2}\s+(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)[a-z]*\s+\d{4}",  // 15 January 2024
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

/// Extract number near a keyword
fn extract_number_near_keyword(content: &str, keyword: &str) -> serde_json::Value {
    let keyword_lower = keyword.to_lowercase();
    let content_lower = content.to_lowercase();

    if let Some(pos) = content_lower.find(&keyword_lower) {
        // Look for numbers within 100 chars after keyword
        let search_area: String = content.chars().skip(pos).take(100).collect();
        let num_re = Regex::new(r"\d+(?:[.,]\d+)?").unwrap();
        if let Some(m) = num_re.find(&search_area) {
            if let Ok(num) = m.as_str().replace(",", "").parse::<f64>() {
                if let Some(json_num) = serde_json::Number::from_f64(num) {
                    return serde_json::Value::Number(json_num);
                }
            }
        }
    }
    serde_json::Value::Null
}

/// Extract text near a keyword
fn extract_text_near_keyword(content: &str, keyword: &str) -> serde_json::Value {
    let keyword_lower = keyword.to_lowercase();
    let content_lower = content.to_lowercase();

    if let Some(pos) = content_lower.find(&keyword_lower) {
        // Get text after keyword until newline or 200 chars
        let after: String = content.chars()
            .skip(pos + keyword.len())
            .take(200)
            .take_while(|c| *c != '\n')
            .collect();

        let trimmed = after.trim().trim_start_matches(':').trim();
        if !trimmed.is_empty() {
            return serde_json::Value::String(trimmed.to_string());
        }
    }
    serde_json::Value::Null
}

/// Extract list near a keyword
fn extract_list_near_keyword(content: &str, keyword: &str) -> serde_json::Value {
    let keyword_lower = keyword.to_lowercase();
    let content_lower = content.to_lowercase();

    if let Some(pos) = content_lower.find(&keyword_lower) {
        // Look for bullet points or numbered items
        let search_area: String = content.chars().skip(pos).take(500).collect();
        let items: Vec<serde_json::Value> = search_area.lines()
            .filter(|l| l.trim().starts_with('-') || l.trim().starts_with('•') || l.trim().starts_with('*'))
            .take(10)
            .map(|l| serde_json::Value::String(l.trim().trim_start_matches(|c| c == '-' || c == '•' || c == '*').trim().to_string()))
            .collect();

        if !items.is_empty() {
            return serde_json::Value::Array(items);
        }
    }
    serde_json::Value::Null
}
