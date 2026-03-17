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

pub const CORTEX_SCOUT_ICON: &str = "data:image/svg+xml;base64,PHN2ZyB3aWR0aD0iNTEyIiBoZWlnaHQ9IjUxMiIgdmlld0JveD0iMCAwIDUxMiA1MTIiIGZpbGw9Im5vbmUiIHhtbG5zPSJodHRwOi8vd3d3LnczLm9yZy8yMDAwL3N2ZyI+CiAgICA8ZGVmcz4KICAgICAgICA8bGluZWFyR3JhZGllbnQgaWQ9ImNyYXdsZXJfZ3JhZCIgeDE9IjAlIiB5MT0iMjAlIiB4Mj0iMTAwJSIgeTI9IjEwMCUiPgogICAgICAgICAgICA8c3RvcCBvZmZzZXQ9IjAlIiBzdHlsZT0ic3RvcC1jb2xvcjojMWUxZTVhO3N0b3Atb3BhY2l0eToxIiAvPiA8c3RvcCBvZmZzZXQ9IjUwJSIgc3R5bGU9InN0b3AtY29sb3I6IzNhM2E5ZTtzdG9wLW9wYWNpdHk6MSIgLz4gPHN0b3Agb2Zmc2V0PSIxMDAlIiBzdHlsZT0ic3RvcC1jb2xvcjojMDBmMmZmO3N0b3Atb3BhY2l0eToxIiAvPiA8L2xpbmVhckdyYWRpZW50PgogICAgICAgIAogICAgICAgIDxyYWRpYWxHcmFkaWVudCBpZD0iZXllX2dsb3ciIGN4PSI1MCUiIGN5PSI1MCUiIHI9IjUwJSIgZng9IjUwJSIgZnk9IjUwJSI+CiAgICAgICAgICAgIDxzdG9wIG9mZnNldD0iMCUiIHN0eWxlPSJzdG9wLWNvbG9yOiNmZmZmZmY7c3RvcC1vcGFjaXR5OjEiIC8+CiAgICAgICAgICAgIDxzdG9wIG9mZnNldD0iMTAwJSIgc3R5bGU9InN0b3AtY29sb3I6IzAwZjJmZjtzdG9wLW9wYWNpdHk6MSIgLz4KICAgICAgICA8L3JhZGlhbEdyYWRpZW50PgoKICAgICAgICA8ZmlsdGVyIGlkPSJzaGFkb3dCbHVyIiB4PSItNTAlIiB5PSItMjAlIiB3aWR0aD0iMjAwJSIgaGVpZ2h0PSIxNTAlIj4KICAgICAgICAgICAgPGZlR2F1c3NpYW5CbHVyIGluPSJTb3VyY2VHcmFwaGljIiBzdGREZXZpYXRpb249IjgiIC8+CiAgICAgICAgPC9maWx0ZXI+CiAgICA8L2RlZnM+CgogICAgPGcgdHJhbnNmb3JtPSJ0cmFuc2xhdGUoMjU2LCAyNTYpIj4KICAgICAgICA8cGF0aCBkPSJNLTEyMCA0MCBDIC0xNDAgODAsIC04MCAxNjAsIDAgMTgwIEMgODAgMTYwLCAxNDAgODAsIDEyMCA0MCBMIDAgODAgWiIgCiAgICAgICAgICAgICAgZmlsbD0idXJsKCNjcmF3bGVyX2dyYWQpIiAKICAgICAgICAgICAgICBvcGFjaXR5PSIwLjQiIAogICAgICAgICAgICAgIGZpbHRlcj0idXJsKCNzaGFkb3dCbHVyKSIKICAgICAgICAgICAgICB0cmFuc2Zvcm09InRyYW5zbGF0ZSgwLCAtMjApIi8+CgogICAgICAgIDxwYXRoIGQ9Ik0wIC0xODAgTCAxNDAgLTYwIEwgMTAwIDYwIEwgMCAxMjAgTCAtMTAwIDYwIEwgLTE0MCAtNjAgWiIgCiAgICAgICAgICAgICAgZmlsbD0idXJsKCNjcmF3bGVyX2dyYWQpIgogICAgICAgICAgICAgIHN0cm9rZT0iIzAwZjJmZiIKICAgICAgICAgICAgICBzdHJva2Utd2lkdGg9IjQiCiAgICAgICAgICAgICAgc3Ryb2tlLWxpbmVqb2luPSJyb3VuZCIvPgogICAgICAgICAgICAgIAogICAgICAgIDxwYXRoIGQ9Ik0wIC00MCBMIDQwIDAgTCAwIDQwIEwgLTQwIDAgWiIgCiAgICAgICAgICAgICAgZmlsbD0idXJsKCNleWVfZ2xvdykiCiAgICAgICAgICAgICAgZmlsdGVyPSJkcm9wLXNoYWRvdygwIDAgMTBweCAjMDBmMmZmKSIvPgogICAgICAgICAgICAgIAogICAgICAgIDxwYXRoIGQ9Ik0tMTAwIDYwIEwgLTEzMCAxNDAgTCAtOTAgMTIwIE0xMDAgNjAgTCAxMzAgMTQwIEwgOTAgMTIwIiAKICAgICAgICAgICAgICBzdHJva2U9InVybCgjY3Jhd2xlcl9ncmFkKSIgCiAgICAgICAgICAgICAgc3Ryb2tlLXdpZHRoPSIxMiIgCiAgICAgICAgICAgICAgc3Ryb2tlLWxpbmVjYXA9InJvdW5kIgogICAgICAgICAgICAgIGZpbGw9Im5vbmUiLz4KICAgIDwvZz4KICAgIAogICAgPC9zdmc+";

