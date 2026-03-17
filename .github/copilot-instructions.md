# Cortex Scout — Agent Usage Guide

Cortex Scout is a web research MCP server. Use its tools instead of IDE-provided fetch tools — they are more token-efficient, handle JS rendering, block anti-bot measures, and cache results in semantic memory.

---

## Tool Decision Tree

```
Need info from the web?
 └─► 1. memory_search first (may already be cached)
      └─► Cache hit (score ≥ 0.60)? → Use it, skip live fetch
      └─► No cache? → choose based on goal:

         SEARCH ONLY (URL discovery)        → web_search
         SEARCH + READ CONTENT (research)   → web_search_json  ← PREFERRED
         SINGLE URL (known page)            → web_fetch
         MULTIPLE URLS (known list)         → web_fetch_batch
         SITE STRUCTURE (find sub-pages)    → web_crawl
         STRUCTURED DATA (title/price/etc)  → fetch_then_extract or extract_fields
         DEEP MULTI-HOP RESEARCH            → deep_research

Blocked / rate-limited?
 └─► proxy_control action=grab → retry with use_proxy=true

Auth wall suspected (auth_risk_score ≥ 0.4)?
 └─► visual_scout → confirm → human_auth_session (saves cookies)

CAPTCHA / heavy anti-bot?
 └─► hitl_web_fetch (human solves in real browser)

Browser automation (forms, SPAs, UI testing)?
 └─► scout_browser_automate → scout_browser_close when done
      └─► First-time login needed? → scout_agent_profile_auth first
```

---

## All Tools — Quick Reference

### `memory_search`
Semantic search over past web searches and scrapes stored in LanceDB.
- **Always call this first** before any `web_search` or `web_fetch`
- If any result has similarity ≥ 0.60, use it — skip the live request
- `entry_type: "search"` for past searches; `"scrape"` for past page fetches
- Default threshold: 0.60

### `web_search`
Multi-engine URL discovery (Google + Bing + DuckDuckGo + Brave).
- Returns ranked URLs with short snippets — **no page content**
- Use when you only need to find URLs, not read content
- For search + read, use `web_search_json` instead
- `time_range: "week"` for recent news; `snippet_chars` to control token usage

### `web_search_json` ← PREFERRED for research
Search + fetch top N pages in **one call**. Returns title, URL, and content preview per result.
- More efficient than calling `web_search` then `web_fetch` separately
- `top_n`: how many pages to fetch (default 3)
- `use_proxy: true` only after confirmed 403/429/rate-limit

### `web_fetch`
Fetch and clean a single web page. Prefers token-efficient text; auto-renders JS via CDP.
- **Best practice for docs/articles**: `output_format: "clean_json"` + `strict_relevance: true` + `query: "your topic"` → strips boilerplate, keeps only relevant paragraphs
- `output_format`:
  - `"text"` — readable prose (default)
  - `"json"` — full ScrapeResponse JSON
  - `"clean_json"` — minimum-token JSON: title + key paragraphs + code blocks
- `extract_app_state: true` — force-return Next.js/Nuxt/Remix hydration JSON for SPAs
- Response includes `auth_risk_score` (0.0–1.0): if ≥ 0.4, call `visual_scout` before proceeding
- On 403/429: call `proxy_control action=grab` → retry with `use_proxy: true`

### `web_fetch_batch`
Fetch multiple URLs in parallel. Results arrive in completion order (fastest first), not input order.
- Use when you have a known list of sources to read
- `max_concurrent` to control parallel load (default 5)

### `web_crawl`
BFS-crawl a website to discover its pages and link structure.
- Use when you have a site's root URL and need to find the right sub-pages before fetching
- Do NOT use for single-page fetching — use `web_fetch`
- `max_pages`: limit total pages (default 50); `max_depth`: BFS depth (default 3)
- `include_patterns` / `exclude_patterns`: filter which URL paths to follow
- Returns NEED_HITL if the start URL requires login

### `extract_fields`
Fetch a URL and extract specific named fields into a JSON object.
- Provide `schema`: array of `{name, type, required}` objects
- Returns `confidence` (0.0–1.0): measures whether fields are non-null, not semantic accuracy
- Check `warnings` field for null-field alerts
- Do NOT use on raw `.md`, `.json`, `.txt` files — use `web_fetch output_format=clean_json` instead
- For combined fetch + extract in one call, use `fetch_then_extract`

### `fetch_then_extract`
Fetch + schema extraction in a single call (lower latency than two separate calls).
- `strict: true` — schema shape enforced; missing fields become `null`/`[]`
- Best for well-structured HTML pages; less reliable on heavily JS-rendered pages
- Confidence measures null-field ratio, not semantic correctness

### `deep_research`
Autonomous multi-hop research pipeline.
- Expands query into sub-queries → searches → scrapes → semantically filters → follows links
- Results saved to `memory_search` history automatically
- Use for complex topics needing 5+ sources
- `depth: 1-3` (hops), `max_sources` per hop (default 10)
- LLM synthesis enabled automatically when `OPENAI_API_KEY` is set
- Avoid for simple lookups — use `web_fetch` instead

