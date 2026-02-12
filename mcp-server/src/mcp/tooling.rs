use serde_json::{Map, Value};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct ToolCatalogEntry {
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
}

pub fn tool_catalog() -> Vec<ToolCatalogEntry> {
    vec![
        ToolCatalogEntry {
            name: "search_web",
            title: "Web Search",
            description: "Find sources on the public web.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "engines": {"type": "string"},
                    "categories": {"type": "string"},
                    "language": {"type": "string"},
                    "safesearch": {"type": "integer", "minimum": 0, "maximum": 2},
                    "time_range": {"type": "string", "enum": ["day", "week", "month", "year"]},
                    "pageno": {"type": "integer", "minimum": 1},
                    "max_results": {"type": "integer", "minimum": 1, "maximum": 100, "default": 10}
                },
                "required": ["query"]
            }),
        },
        ToolCatalogEntry {
            name: "search_structured",
            title: "Search Structured",
            description: "Search and scrape top results.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "top_n": {"type": "integer", "minimum": 1, "default": 3},
                    "use_proxy": {"type": "boolean", "default": false}
                },
                "required": ["query"]
            }),
        },
        ToolCatalogEntry {
            name: "scrape_url",
            title: "Scrape URL",
            description: "Fetch a URL and extract content.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string"},
                    "max_chars": {"type": "integer"},
                    "max_links": {"type": "integer", "minimum": 1},
                    "output_format": {"type": "string", "enum": ["text", "json"], "default": "text"},
                    "use_proxy": {"type": "boolean", "default": false}
                },
                "required": ["url"]
            }),
        },
        ToolCatalogEntry {
            name: "scrape_batch",
            title: "Scrape Batch",
            description: "Scrape many URLs in parallel.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "urls": {"type": "array", "items": {"type": "string"}},
                    "max_concurrent": {"type": "integer", "minimum": 1},
                    "max_chars": {"type": "integer"},
                    "output_format": {"type": "string", "enum": ["text", "json"], "default": "json"},
                    "use_proxy": {"type": "boolean", "default": false}
                },
                "required": ["urls"]
            }),
        },
        ToolCatalogEntry {
            name: "crawl_website",
            title: "Crawl Website",
            description: "Crawl a site recursively.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string"},
                    "max_depth": {"type": "integer", "minimum": 0},
                    "max_pages": {"type": "integer", "minimum": 1},
                    "max_concurrent": {"type": "integer", "minimum": 1},
                    "include_patterns": {"type": "array", "items": {"type": "string"}},
                    "exclude_patterns": {"type": "array", "items": {"type": "string"}},
                    "same_domain_only": {"type": "boolean"},
                    "max_chars_per_page": {"type": "integer", "minimum": 1},
                    "use_proxy": {"type": "boolean", "default": false}
                },
                "required": ["url"]
            }),
        },
        ToolCatalogEntry {
            name: "extract_structured",
            title: "Extract Structured",
            description: "Extract structured fields from a page.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string"},
                    "schema": {"type": "array", "items": {"type": "object"}},
                    "prompt": {"type": "string"},
                    "max_chars": {"type": "integer"},
                    "use_proxy": {"type": "boolean", "default": false}
                },
                "required": ["url"]
            }),
        },
        ToolCatalogEntry {
            name: "research_history",
            title: "Research History",
            description: "Search prior searches/scrapes by meaning.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 50},
                    "threshold": {"type": "number", "minimum": 0.0, "maximum": 1.0},
                    "entry_type": {"type": "string", "enum": ["search", "scrape"]}
                },
                "required": ["query"]
            }),
        },
        ToolCatalogEntry {
            name: "proxy_manager",
            title: "Proxy Manager",
            description: "Unified proxy manager: grab, list, status, switch, test.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "action": {"type": "string", "enum": ["grab", "list", "status", "switch", "test"]},
                    "limit": {"type": "integer", "minimum": 1},
                    "proxy_type": {"type": "string", "enum": ["http", "https", "socks5", "socks4", "sock5", "sock4"]},
                    "random": {"type": "boolean", "default": false},
                    "store_ip_txt": {"type": "boolean", "default": false},
                    "clear_ip_txt": {"type": "boolean", "default": false},
                    "append": {"type": "boolean", "default": false},
                    "show_proxy_type": {"type": "boolean", "default": true},
                    "force_new": {"type": "boolean", "default": false},
                    "proxy_url": {"type": "string"},
                    "target_url": {"type": "string"}
                },
                "required": ["action"]
            }),
        },
    ]
}

pub fn schema_to_object_map(schema: &Value) -> Arc<Map<String, Value>> {
    match schema {
        Value::Object(map) => Arc::new(map.clone()),
        _ => Arc::new(Map::new()),
    }
}

pub fn format_proxy_display(url: &str) -> String {
    if let Ok(parsed) = url::Url::parse(url) {
        let user_segment = if parsed.username().is_empty() {
            String::new()
        } else {
            format!("{}@", parsed.username())
        };

        let host = parsed.host_str().unwrap_or("unknown");
        let port = parsed
            .port()
            .map(|p| format!(":{}", p))
            .unwrap_or_default();

        format!("{}://{}{}{}", parsed.scheme(), user_segment, host, port)
    } else {
        url.to_string()
    }
}