/// Returns `true` when the `deep_research` tool should be registered at runtime.
/// Both the `deep-research` Cargo feature AND the env-var gate must be satisfied.
///
/// Build-time opt-out:  `cargo build --no-default-features`  → feature is absent → always false.
/// Runtime opt-out:     `DEEP_RESEARCH_ENABLED=0`             → feature present  → false.
/// Runtime opt-in:      feature present + no env var (or `=1`) → true  (default).
pub fn deep_research_enabled() -> bool {
    if !cfg!(feature = "deep-research") {
        return false;
    }
    // Any value other than "0" / "false" / "no" / "off" is treated as enabled.
    match std::env::var("DEEP_RESEARCH_ENABLED") {
        Ok(v) => {
            let v = v.trim().to_lowercase();
            !matches!(v.as_str(), "0" | "false" | "no" | "off")
        }
        Err(_) => true, // default: enabled
    }
}

pub fn tool_catalog() -> Vec<ToolCatalogEntry> {
    let mut tools = vec![
        ToolCatalogEntry {
            name: "search_web",
            title: "Web Search (Multi-Engine)",
            description: "Primary URL discovery. Multi-engine search (Google/Bing/DDG/Brave), deduped + ranked. Use this before web_fetch. \
⚠️ AGENT RULE: ALWAYS call memory_search BEFORE this tool — the answer may already be cached from a previous session. \
For initial research where you will also fetch content, strongly prefer web_search_json over calling this tool and then web_fetch separately — it short-circuits to a single round-trip.",
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
                    "max_results": {"type": "integer", "minimum": 1, "maximum": 100, "default": 10},
                    "snippet_chars": {
                        "type": "integer",
                        "minimum": 20,
                        "maximum": 1000,
                        "description": "Max chars of each result's content snippet. Default: 120 (NeuroSiphon) / 200 (standard). Increase for deep research; decrease for token-constrained tasks."
                    }
                },
                "required": ["query"]
            }),
            icons: vec![CORTEX_SCOUT_ICON],
        },
        ToolCatalogEntry {
            name: "search_structured",
            title: "Web Search (Top Results JSON)",
            description: "Search + return top results as clean JSON (deduped, ranked). \
✅ PREFERRED for initial research: combines search + pre-scraped content summaries in a single call — use this INSTEAD of web_search + separate web_fetch. \
Defaults to direct/no-proxy mode; only retry with use_proxy=true after confirmed rate limits or IP blocks. \
Note: still call memory_search first to avoid redundant fetches.",
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
            icons: vec![CORTEX_SCOUT_ICON],
        },
        ToolCatalogEntry {
            name: "scrape_url",
            title: "Web Fetch (Token-Efficient)",
            description: "PRIMARY page fetch for agents. Clean token-efficient text + key links; auto-escalates to native CDP rendering when needed. Prefer over IDE fetch. Use hitl_web_fetch only for heavy challenges (CAPTCHA/login). \
✅ BEST PRACTICE — documentation / article pages: set output_format: clean_json + strict_relevance: true + a query string for maximum noise reduction and minimum token usage. \
Default path is direct/no-proxy. ⚠️ PROXY RULE: if this tool returns a 403, 429, or any rate-limit / IP-block error, IMMEDIATELY call proxy_control with action: grab to rotate the IP, then retry this call with use_proxy: true. \
🔒 AUTH-RISK FIELD: every response includes `auth_risk_score` (0.0–1.0). If score >= 0.4, STOP reading content and call `visual_scout` for visual confirmation before escalating to `human_auth_session`.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string"},
                    "query": {
                        "type": "string",
                        "description": "Optional query for Semantic Shaving. When strict_relevance=true, keeps only query-relevant paragraphs (major token savings on long pages)."
                    },
                    "extract_relevant_sections": {
                        "type": "boolean",
                        "default": false,
                        "description": "When true, return ONLY the most relevant sections for the query (short output). Helps avoid huge outputs that overflow tool UIs. Requires 'query'."
                    },
                    "section_limit": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 20,
                        "default": 5,
                        "description": "Max number of sections to keep when extract_relevant_sections=true."
                    },
                    "section_threshold": {
                        "type": "number",
                        "minimum": 0.0,
                        "maximum": 1.0,
                        "default": 0.45,
                        "description": "Similarity threshold for section extraction. Higher = fewer sections (shorter output)."
                    },
                    "strict_relevance": {
                        "type": "boolean",
                        "default": false,
                        "description": "Enable Semantic Shaving (requires query). Filters content to only relevant paragraphs using Model2Vec cosine similarity."
                    },
                    "relevance_threshold": {
                        "type": "number",
                        "minimum": 0.0,
                        "maximum": 1.0,
                        "default": 0.35,
                        "description": "Cosine similarity threshold for Semantic Shaving (default 0.35). Lower = keep more; higher = keep less."
                    },
                    "max_chars": {
                        "type": "integer",
                        "description": "Hard cap on the TOTAL serialized output payload (not just the text field). Prevents workspace storage spills. In json mode, caps the entire ScrapeResponse JSON including links[], images[], code_blocks[]. Default: 10000."
                    },
                    "max_links": {"type": "integer", "minimum": 1},
                    "max_headings": {
                        "type": "integer",
                        "minimum": 0,
                        "default": 10,
                        "description": "Max headings to include in text mode output. Default: 10."
                    },
                    "max_images": {
                        "type": "integer",
                        "minimum": 0,
                        "default": 3,
                        "description": "Max image markdown hints to include in text mode output. Default: 3."
                    },
                    "short_content_threshold": {
                        "type": "integer",
                        "minimum": 0,
                        "default": 50,
                        "description": "Word-count threshold below which short_content warning fires. Default: 50. Set to 0 to disable."
                    },
                    "extraction_score_threshold": {
                        "type": "number",
                        "minimum": 0.0,
                        "maximum": 1.0,
                        "default": 0.4,
                        "description": "Extraction quality threshold below which low_extraction_score warning fires. Default: 0.4. Set to 0.0 to disable."
                    },
                    "output_format": {
                        "type": "string",
                        "enum": ["text", "json", "clean_json"],
                        "default": "text",
                        "description": "Output format. 'text' = readable prose (default). 'json' = full ScrapeResponse JSON. 'clean_json' = Sniper Mode: lean token-optimised JSON with only title, key paragraphs, code blocks and metadata — strips 100% of nav/footer/boilerplate noise."
                    },
                    "include_raw_html": {"type": "boolean", "default": false, "description": "Include raw HTML in JSON responses. Note: in NeuroSiphon or aggressive mode this is force-disabled to prevent token leaks."},
                    "use_proxy": {"type": "boolean", "default": false},
                    "quality_mode": {"type": "string", "enum": ["balanced", "aggressive", "high"], "default": "balanced"},
                    "extract_app_state": {
                        "type": "boolean",
                        "default": false,
                        "description": "Force-return embedded SPA hydration JSON (Next/Nuxt/Remix). When true and state exists, this becomes the ONLY content (DOM extras are dropped) for maximum token efficiency."
                    }
                },
                "required": ["url"]
            }),
            icons: vec![CORTEX_SCOUT_ICON],
        },
        ToolCatalogEntry {
            name: "scrape_batch",
            title: "Batch Web Fetch",
            description: "Fetch many URLs in parallel and return clean outputs for agents. Use for research runs and evidence capture. Defaults to direct/no-proxy mode unless use_proxy=true is explicitly requested.",
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
            icons: vec![CORTEX_SCOUT_ICON],
        },
        ToolCatalogEntry {
            name: "deep_research",
            title: "Deep Research",
            description: "Multi-hop search + scrape + semantic-filter research pipeline. \
Expands your query into sub-queries, searches multiple engines, reranks results by relevance, \
batch-scrapes the top sources, applies semantic filtering to keep only relevant content, \
then optionally follows links from those pages for deeper coverage. \
Results are logged to research_history for later recall. \
Defaults to direct/no-proxy mode. Use proxy: true only after confirmed 403/429/rate-limit/IP-block conditions or during large research runs that require proxy rotation.\
\n\
🧠 LLM Synthesis: Automatically enabled when OPENAI_API_KEY is set. \
For fully LOCAL / offline synthesis set OPENAI_BASE_URL to an OpenAI-compatible endpoint \
(e.g. Ollama: http://localhost:11434/v1  |  LM Studio: http://localhost:1234/v1) \
and set OPENAI_API_KEY=local (any non-empty value). \
Set DEEP_RESEARCH_LLM_MODEL to choose the model (default: gpt-4o-mini). \
\
On/Off: set DEEP_RESEARCH_ENABLED=0 to hide this tool from the catalog at runtime. \
Strip from binary entirely: cargo build --no-default-features.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The research question or topic to investigate."
                    },
                    "depth": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 3,
                        "default": 1,
                        "description": "Number of search+scrape hops (1=single pass, 2=follow links from first-hop pages, 3=follow links two levels deep). Higher depth = more sources but slower."
                    },
                    "max_sources": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 20,
                        "default": 10,
                        "description": "Maximum sources to scrape per hop. Total sources = depth × max_sources (upper bound)."
                    },
                    "max_chars_per_source": {
                        "type": "integer",
                        "default": 20000,
                        "description": "Maximum characters extracted from each source page."
                    },
                    "max_concurrent": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 10,
                        "default": 3,
                        "description": "Maximum concurrent scrape connections. Keep low (2-3) for home use to avoid IP blocks."
                    },
                    "use_proxy": {
                        "type": "boolean",
                        "default": false,
                        "description": "Route scraping through proxy to avoid IP rate-limiting on large research runs."
                    },
                    "relevance_threshold": {
                        "type": "number",
                        "minimum": 0.0,
                        "maximum": 1.0,
                        "default": 0.25,
                        "description": "Semantic similarity threshold for content filtering [0.0–1.0]. Lower = keep more content; higher = keep only highly relevant chunks. Requires memory/LanceDB enabled."
                    },
                    "quality_mode": {
                        "type": "string",
                        "enum": ["balanced", "aggressive"],
                        "default": "balanced",
                        "description": "Scraper quality. Use aggressive for JS-heavy sites (slower but more thorough)."
                    }
                },
                "required": ["query"]
            }),
            icons: vec![CORTEX_SCOUT_ICON],
        },
        ToolCatalogEntry {
            name: "crawl_website",
            title: "Crawl Website (Link Map)",
            description: "Bounded crawl to map a site's link structure before targeted fetching. \
Use this when you know a doc site's index URL and need to discover the right sub-page before fetching — do NOT assume a single web_fetch of the index is sufficient. \
If the start URL returns NEED_HITL, the crawl aborts early with a structured error.",
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
                    "max_chars": {
                        "type": "integer", "minimum": 1,
                        "description": "Max total JSON output characters for the crawl result (default 10000). Increase when crawling many pages to avoid truncation."
                    },
                    "use_proxy": {"type": "boolean", "default": false},
                    "quality_mode": {"type": "string", "enum": ["balanced", "aggressive"], "default": "balanced"}
                },
                "required": ["url"]
            }),
            icons: vec![CORTEX_SCOUT_ICON],
        },
        ToolCatalogEntry {
            name: "extract_structured",
            title: "Extract Structured Fields",
            description: "Schema-driven extraction into JSON fields. Use after web_fetch when you need a JSON object rather than free text. \
⛔ CONSTRAINT: use ONLY on structured HTML pages (official docs, articles, MDN-style pages). \
Do NOT use on raw .md, .json, .txt, or .rst files — fields will return null and confidence will be low. \
For raw Markdown sources, use web_fetch with output_format: clean_json instead. \
⚠️ AUTO-WARN: if the URL is a raw markdown/text file, a raw_markdown_url warning is automatically injected into the response.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string"},
                    "schema": {"type": "array", "items": {"type": "object"}},
                    "prompt": {"type": "string"},
                    "strict": {
                        "type": "boolean",
                        "default": true,
                        "description": "Strict schema mode: enforce schema shape exactly (no extra keys). Missing array fields become [], missing scalars become null."
                    },
                    "max_chars": {"type": "integer"},
                    "use_proxy": {"type": "boolean", "default": false},
                    "quality_mode": {"type": "string", "enum": ["balanced", "aggressive", "high"], "default": "balanced"},
                    "placeholder_word_threshold": {
                        "type": "integer", "minimum": 1, "default": 10,
                        "description": "Word-count threshold below which content is considered sparse (possible JS-only placeholder). Default 10. Lower = less sensitive."
                    },
                    "placeholder_empty_ratio": {
                        "type": "number", "minimum": 0, "maximum": 1, "default": 0.9,
                        "description": "Fraction of schema fields that must be null/empty before confidence is forced to 0.0. Default 0.9. Raise toward 1.0 to reduce false positives."
                    }
                },
                "required": ["url"]
            }),
            icons: vec![CORTEX_SCOUT_ICON],
        },

        ToolCatalogEntry {
            name: "fetch_then_extract",
            title: "Fetch Then Extract (Single Call)",
            description: "Fetch + extract in a single call to reduce latency and token usage. \
Schema-first: provide `schema` (preferred) or a schema-like `prompt`. \
When `strict=true`, output matches requested schema exactly (no schema drift).",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string"},
                    "schema": {"type": "array", "items": {"type": "object"}},
                    "prompt": {"type": "string"},
                    "strict": {"type": "boolean", "default": true},
                    "max_chars": {"type": "integer"},
                    "output_format": {"type": "string", "enum": ["json", "text"], "default": "json"},
                    "use_proxy": {"type": "boolean", "default": false},
                    "quality_mode": {"type": "string", "enum": ["balanced", "aggressive", "high"], "default": "balanced"},
                    "placeholder_word_threshold": {
                        "type": "integer", "minimum": 1, "default": 10,
                        "description": "Word-count threshold below which content is considered sparse (possible JS-only placeholder). Default 10."
                    },
                    "placeholder_empty_ratio": {
                        "type": "number", "minimum": 0, "maximum": 1, "default": 0.9,
                        "description": "Fraction of schema fields that must be null/empty before confidence is forced to 0.0. Default 0.9."
                    }
                },
                "required": ["url"]
            }),
            icons: vec![CORTEX_SCOUT_ICON],
        },
       ToolCatalogEntry {
        name: "research_history",
        title: "Search Past Research", 
        description: "Semantic research memory (LanceDB). Use to retrieve past searches/scrapes and avoid re-fetching the same sources. \
⚠️ AGENT RULE: call this BEFORE any web_search or web_fetch — if similarity score >= 0.60 on a returned entry, use that cached result directly and skip the live fetch entirely.",
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
            icons: vec![CORTEX_SCOUT_ICON],
            },
        ToolCatalogEntry {
            name: "proxy_manager",
            title: "Proxy Control",
            description: "Manage proxies (grab/list/status/switch/test). Use when a site rate-limits or blocks your IP. \
⚠️ TRIGGER RULE: when web_fetch or web_search returns a 403, 429, or rate-limit error, call this tool with action: grab to rotate your IP, then immediately retry the failed call with use_proxy: true. \
Do NOT wait for further failures — rotate on first block signal.",
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
            icons: vec![CORTEX_SCOUT_ICON],
        },
        ToolCatalogEntry {
            name: "non_robot_search",
            title: "Web Fetch (HITL Anti-Bot)", 
            description: "LAST RESORT for heavy anti-bot (Cloudflare/LinkedIn/CAPTCHA/login). Opens a real browser on the host. Use web_fetch first; use this only when automation is blocked and a human can solve the challenge.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                "url": {"type": "string", "description": "The URL that is blocking standard bot access."},
                "output_format": {"type": "string", "enum": ["text", "json"], "default": "json"},
                "max_chars": {"type": "integer", "minimum": 1, "default": 10000},
                "use_proxy": {"type": "boolean", "default": false},
                "quality_mode": {"type": "string", "enum": ["balanced", "aggressive", "high"], "default": "balanced"},
                "captcha_grace_seconds": {"type": "integer", "minimum": 1, "default": 5, "description": "Seconds to wait for a human to solve a CAPTCHA before checking content again."},
                "human_timeout_seconds": {"type": "integer", "minimum": 1, "default": 1200, "description": "Soft timeout window (seconds) to wait for human interaction/login. The browser only closes after the user clicks FINISH & RETURN."},
                "user_profile_path": {"type": "string", "description": "Path to a real browser profile (Chrome/Brave) to bypass login walls using existing cookies."},
                "auto_scroll": {"type": "boolean", "default": false, "description": "Scroll down to trigger lazy-loaded items (critical for infinite-scroll sites)."},
                "wait_for_selector": {"type": "string", "description": "Wait for this CSS element to ensure the page has fully bypassed the bot wall."},
                "keep_open": {"type": "boolean", "default": false, "description": "Leave the browser window open after content is extracted. Useful for multi-step workflows."},
                "instruction_message": {"type": "string", "description": "Message displayed inside the browser overlay telling the user what to do (e.g. 'Please log in to GitHub')."}
        },
        "required": ["url"]
        }),
        icons: vec![CORTEX_SCOUT_ICON],
    },
        ToolCatalogEntry {
            name: "visual_scout",
            title: "Visual Page Scout (Screenshot)",
            description: "🔭 Take a headless screenshot of a URL and return it as a base64 PNG for Vision-AI analysis. \
Use this in Step 2 of the Auth-Gatekeeper Protocol when `web_fetch` returns `auth_risk_score >= 0.4`. \
Inspect the screenshot to confirm whether a login modal/gate is present before escalating to `human_auth_session`. \
⚡ TOKEN TIP: when analysing the screenshot only look for login buttons, forms, and modals — do NOT describe the page aesthetics.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string", "description": "The URL to photograph."},
                    "proxy_url": {"type": "string", "description": "Optional proxy (http/socks5) to use for the screenshot request."},
                    "width": {"type": "integer", "minimum": 320, "maximum": 2560, "default": 1280, "description": "Viewport width in pixels."},
                    "height": {"type": "integer", "minimum": 200, "maximum": 1600, "default": 800, "description": "Viewport height in pixels."},
                    "output_format": {"type": "string", "enum": ["json", "text"], "default": "json", "description": "'json' returns structured metadata + base64 PNG. 'text' returns a plain summary with the base64 appended."}
                },
                "required": ["url"]
            }),
            icons: vec![CORTEX_SCOUT_ICON],
        },
        ToolCatalogEntry {
            name: "human_auth_session",
            title: "Auth Session (HITL Login + Session Save)",
            description: "🔐 The Auth-Gatekeeper's escalation tool. Opens a real visible browser, shows the user a clear instruction card, waits for them to complete login, then scrapes the authenticated content. \
After a successful auth flow, cookies are automatically persisted to `~/.cortex-scout/sessions/{domain}.json` so future requests to the same domain skip the HITL step entirely. \
Use ONLY after `visual_scout` has confirmed AUTH_REQUIRED — never as a first attempt. \
Send `instruction_message` to tell the user exactly what to log in to and why, e.g. *'Please log in to GitHub so I can read the private Discussions.'*",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string", "description": "The auth-walled URL to access after login."},
                    "instruction_message": {
                        "type": "string",
                        "description": "Clear instruction displayed in the browser overlay. Example: 'Please log in to GitHub so I can read the Discussions in this repo.'"
                    },
                    "keep_open": {
                        "type": "boolean",
                        "default": false,
                        "description": "Leave the browser window open after content is extracted."
                    },
                    "output_format": {"type": "string", "enum": ["text", "json"], "default": "json"},
                    "max_chars": {"type": "integer", "minimum": 1, "default": 10000},
                    "use_proxy": {"type": "boolean", "default": false},
                    "quality_mode": {"type": "string", "enum": ["balanced", "aggressive", "high"], "default": "balanced"},
                    "captcha_grace_seconds": {"type": "integer", "minimum": 1, "default": 5},
                    "human_timeout_seconds": {"type": "integer", "minimum": 1, "default": 1200, "description": "Soft timeout window (seconds) to wait for login completion. The browser closes only after the user clicks FINISH & RETURN."},
                    "user_profile_path": {"type": "string", "description": "Persistent Chrome/Brave profile path. When provided, existing cookies are reused automatically."},
                    "auto_scroll": {"type": "boolean", "default": false},
                    "wait_for_selector": {"type": "string"}
                },
                "required": ["url"]
            }),
            icons: vec![CORTEX_SCOUT_ICON],
        }
    ];

    // ── Phase 18: Playwright Killer — stateful browser automation ─────────────
    tools.push(ToolCatalogEntry {
        name: "browser_automate",
        title: "Browser Automate (Omni-Tool)",
        description: "The ultimate browser automation + QA tool. Executes an ordered sequence of \
actions in a stateful Brave browser session that persists between calls. \
Runs silently in the background (--headless=new) using a dedicated isolated agent profile \
(~/.cortex-scout/agent_profile) so it never disrupts the user's active desktop session. \
Cookies and login state persist across close/reopen cycles. \
The session stays open until scout_browser_close is called. \
\n\
Supported actions (steps array):\n\
• navigate          — go to a URL and wait for network-idle (target=URL)\n\
• click             — click a CSS selector (target=selector)\n\
• type              — click + type text into a selector (target=selector, value=text)\n\
• press_key         — dispatch a key event (key=\"Enter\"|\"Escape\"|\"Tab\"|\"ArrowDown\"|...)\n\
• scroll            — scroll the viewport (direction=\"down\"|\"up\"|\"bottom\"|\"top\", pixels=500)\n\
• evaluate          — run arbitrary JS, capture return value (value=script)\n\
• wait_for_selector — poll until selector appears (target=selector, timeout_ms=10000)\n\
• snapshot          — returns title/URL/headings/inputs/buttons/links/bodyText snapshot\n\
• screenshot        — returns inline base64 PNG of current viewport\n\
• assert            — fail-fast DOM assertion; HALTS the sequence on failure \
(target=selector, value=expected_text, condition=\"contains_text\"|\"is_visible\"|\"is_hidden\")\n\
• mock_api          — intercept fetch+XHR matching a glob URL pattern and return a fake JSON \
response (url_pattern=\"*api/v1/users*\", response_json=\"{...}\", status_code=200); \
active on current page AND all future navigations",
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "steps": {
                    "type": "array",
                    "description": "Ordered list of automation actions to execute sequentially.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "action": {
                                "type": "string",
                                "enum": ["navigate", "click", "type", "press_key", "scroll",
                                         "evaluate", "wait_for_selector", "snapshot", "screenshot",
                                         "assert", "mock_api"],
                                "description": "The action to perform."
                            },
                            "target": {
                                "type": "string",
                                "description": "URL (for navigate) or CSS selector (for click, type, wait_for_selector, assert)."
                            },
                            "value": {
                                "type": "string",
                                "description": "Text to type (for type), JS expression (for evaluate), or expected text/state (for assert)."
                            },
                            "condition": {
                                "type": "string",
                                "enum": ["contains_text", "is_visible", "is_hidden"],
                                "description": "Assertion condition (for assert). Default: contains_text."
                            },
                            "url_pattern": {
                                "type": "string",
                                "description": "Glob URL pattern to intercept (for mock_api). Supports * and ? wildcards. Example: '*api/v1/users*'."
                            },
                            "response_json": {
                                "type": "string",
                                "description": "JSON string to return as the mocked API response body (for mock_api)."
                            },
                            "status_code": {
                                "type": "integer",
                                "default": 200,
                                "description": "HTTP status code for the mocked response (for mock_api). Default 200."
                            },
                            "key": {
                                "type": "string",
                                "description": "Key to press (for press_key). Examples: \"Enter\", \"Escape\", \"Tab\", \"ArrowDown\", \"Space\", \"Backspace\", \"F5\"."
                            },
                            "direction": {
                                "type": "string",
                                "enum": ["down", "up", "bottom", "top"],
                                "default": "down",
                                "description": "Scroll direction (for scroll). 'bottom'/'top' jump to the page edge; 'down'/'up' scroll by pixels."
                            },
                            "pixels": {
                                "type": "integer",
                                "default": 500,
                                "description": "Pixels to scroll (for scroll with direction=down or up). Default 500."
                            },
                            "timeout_ms": {
                                "type": "integer",
                                "default": 10000,
                                "description": "Timeout for wait_for_selector in milliseconds (default 10000)."
                            }
                        },
                        "required": ["action"]
                    }
                }
            },
            "required": ["steps"]
        }),
        icons: vec![CORTEX_SCOUT_ICON],
    });

    tools.push(ToolCatalogEntry {
        name: "browser_close",
        title: "Browser Close (Cleanup)",
        description: "Gracefully terminate the stateful Brave browser session started by \
scout_browser_automate. Call this when you are done with all automation steps to free resources.",
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {}
        }),
        icons: vec![CORTEX_SCOUT_ICON],
    });

    // ── Phase 20: Agent Auth Portal ───────────────────────────────────────────
    tools.push(ToolCatalogEntry {
        name: "agent_profile_auth",
        title: "Agent Auth Portal (HITL Login Bootstrap)",
        description: "🔓 Launch the silent agent profile in a VISIBLE browser window so a human can \
complete an initial login, OAuth flow, 2FA, or CAPTCHA challenge — in the agent's name. \
\n\
Use this ONLY when scout_browser_automate is blocked because the agent profile has no \
authenticated session for a domain (e.g., first-time Google/AWS/GitHub login). \
\n\
Workflow:\n\
1. Closes the headless automation session to release the SingletonLock.\n\
2. Opens a VISIBLE Brave window pointing to the same ~/.cortex-scout/agent_profile.\n\
3. Navigates to `url` and shows the window to the user.\n\
4. Waits up to `timeout_secs` seconds (default 120) — the user completes the login.\n\
5. Closes the window; cookies are flushed to the persistent profile.\n\
6. Future scout_browser_automate calls will reuse those cookies silently.",
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL to open (e.g. the login page for the service)."
                },
                "instruction": {
                    "type": "string",
                    "description": "Human-readable instruction shown in server logs telling the user what to do, e.g. 'Please log in to AWS so I can automate it'."
                },
                "timeout_secs": {
                    "type": "integer",
                    "minimum": 10,
                    "maximum": 600,
                    "default": 120,
                    "description": "How many seconds to keep the window open waiting for the user. Default 120 (2 minutes). The window also closes immediately if the user closes it manually."
                }
            },
            "required": ["url"]
        }),
        icons: vec![CORTEX_SCOUT_ICON],
    });

    // Build-time + runtime gate: remove deep_research from the catalog when disabled.
    // This makes it invisible to agents (list_tools returns nothing) and unreachable
    // (call_tool returns "Unknown tool") without touching any other codepath.
    if !deep_research_enabled() {
        tools.retain(|t| t.name != "deep_research");
    }

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
