# Changelog

## v2.4.1 (2026-02-19)

### Agent-first tool naming (MCP)

- Renames the primary page fetch tool for agents from `scrape_url` (internal) to `web_fetch` (public).
- Adds tool-name aliases: `web_fetch`, `fetch_url`, `fetch_webpage` → `scrape_url` (internal).
- Updates tool titles/descriptions to explicitly steer agents to ShadowCrawl tools (token-efficient) over IDE fetch.


## v2.4.0 (2026-02-19)

### NeuroSiphon token-efficiency integration

- Adds a NeuroSiphon-inspired “Smart Router” pipeline for token-efficient scraping/search.
- Stops raw HTML token leaks in aggressive/NeuroSiphon modes.
- Protects documentation/tutorial pages from import stripping.
- Enforces strict SPA hydration JSON output when `extract_app_state=true`.

### Scraping improvements

- Rule B: infers language from URL extension so raw source URLs (e.g. `*.rs`) get correct code handling.
- Import nuking now has stronger guards (including a minimum-length gate) to avoid harming short snippets.

### Production hardening

- Defaults `RUST_LOG` fallback to `info,tower_http=warn` to reduce per-request logging overhead unless explicitly enabled.
- Gates dev helper binaries behind a `dev-tools` Cargo feature.
- Removes integration/manual test scripts under `mcp-server/tests/`.

### Notes

- Kill switch: `SHADOWCRAWL_NEUROSIPHON=0` disables all NeuroSiphon behaviors.
