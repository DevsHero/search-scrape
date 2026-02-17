use serde_json::{Map, Value};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct ToolCatalogEntry {
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
    pub icons: Vec<&'static str>,
}

pub const SHADOWCRAWL_ICON: &str = "data:image/svg+xml;base64,PHN2ZyB3aWR0aD0iNTEyIiBoZWlnaHQ9IjUxMiIgdmlld0JveD0iMCAwIDUxMiA1MTIiIGZpbGw9Im5vbmUiIHhtbG5zPSJodHRwOi8vd3d3LnczLm9yZy8yMDAwL3N2ZyI+CiAgICA8ZGVmcz4KICAgICAgICA8bGluZWFyR3JhZGllbnQgaWQ9ImNyYXdsZXJfZ3JhZCIgeDE9IjAlIiB5MT0iMjAlIiB4Mj0iMTAwJSIgeTI9IjEwMCUiPgogICAgICAgICAgICA8c3RvcCBvZmZzZXQ9IjAlIiBzdHlsZT0ic3RvcC1jb2xvcjojMWUxZTVhO3N0b3Atb3BhY2l0eToxIiAvPiA8c3RvcCBvZmZzZXQ9IjUwJSIgc3R5bGU9InN0b3AtY29sb3I6IzNhM2E5ZTtzdG9wLW9wYWNpdHk6MSIgLz4gPHN0b3Agb2Zmc2V0PSIxMDAlIiBzdHlsZT0ic3RvcC1jb2xvcjojMDBmMmZmO3N0b3Atb3BhY2l0eToxIiAvPiA8L2xpbmVhckdyYWRpZW50PgogICAgICAgIAogICAgICAgIDxyYWRpYWxHcmFkaWVudCBpZD0iZXllX2dsb3ciIGN4PSI1MCUiIGN5PSI1MCUiIHI9IjUwJSIgZng9IjUwJSIgZnk9IjUwJSI+CiAgICAgICAgICAgIDxzdG9wIG9mZnNldD0iMCUiIHN0eWxlPSJzdG9wLWNvbG9yOiNmZmZmZmY7c3RvcC1vcGFjaXR5OjEiIC8+CiAgICAgICAgICAgIDxzdG9wIG9mZnNldD0iMTAwJSIgc3R5bGU9InN0b3AtY29sb3I6IzAwZjJmZjtzdG9wLW9wYWNpdHk6MSIgLz4KICAgICAgICA8L3JhZGlhbEdyYWRpZW50PgoKICAgICAgICA8ZmlsdGVyIGlkPSJzaGFkb3dCbHVyIiB4PSItNTAlIiB5PSItMjAlIiB3aWR0aD0iMjAwJSIgaGVpZ2h0PSIxNTAlIj4KICAgICAgICAgICAgPGZlR2F1c3NpYW5CbHVyIGluPSJTb3VyY2VHcmFwaGljIiBzdGREZXZpYXRpb249IjgiIC8+CiAgICAgICAgPC9maWx0ZXI+CiAgICA8L2RlZnM+CgogICAgPGcgdHJhbnNmb3JtPSJ0cmFuc2xhdGUoMjU2LCAyNTYpIj4KICAgICAgICA8cGF0aCBkPSJNLTEyMCA0MCBDIC0xNDAgODAsIC04MCAxNjAsIDAgMTgwIEMgODAgMTYwLCAxNDAgODAsIDEyMCA0MCBMIDAgODAgWiIgCiAgICAgICAgICAgICAgZmlsbD0idXJsKCNjcmF3bGVyX2dyYWQpIiAKICAgICAgICAgICAgICBvcGFjaXR5PSIwLjQiIAogICAgICAgICAgICAgIGZpbHRlcj0idXJsKCNzaGFkb3dCbHVyKSIKICAgICAgICAgICAgICB0cmFuc2Zvcm09InRyYW5zbGF0ZSgwLCAtMjApIi8+CgogICAgICAgIDxwYXRoIGQ9Ik0wIC0xODAgTCAxNDAgLTYwIEwgMTAwIDYwIEwgMCAxMjAgTCAtMTAwIDYwIEwgLTE0MCAtNjAgWiIgCiAgICAgICAgICAgICAgZmlsbD0idXJsKCNjcmF3bGVyX2dyYWQpIgogICAgICAgICAgICAgIHN0cm9rZT0iIzAwZjJmZiIKICAgICAgICAgICAgICBzdHJva2Utd2lkdGg9IjQiCiAgICAgICAgICAgICAgc3Ryb2tlLWxpbmVqb2luPSJyb3VuZCIvPgogICAgICAgICAgICAgIAogICAgICAgIDxwYXRoIGQ9Ik0wIC00MCBMIDQwIDAgTCAwIDQwIEwgLTQwIDAgWiIgCiAgICAgICAgICAgICAgZmlsbD0idXJsKCNleWVfZ2xvdykiCiAgICAgICAgICAgICAgZmlsdGVyPSJkcm9wLXNoYWRvdygwIDAgMTBweCAjMDBmMmZmKSIvPgogICAgICAgICAgICAgIAogICAgICAgIDxwYXRoIGQ9Ik0tMTAwIDYwIEwgLTEzMCAxNDAgTCAtOTAgMTIwIE0xMDAgNjAgTCAxMzAgMTQwIEwgOTAgMTIwIiAKICAgICAgICAgICAgICBzdHJva2U9InVybCgjY3Jhd2xlcl9ncmFkKSIgCiAgICAgICAgICAgICAgc3Ryb2tlLXdpZHRoPSIxMiIgCiAgICAgICAgICAgICAgc3Ryb2tlLWxpbmVjYXA9InJvdW5kIgogICAgICAgICAgICAgIGZpbGw9Im5vbmUiLz4KICAgIDwvZz4KICAgIAogICAgPC9zdmc+";

