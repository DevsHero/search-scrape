# Cortex Scout — Agent Usage Guide

Cortex Scout is a web research MCP server. Prefer Cortex tools over IDE-provided fetch tools.

---

## Tool Decision Tree

```text
Need info from the web?
 └─► 1. memory_search first (may already be cached)
      └─► Cache hit (score ≥ 0.60)? → Use it, skip live fetch
      └─► No cache? → choose based on goal:

         SEARCH ONLY (URL discovery)        → web_search
         SEARCH + READ CONTENT (research)   → web_search(include_content=true)
         SINGLE URL                          → web_fetch(mode="single")
         MULTIPLE URLS                       → web_fetch(mode="batch")
         SITE STRUCTURE                      → web_fetch(mode="crawl")
         STRUCTURED DATA                     → extract_fields
         DEEP MULTI-HOP RESEARCH            → deep_research

Blocked / rate-limited?
 └─► proxy_control(action="grab") → retry with use_proxy=true

Auth wall suspected (auth_risk_score ≥ 0.4)?
 └─► visual_scout → confirm
      ├─► challenge/captcha wall → hitl_web_fetch(auth_mode="challenge")
      └─► login wall             → hitl_web_fetch(auth_mode="auth")
```

---

## Unified Primary Tools

### `web_search`
- URL discovery mode (default).
- Set `include_content=true` to also scrape top results in one call.
- Use `top_n`, `use_proxy`, `quality_mode` when `include_content=true`.

### `web_fetch`
Unified web content tool via `mode`:
- `mode="single"` (default): one URL fetch.
- `mode="batch"`: batch fetch via `urls`.
- `mode="crawl"`: site crawl from a root URL.

Common behavior:
- Supports token-efficient extraction (`clean_json` in single mode).
- Supports proxy retry (`use_proxy=true`).
- Supports relevance filtering and JS rendering fallback.

### `extract_fields`
Primary structured extraction tool.
- Use for schema/field extraction (title, price, author, etc.).
- Do not use for raw `.md/.json/.txt` files; use `web_fetch(output_format="clean_json")`.

### `hitl_web_fetch`
Unified HITL escalation tool via `auth_mode`:
- `auth_mode="challenge"`: CAPTCHA/Cloudflare/anti-bot bypass.
- `auth_mode="auth"`: login-focused flow with session persistence.

### `scout_browser_automate`
Stateful headless browser automation for workflows and smoke tests.
- Use with `scout_browser_close` cleanup.
- Supports assert auto-retries, mock API, console capture, and storage state import/export.

---

## Legacy Aliases (Compatibility)

These remain callable for backward compatibility, but agents should prefer the unified primary tools.

- `web_search_json` → use `web_search(include_content=true)`
- `web_fetch_batch` → use `web_fetch(mode="batch")`
- `web_crawl` → use `web_fetch(mode="crawl")`
- `human_auth_session` → use `hitl_web_fetch(auth_mode="auth")`
- `fetch_then_extract` → use `extract_fields`

---

## Auth-Gatekeeper Protocol

1. Call `web_fetch` first.
2. If `auth_risk_score >= 0.4`, call `visual_scout`.
3. If wall is confirmed:
- Challenge wall: `hitl_web_fetch(auth_mode="challenge")`
- Login wall: `hitl_web_fetch(auth_mode="auth")`

---

## Token Efficiency Tips

- Research topic: `web_search(query, include_content=true, top_n=3)`
- Read long docs: `web_fetch(url, output_format="clean_json", strict_relevance=true, query="...")`
- Structured extraction: `extract_fields(url, schema=[...])`
- Memory-first: `memory_search(query)` before live fetch

---

## Common Mistakes

- Calling `web_search` then `web_fetch` separately when you need both.
- Skipping `memory_search` before live requests.
- Using `extract_fields` on raw markdown/json URLs.
- Escalating to HITL before trying `web_fetch` + `visual_scout`.
- Leaving browser automation sessions open (call `scout_browser_close`).

---

## Browser Automation Scope

- Use `scout_browser_automate` for exploratory workflows, smoke validation, environment checks, and live debugging.
- Keep Playwright for full regression suites that need fixtures, parallel runners, rich traces/videos, and CI-native reporting.