### `proxy_control`
Manage proxy pool for IP rotation.
- `action: "grab"` — rotate to a fresh proxy (call on first 403/429/rate-limit signal)
- `action: "status"` — check current proxy and pool health
- `action: "list"` — list all available proxies
- After `grab`, retry the blocked tool with `use_proxy: true`

### `visual_scout`
Headless screenshot of a URL saved to a local temp file.
- Use when `web_fetch` returns `auth_risk_score ≥ 0.4` to confirm login/CAPTCHA wall visually
- Response contains `screenshot_path` (local PNG file), `page_title`, and `hint`
- **Does NOT embed base64 image inline** — the PNG is stored on disk at `screenshot_path`
- `hint` field tells you AUTH_WALL (escalate) vs OK (proceed with web_fetch)

### `human_auth_session`
Opens a real browser for the user to log in; then scrapes authenticated content.
- Use ONLY after `visual_scout` confirms `AUTH_REQUIRED` in the hint
- Saves cookies to `~/.cortex-scout/sessions/{domain}.json` — future `web_fetch` calls auto-use them
- Set `instruction_message` to tell the user exactly what to log in to
- Try `web_fetch` first — most sites don't need this

### `hitl_web_fetch`
Opens a real browser for the user to solve CAPTCHA / Cloudflare / heavy anti-bot challenges.
- LAST RESORT — try `web_fetch` first; only use when automation is fully blocked
- Unlike `human_auth_session`, cookies are NOT persisted for future use
- For login-walled pages where persistence matters, prefer `human_auth_session`

### `scout_browser_automate`
Stateful headless browser automation (Brave, persistent agent profile).
- Session persists between calls — `scout_browser_close` to stop
- Steps: `navigate`, `click`, `type`, `press_key`, `scroll`, `evaluate`, `wait_for_selector`, `snapshot`, `screenshot`, `assert`, `mock_api`
- `screenshot` step returns inline base64 PNG (unlike `visual_scout` which saves to disk)
- `snapshot` returns DOM summary: title, URL, headings, buttons, inputs, links, body text
- `assert` halts the step sequence on failure (use for QA checks)
- `mock_api` intercepts network requests — useful for testing without hitting real APIs
- First-time login to a service: run `scout_agent_profile_auth` first

### `scout_browser_close`
Terminate the persistent browser session. Call when done with automation.

### `scout_agent_profile_auth`
Opens a VISIBLE browser for a human to log in to a service using the agent's profile.
- Use ONLY when `scout_browser_automate` is blocked due to no session for a domain
- After login, cookies are saved to the agent profile — future `scout_browser_automate` calls use them silently

---

## Auth-Gatekeeper Protocol

When fetching a page that may require login:

```
1. web_fetch(url)
   └─► auth_risk_score ≥ 0.4?
        ├─► NO  → use content normally
        └─► YES → 2. visual_scout(url)
                    └─► hint says AUTH_WALL?
                         ├─► NO  → use web_fetch content
                         └─► YES → 3. human_auth_session(url, instruction_message=...)
                                      → content returned after login
                                      → cookies saved for future requests
```

---

## Proxy Protocol

When a site blocks you:

```
web_fetch / web_search returns 403 / 429 / "rate limit" / "IP block"
  └─► proxy_control(action="grab")
       └─► retry original call with use_proxy=true
```

---

## Token Efficiency Tips

| Goal | Recommended Call |
|------|-----------------|
| Research a topic | `web_search_json(query, top_n=3)` |
| Read a long doc | `web_fetch(url, output_format="clean_json", strict_relevance=true, query="topic")` |
| Extract structured data | `fetch_then_extract(url, schema=[...])` |
| Check memory first | `memory_search(query)` — skip live fetch if score ≥ 0.60 |
| SPA / JS-heavy page | `web_fetch(url, extract_app_state=true)` |

---

## Common Mistakes to Avoid

- ❌ Calling `web_search` then `web_fetch` separately → use `web_search_json`
- ❌ Skipping `memory_search` before fetching → wastes tokens on cached data
- ❌ Using `extract_fields`/`fetch_then_extract` on `.md`/`.json`/`.txt` files → use `web_fetch clean_json`
- ❌ Trusting `confidence` score alone in extraction results → always check `warnings` and verify values
- ❌ Calling `human_auth_session` without `visual_scout` confirmation → check `auth_risk_score` first
- ❌ Calling `web_fetch` for anti-bot-blocked pages without trying proxy rotation first
- ❌ Leaving `scout_browser_automate` session open after tasks — call `scout_browser_close`
- ❌ Expecting `visual_scout` to return base64 image inline — it saves to disk at `screenshot_path`
- ❌ Expecting `web_fetch_batch` results in input URL order — results arrive in fastest-first order