pub fn tool_catalog() -> Vec<ToolCatalogEntry> {
    let mut tools = vec![
        ToolCatalogEntry {
            name: "search_web",
            title: "Web Search",
            description: "Search the internet for real-time information and links. Use this first to find URLs before scraping.",
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
            icons: vec![SHADOWCRAWL_ICON],
        },
        ToolCatalogEntry {
            name: "search_structured",
            title: "Search and Extract",
            description: "Search the web and immediately return the top results as a clean, structured JSON format.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "top_n": {"type": "integer", "minimum": 1, "default": 3},
                    "use_proxy": {"type": "boolean", "default": false},
                    "quality_mode": {"type": "string", "enum": ["balanced", "aggressive"], "default": "balanced"}
                },
                "required": ["query"]
            }),
            icons: vec![SHADOWCRAWL_ICON],
        },
        ToolCatalogEntry {
            name: "scrape_url",
            title: "Scrape Normal Website",
            description: "Extract readable text and links from a single, standard website URL. Do NOT use this for protected sites (Cloudflare/LinkedIn).",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string"},
                    "max_chars": {"type": "integer"},
                    "max_links": {"type": "integer", "minimum": 1},
                    "output_format": {"type": "string", "enum": ["text", "json"], "default": "text"},
                    "include_raw_html": {"type": "boolean", "default": false},
                    "use_proxy": {"type": "boolean", "default": false},
                    "quality_mode": {"type": "string", "enum": ["balanced", "aggressive"], "default": "balanced"}
                },
                "required": ["url"]
            }),
            icons: vec![SHADOWCRAWL_ICON],
        },
        ToolCatalogEntry {
            name: "scrape_batch",
            title: "Batch Scrape",
            description: "Scrape multiple standard URLs at the same time to speed up research.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "urls": {"type": "array", "items": {"type": "string"}},
                    "max_concurrent": {"type": "integer", "minimum": 1},
                    "max_chars": {"type": "integer"},
                    "output_format": {"type": "string", "enum": ["text", "json"], "default": "json"},
                    "use_proxy": {"type": "boolean", "default": false},
                    "quality_mode": {"type": "string", "enum": ["balanced", "aggressive"], "default": "balanced"}
                },
                "required": ["urls"]
            }),
            icons: vec![SHADOWCRAWL_ICON],
        },
        ToolCatalogEntry {
            name: "crawl_website",
            title: "Crawl Site Map",
            description: "Find all sub-pages and links within a specific website to understand its structure.",
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
                    "use_proxy": {"type": "boolean", "default": false},
                    "quality_mode": {"type": "string", "enum": ["balanced", "aggressive"], "default": "balanced"}
                },
                "required": ["url"]
            }),
            icons: vec![SHADOWCRAWL_ICON],
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
                    "use_proxy": {"type": "boolean", "default": false},
                    "quality_mode": {"type": "string", "enum": ["balanced", "aggressive"], "default": "balanced"}
                },
                "required": ["url"]
            }),
            icons: vec![SHADOWCRAWL_ICON],
        },
       ToolCatalogEntry {
        name: "research_history",
        title: "Search Past Research", // เปลี่ยนจาก Research History เป็นกริยา
        description: "Access your memory of previous searches and scrapes. Use this to retrieve information you already found earlier in this session or past sessions. Search by meaning to avoid re-searching or re-scraping the same URLs.",
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
            "query": {
                "type": "string", 
                "description": "Semantic search query to find relevant past information (e.g., 'What did I find about Rust safety?')"
            },
            "limit": {
                "type": "integer", 
                "minimum": 1, 
                "maximum": 50,
                "description": "Number of historical entries to return."
            },
            "threshold": {
                "type": "number", 
                "minimum": 0.0, 
                "maximum": 1.0,
                "description": "Similarity score; higher means more exact matches."
            },
            "entry_type": {
                "type": "string", 
                "enum": ["search", "scrape"],
                "description": "Optional: Filter by 'search' queries or 'scrape' content."
            }
            },
            "required": ["query"]
            }),
            icons: vec![SHADOWCRAWL_ICON],
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
                    "strict_proxy_health": {"type": "boolean", "default": false},
                    "proxy_url": {"type": "string"},
                    "target_url": {"type": "string"}
                },
                "required": ["action"]
            }),
            icons: vec![SHADOWCRAWL_ICON],
        },
        ToolCatalogEntry {
            name: "non_robot_search",
            title: "Human-In-The-Loop Stealth Scraper", 
            description: "ULTIMATE BYPASS: Use this ONLY for sites with heavy anti-bot (Cloudflare, LinkedIn, Upwork, CAPTCHA). \
                  It opens a REAL browser on the host machine. If a challenge/login appears, the Agent MUST ask the human \
                  user to solve it on their screen. Use this when 'scrape_url' is blocked or returns bot-detection errors.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                "url": {"type": "string", "description": "The URL that is blocking standard bot access."},
                "output_format": {"type": "string", "enum": ["text", "json"], "default": "json"},
                "max_chars": {"type": "integer", "minimum": 1, "default": 10000},
                "use_proxy": {"type": "boolean", "default": false},
                "quality_mode": {"type": "string", "enum": ["balanced", "aggressive", "high"], "default": "balanced"},
                "captcha_grace_seconds": {"type": "integer", "minimum": 1, "default": 5, "description": "Seconds to wait for a human to solve a CAPTCHA before checking content again."},
                "human_timeout_seconds": {"type": "integer", "minimum": 1, "default": 60, "description": "Max time (seconds) to keep the browser open for human interaction/login."},
                "user_profile_path": {"type": "string", "description": "Path to a real browser profile (Chrome/Brave) to bypass login walls using existing cookies."},
                "auto_scroll": {"type": "boolean", "default": false, "description": "Scroll down to trigger lazy-loaded items (critical for infinite-scroll sites)."},
                "wait_for_selector": {"type": "string", "description": "Wait for this CSS element to ensure the page has fully bypassed the bot wall."}
        },
        "required": ["url"]
        }),
        icons: vec![SHADOWCRAWL_ICON],
    }
    ];
    tools
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
        let port = parsed.port().map(|p| format!(":{}", p)).unwrap_or_default();

        format!("{}://{}{}{}", parsed.scheme(), user_segment, host, port)
    } else {
        url.to_string()
    }
}
