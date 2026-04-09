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
- Default path is non-proxy. Use proxies only after confirmed blocking/rate-limit symptoms or when you know your IP reputation is poor.
- Supports token-efficient extraction (`clean_json` in single mode).
- Supports proxy retry (`use_proxy=true`).
- Supports relevance filtering and JS rendering fallback.
- Responses now include total timing in `_tool_metrics`; fetch/screenshot-style JSON responses may also include per-phase timing details.

### `extract_fields`
Primary structured extraction tool.
- Use for schema/field extraction (title, price, author, etc.).
- Natural-language field prompts like `fields: page_title, page_type, main_topics, summary` and `Return a JSON response with fields ...` are supported for strict extraction contracts.
- Do not use for raw `.md/.json/.txt` files; use `web_fetch(output_format="clean_json")`.

### `hitl_web_fetch`
Unified HITL escalation tool via `auth_mode`:
- `auth_mode="challenge"`: CAPTCHA/Cloudflare/anti-bot bypass.
- `auth_mode="auth"`: login-focused flow with session persistence.

### `scout_browser_automate`
Stateful headless browser automation for workflows and smoke tests.
- Use with `scout_browser_close` cleanup.
- Prefer this over Playwright-style multi-tool sequences when the task is one browser workflow.
- Core action families now include:
- navigation/input: `navigate`, `navigate_back`, `hover`, `click`, `type`, `press_key`, `scroll`, `wait_for`, `wait_for_selector`, `wait_for_locator`
- locator/assert helpers: `click_locator`, `type_locator`, `assert`, `assert_locator`, `generate_locator`, `verify_element_visible`, `verify_text_visible`, `verify_list_visible`, `verify_value`
- browser/session control: `tabs`, `resize`, `handle_dialog`, `file_upload`, `fill_form`, `pdf_save`, `screenshot`, `snapshot`
- diagnostics/state: `trace_start`/`trace_stop`/`trace_export`, `console_tap`/`console_dump`, `network_tap`/`network_dump`, `mock_api`/`route_list`/`unroute` (including header overrides/stripping), `storage_state_*`, `storage_checkpoint`/`storage_rollback`, `cookie_*`, `localstorage_*`, `sessionstorage_*`
- Use output file parameters like `filename` when you need artifacts persisted to disk.
- Use `tabs` instead of opening separate browser sessions for multi-page flows.
- For first-time auth on a domain, stop and use `scout_agent_profile_auth`, then resume automation.

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
- Performance inspection: check `_tool_metrics.total_duration_ms`; for `web_fetch`/`visual_scout`, inspect `metrics.phases` to see where time was spent.

---

## Common Mistakes

- Calling `web_search` then `web_fetch` separately when you need both.
- Skipping `memory_search` before live requests.
- Using `extract_fields` on raw markdown/json URLs.
- Using repo smoke scripts to validate MCP runtime behavior after a rebuild. For release validation, prefer direct MCP tool calls against realistic public URLs so the observed result matches what agents actually receive.
- Escalating to HITL before trying `web_fetch` + `visual_scout`.
- Leaving browser automation sessions open (call `scout_browser_close`).

---

## Browser Automation Scope

- Use `scout_browser_automate` for exploratory workflows, smoke validation, environment checks, and live debugging.
- Keep Playwright only for full regression suites that truly need its own test runner, fixture model, parallel worker orchestration, or richer CI reporting/video traces.
