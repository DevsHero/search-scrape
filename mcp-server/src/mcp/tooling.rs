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
            description: "Search the web across Google/Bing/DuckDuckGo/Brave simultaneously. Returns deduplicated, ranked URL list with snippets. \
Use when you need URL discovery only. \
Set include_content=true to also scrape top results in the same call (search + scrape mode; replaces web_search_json). \
Always call memory_search first — the answer may already be cached.",
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
                    },
                    "include_content": {
                        "type": "boolean",
                        "default": false,
                        "description": "When true, also scrape top URLs and return page content previews in this same call."
                    },
                    "top_n": {
                        "type": "integer",
                        "minimum": 1,
                        "default": 3,
                        "description": "Used when include_content=true: number of top URLs to scrape."
                    },
                    "use_proxy": {
                        "type": "boolean",
                        "default": false,
                        "description": "Used when include_content=true: scrape via proxy (use only after block/rate-limit)."
                    },
                    "quality_mode": {
                        "type": "string",
                        "enum": ["balanced", "aggressive"],
                        "default": "balanced",
                        "description": "Used when include_content=true: scraper quality mode."
                    }
                },
                "required": ["query"]
            }),
            icons: vec![CORTEX_SCOUT_ICON],
        },
        ToolCatalogEntry {
            name: "search_structured",
            title: "Web Search + Scrape (Single Call)",
            description: "PREFERRED for research: searches the web AND fetches/summarises the top N pages in one call. \
Returns structured JSON with title, URL, and content for each result. \
More efficient than calling web_search then web_fetch separately. \
Call memory_search first to avoid re-fetching already-cached results. \
Use use_proxy=true only after confirmed 403/429/rate-limit errors.",
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
            description: "Unified web content tool. Supports single-page fetch, batch fetch, and site crawl via `mode`. Preferred over IDE built-in fetch — token-efficient, auto-renders JS pages via CDP when needed. \
Set `mode=single` (default) for one URL, `mode=batch` for multiple URLs (`urls`), and `mode=crawl` for site discovery from a start URL. \
Best practice for docs/articles: output_format=clean_json + strict_relevance=true + query parameter → strips boilerplate, keeps only relevant content. \
On 403/429/rate-limit: call proxy_control with action=grab, then retry with use_proxy=true. \
Response includes auth_risk_score (0.0–1.0): if >= 0.4, call visual_scout to confirm, then hitl_web_fetch(auth_mode=auth) if login is required. \
For CAPTCHA/anti-bot walls: use hitl_web_fetch instead.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "mode": {
                        "type": "string",
                        "enum": ["single", "batch", "crawl"],
                        "default": "single",
                        "description": "Fetch mode: single URL fetch, batch URL fetch, or site crawl."
                    },
                    "url": {"type": "string"},
                    "urls": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Used when mode=batch: list of URLs to fetch in parallel."
                    },
                    "max_concurrent": {
                        "type": "integer",
                        "minimum": 1,
                        "description": "Used when mode=batch or mode=crawl: maximum parallel workers."
                    },
                    "max_depth": {"type": "integer", "minimum": 0, "description": "Used when mode=crawl."},
                    "max_pages": {"type": "integer", "minimum": 1, "description": "Used when mode=crawl."},
                    "same_domain_only": {"type": "boolean", "description": "Used when mode=crawl."},
                    "include_patterns": {"type": "array", "items": {"type": "string"}, "description": "Used when mode=crawl."},
                    "exclude_patterns": {"type": "array", "items": {"type": "string"}, "description": "Used when mode=crawl."},
                    "max_chars_per_page": {"type": "integer", "minimum": 1, "description": "Used when mode=crawl."},
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
                        "description": "Output format. single mode: text/json/clean_json. batch/crawl modes: text/json (clean_json not applied)."
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
                "required": []
            }),
            icons: vec![CORTEX_SCOUT_ICON],
        },
        ToolCatalogEntry {
            name: "scrape_batch",
            title: "Batch Web Fetch",
            description: "Legacy alias for `web_fetch` with `mode=batch`. Fetch multiple URLs in parallel and return clean text/JSON for each. \
Note: results are returned in completion order (fastest first), not input order. \
Use use_proxy=true if sites are rate-limiting.",
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
            description: "Full autonomous research pipeline: expands the query into sub-queries, searches multiple engines, reranks results, batch-scrapes top sources, applies semantic filtering, then optionally follows links for deeper coverage. \
Results are automatically saved to memory_search history for future recall. \
Use for complex research topics requiring multiple sources. Avoid for simple single-URL lookups — use web_fetch instead. \
LLM synthesis is automatic when OPENAI_API_KEY is set (set DEEP_RESEARCH_ENABLED=0 to disable the tool).",
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
            title: "Crawl Website (Link Discovery)",
            description: "Legacy alias for `web_fetch` with `mode=crawl`. BFS-crawl a website to discover its link structure and page content. \
Do NOT use for single-page fetching — use web_fetch instead. \
Aborts early with a structured error if the start URL requires human login (NEED_HITL).",
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
            description: "Primary structured extraction tool. Fetches a URL and extracts specific named fields into a JSON object using a schema. \
Do NOT use on raw .md/.json/.txt files — use web_fetch with output_format=clean_json instead. \
Note: confidence score indicates extraction quality; check warnings field for null fields. \
`fetch_then_extract` is a legacy alias/variant for compatibility.",
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
            title: "Fetch + Extract (Single Call)",
            description: "Legacy alias/variant of structured extraction. Prefer `extract_fields` as the primary extraction interface. \
Fetch a URL and extract structured fields in a single call (lower latency than calling web_fetch + extract_fields separately). \
Provide schema (preferred) or a prompt describing the fields to extract. \
strict=true enforces schema shape exactly — missing fields become null/[]. \
Best for well-structured HTML pages; less reliable on heavily JS-rendered or navigation-heavy pages.",
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
        title: "Search Past Research (Memory)",
        description: "Semantic memory search over past web searches and page scrapes (stored in LanceDB). \
Call this BEFORE web_search or web_fetch — if any result has similarity >= 0.60, use it directly and skip the live request. \
    Past results from deep_research and web_search(include_content=true) are saved automatically. \
Use entry_type filter to search only past searches ('search') or past scrapes ('scrape').",
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
            description: "Manage proxy pool: grab a fresh proxy IP, list available proxies, check status, switch, or test connectivity. \
Action=grab: rotate to a new proxy when web_fetch or web_search returns 403/429/rate-limit — then retry with use_proxy=true. \
Action=status: check current proxy and pool health. Action=list: see all available proxies.",
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
            title: "Web Fetch (HITL — Human Solves Anti-Bot)",
            description: "Unified HITL escalation tool. Use `auth_mode=challenge` (default) for CAPTCHA/Cloudflare bypass, or `auth_mode=auth` for login-focused session flow with cookie persistence. \
LAST RESORT: opens a real visible browser window so a human can solve anti-bot or auth walls that block automated fetching. \
Always try web_fetch first; only use this when web_fetch returns an anti-bot/CAPTCHA block.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                "auth_mode": {
                    "type": "string",
                    "enum": ["challenge", "auth"],
                    "default": "challenge",
                    "description": "challenge: anti-bot/CAPTCHA solving. auth: login-focused flow with session persistence."
                },
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
            description: "Take a headless screenshot of a URL. Returns the screenshot saved to a local temp file path (not base64-embedded). \
Use this when web_fetch returns auth_risk_score >= 0.4 to visually confirm whether a login/CAPTCHA wall is present before escalating to hitl_web_fetch(auth_mode=auth). \
The response contains screenshot_path (local file), page_title, and a hint about auth wall detection. \
Does NOT return base64 image data inline — the PNG is stored at screenshot_path on disk.",
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
            title: "Auth Session (HITL Login + Cookie Persistence)",
            description: "Legacy auth-focused alias of HITL flow. Equivalent to `hitl_web_fetch` with `auth_mode=auth`. Opens a real visible browser, waits for user to log in, then scrapes the content using the authenticated session. \
Cookies are saved to ~/.cortex-scout/sessions/{domain}.json — future web_fetch calls to the same domain auto-use these cookies without HITL. \
Use ONLY after visual_scout confirms a login wall (AUTH_REQUIRED). Always try web_fetch first. \
Set instruction_message to tell the user exactly what to do, e.g. 'Please log in to GitHub to access private Discussions.'",
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
        description: "Stateful headless browser automation. Executes an ordered sequence of steps in a persistent Brave browser session (~/.cortex-scout/agent_profile). \
Runs headless and keeps login/cookie state across calls until scout_browser_close is used. \
Supports Playwright-style workflows inside one omni-tool: navigation, hover/click/type/wait, locator actions, assertions, tabs, resize, screenshots, PDF export, file upload, form fill, dialog policy, coordinate mouse actions, route mocking, console/network capture, storage checkpoints, cookie/localStorage/sessionStorage CRUD, and verification helpers. \
run_flow can execute a nested flow in one step for reusable scenario blocks. \
Use for JS-rendered scraping, smoke validation, interactive debugging, and browser-state fixture setup. \
First-time login to a service: use scout_agent_profile_auth to authenticate the profile, then use this tool.",
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
                                "enum": ["navigate", "navigate_back", "click", "hover", "type", "press_key", "scroll",
                                         "run_flow",
                                         "evaluate", "run_code", "wait_for_selector", "wait_for_locator", "wait_for", "snapshot", "screenshot", "take_screenshot", "resize",
                                         "select_option", "drag_drop", "file_upload", "fill_form", "handle_dialog", "tabs", "pdf_save",
                                         "mouse_click_xy", "mouse_down", "mouse_drag_xy", "mouse_move_xy", "mouse_up", "mouse_wheel",
                                         "click_locator", "type_locator", "assert_locator",
                                         "assert", "generate_locator", "verify_element_visible", "verify_list_visible", "verify_text_visible", "verify_value",
                                         "trace_start", "start_tracing", "trace_stop", "stop_tracing", "trace_export",
                                         "network_tap", "network_dump", "network_requests", "network_state_set",
                                         "mock_api", "route", "route_list", "unroute", "console_tap", "console_dump", "console_messages",
                                         "storage_clear", "storage_state_export", "storage_state", "storage_state_import", "set_storage_state",
                                         "storage_checkpoint", "storage_rollback",
                                         "cookie_clear", "cookie_delete", "cookie_get", "cookie_list", "cookie_set",
                                         "localstorage_clear", "localstorage_delete", "localstorage_get", "localstorage_list", "localstorage_set",
                                         "sessionstorage_clear", "sessionstorage_delete", "sessionstorage_get", "sessionstorage_list", "sessionstorage_set"],
                                "description": "The action to perform."
                            },
                            "steps": {
                                "type": "array",
                                "items": {"type": "object"},
                                "description": "Used by run_flow: nested action objects executed sequentially."
                            },
                            "target": {
                                "type": "string",
                                "description": "URL (navigate), CSS selector, locator value, export path, route pattern, tab URL, checkpoint key, or storage scope depending on action."
                            },
                            "value": {
                                "type": "string",
                                "description": "Typed text, JS expression, expected assertion text, select option, destination selector, nested JSON payload, tab sub-action, or mocked response body depending on action."
                            },
                            "condition": {
                                "type": "string",
                                "enum": ["contains_text", "is_visible", "is_hidden"],
                                "description": "Assertion condition for assert/assert_locator. Default: contains_text. Assertions auto-retry until timeout_ms."
                            },
                            "locator": {
                                "type": "string",
                                "enum": ["css", "text", "role", "label", "placeholder", "testid"],
                                "description": "Locator strategy for *_locator actions. Default: css."
                            },
                            "name": {
                                "type": "string",
                                "description": "Optional accessible name for role locators (e.g., role=button + name=Submit)."
                            },
                            "exact": {
                                "type": "boolean",
                                "description": "Whether locator text matching should be exact. Default false (substring match)."
                            },
                            "scope": {
                                "type": "string",
                                "description": "Optional CSS selector scope used to limit locator search within a subtree."
                            },
                            "filename": {
                                "type": "string",
                                "description": "Optional output path for snapshot/screenshot/pdf/console_dump/network_dump/storage_state_export."
                            },
                            "type": {
                                "type": "string",
                                "description": "Screenshot format (png/jpeg/webp), fill_form field type, or verify_value control type depending on action."
                            },
                            "fullPage": {
                                "type": "boolean",
                                "description": "When true, screenshot captures beyond the viewport."
                            },
                            "width": {
                                "type": "integer",
                                "description": "Viewport width for resize."
                            },
                            "height": {
                                "type": "integer",
                                "description": "Viewport height for resize."
                            },
                            "url_pattern": {
                                "type": "string",
                                "description": "Glob URL pattern to intercept (for mock_api). Supports * and ? wildcards. Example: '*api/v1/users*'."
                            },
                            "pattern": {
                                "type": "string",
                                "description": "Alias for url_pattern or unroute target pattern."
                            },
                            "method": {
                                "type": "string",
                                "description": "Optional HTTP method constraint for mock_api (e.g., GET, POST)."
                            },
                            "response_json": {
                                "type": "string",
                                "description": "JSON string to return as the mocked API response body (for mock_api)."
                            },
                            "response_headers": {
                                "type": "object",
                                "description": "Optional response headers object for mock_api replies."
                            },
                            "remove_headers": {
                                  "type": "array",
                                  "items": {"type": "string"},
                                  "description": "Optional header names to strip when defining a mocked route."
                            },
                            "status_code": {
                                "type": "integer",
                                "default": 200,
                                "description": "HTTP status code for the mocked response (for mock_api). Default 200."
                            },
                            "delay_ms": {
                                "type": "integer",
                                "minimum": 0,
                                "description": "Optional delay (ms) before fulfilling a mocked response."
                            },
                            "once": {
                                "type": "boolean",
                                "description": "If true, mock_api applies only to the first matching request."
                            },
                            "state": {
                                "type": "string",
                                "enum": ["online", "offline"],
                                "description": "Network state for network_state_set."
                            },
                            "includeStatic": {
                                "type": "boolean",
                                "description": "Include successful resource timing entries in network_dump/network_requests."
                            },
                            "level": {
                                "type": "string",
                                "enum": ["error", "warning", "info", "debug"],
                                "description": "Console severity filter for console_dump/console_messages."
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
                            "time": {
                                "type": "number",
                                "description": "Seconds to wait when using wait_for with time-based sleep."
                            },
                            "text": {
                                "type": "string",
                                "description": "Text to wait for or verify, depending on action."
                            },
                            "textGone": {
                                "type": "string",
                                "description": "Text that must disappear for wait_for."
                            },
                            "button": {
                                "type": "string",
                                "enum": ["left", "right", "middle", "back", "forward"],
                                "description": "Mouse button for click/mouse actions."
                            },
                            "doubleClick": {
                                "type": "boolean",
                                "description": "When true, click behaves as a double-click."
                            },
                            "clickCount": {
                                "type": "integer",
                                "description": "Explicit mouse click count override."
                            },
                            "delay": {
                                "type": "integer",
                                "description": "Optional click delay in milliseconds."
                            },
                            "modifiers": {
                                "type": "array",
                                "items": {"type": "string", "enum": ["Alt", "Control", "Ctrl", "Meta", "Command", "ControlOrMeta", "Shift"]},
                                "description": "Keyboard modifiers to hold during click or mouse actions."
                            },
                            "submit": {
                                "type": "boolean",
                                "description": "When true, type presses Enter after input."
                            },
                            "slowly": {
                                "type": "boolean",
                                "description": "When true, type enters text character-by-character."
                            },
                            "values": {
                                "type": "array",
                                "items": {"type": "string"},
                                "description": "Option list for select_option. First value is used."
                            },
                            "paths": {
                                "type": "array",
                                "items": {"type": "string"},
                                "description": "Absolute file paths to upload for file_upload."
                            },
                            "fields": {
                                "type": "array",
                                "description": "Field definitions for fill_form.",
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "name": {"type": "string"},
                                        "selector": {"type": "string"},
                                        "target": {"type": "string"},
                                        "type": {"type": "string", "enum": ["textbox", "checkbox", "radio", "combobox", "slider"]},
                                        "value": {}
                                    }
                                }
                            },
                            "accept": {
                                "type": "boolean",
                                "description": "Whether handle_dialog should accept dialogs."
                            },
                            "promptText": {
                                "type": "string",
                                "description": "Prompt text returned when handle_dialog accepts window.prompt."
                            },
                            "tab_action": {
                                "type": "string",
                                "enum": ["list", "new", "select", "close"],
                                "description": "Optional explicit sub-action for tabs. Value can also be passed via 'value'."
                            },
                            "index": {
                                "type": "integer",
                                "description": "Tab index used by tabs select/close."
                            },
                            "x": {
                                "type": "number",
                                "description": "X coordinate for mouse actions."
                            },
                            "y": {
                                "type": "number",
                                "description": "Y coordinate for mouse actions."
                            },
                            "deltaX": {
                                "type": "number",
                                "description": "Horizontal wheel delta for mouse_wheel."
                            },
                            "deltaY": {
                                "type": "number",
                                "description": "Vertical wheel delta for mouse_wheel."
                            },
                            "startX": {
                                "type": "number",
                                "description": "Drag start X coordinate for mouse_drag_xy."
                            },
                            "startY": {
                                "type": "number",
                                "description": "Drag start Y coordinate for mouse_drag_xy."
                            },
                            "endX": {
                                "type": "number",
                                "description": "Drag end X coordinate for mouse_drag_xy."
                            },
                            "endY": {
                                "type": "number",
                                "description": "Drag end Y coordinate for mouse_drag_xy."
                            },
                            "role": {
                                "type": "string",
                                "description": "Accessible role used by verify_element_visible."
                            },
                            "accessibleName": {
                                "type": "string",
                                "description": "Accessible name used with verify_element_visible role matching."
                            },
                            "items": {
                                "type": "array",
                                "items": {"type": "string"},
                                "description": "Expected visible list items for verify_list_visible."
                            },
                            "domain": {
                                "type": "string",
                                "description": "Cookie domain filter or set target."
                            },
                            "path": {
                                "type": "string",
                                "description": "Cookie path filter or set target."
                            },
                            "secure": {
                                "type": "boolean",
                                "description": "Cookie secure flag for cookie_set."
                            },
                            "httpOnly": {
                                "type": "boolean",
                                "description": "Cookie HttpOnly flag for cookie_set."
                            },
                            "sameSite": {
                                "type": "string",
                                "enum": ["Strict", "Lax", "None"],
                                "description": "Cookie same-site mode for cookie_set."
                            },
                            "expires": {
                                "type": "number",
                                "description": "Cookie expiry as Unix epoch seconds for cookie_set."
                            },
                            "timeout_ms": {
                                "type": "integer",
                                "default": 10000,
                                "description": "Timeout for waits, actions, and assertions in milliseconds."
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
        description: "Close the persistent headless browser session started by scout_browser_automate. Call when done with all automation steps to free memory and release the browser process.",
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
        description: "Bootstrap the agent browser profile by showing a VISIBLE browser for a human to complete first-time login, OAuth, 2FA, or CAPTCHA. \
Use ONLY when scout_browser_automate is blocked because the agent profile has no session for a domain. \
This closes the headless session temporarily, opens a visible Brave window at `url`, waits for login (up to timeout_secs), then saves cookies back to the agent profile. \
After this completes, scout_browser_automate can reuse those cookies silently.",
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

#[cfg(test)]
mod tests {
    use super::tool_catalog;
    use serde_json::Value;

    fn assert_arrays_have_items(node: &Value, path: &str) {
        match node {
            Value::Object(map) => {
                if map.get("type").and_then(|v| v.as_str()) == Some("array") {
                    assert!(
                        map.contains_key("items"),
                        "array schema missing items at {}",
                        path
                    );
                }
                for (k, v) in map {
                    let child_path = format!("{}.{}", path, k);
                    assert_arrays_have_items(v, &child_path);
                }
            }
            Value::Array(arr) => {
                for (idx, v) in arr.iter().enumerate() {
                    let child_path = format!("{}[{}]", path, idx);
                    assert_arrays_have_items(v, &child_path);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn all_tool_array_schemas_define_items() {
        for tool in tool_catalog() {
            assert_arrays_have_items(&tool.input_schema, tool.name);
        }
    }
}
