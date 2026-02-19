# Changelog

Policy:
- Keep changes under **Unreleased** during normal development.
- Only bump version + move Unreleased entries into a new version section when you run `bash scripts/release.sh`.

## Unreleased

### Added

- —

### Changed

- —

### Fixed

- —

## v2.5.0 (2026-02-19)

### Added

- **Markdown post-processor**: `normalize_markdown(text: String) -> String` unescapes token-wasting Markdown escapes, collapses excess blank lines, and dedupes navigation link spam.
- **GitHub blob URL auto-rewrite**: `web_fetch` on `github.com/*/blob/*` URLs is transparently rewritten to `raw.githubusercontent.com` before fetching — returns the raw file/source directly instead of GitHub's React SPA shell.
- **GitHub SPA payload extraction**: `looks_like_spa` now detects GitHub's `react-app.embeddedData` script tag. `extract_spa_json_state` extracts `payload.blob.text`, `payload.readme`, `payload.issue.body`, `payload.pullRequest.body`, `payload.discussion.body` from the embedded JSON.
- **Smart Auth-Wall Guard Dog**: HTML DOM selector heuristics + clean-text keyword heuristics set `auth_wall_reason` and prevent returning login pages as real content.
- **Auth-wall structured outcome**: `web_fetch` / `web_crawl` return `{"status":"NEED_HITL","suggested_action":"non_robot_search"}` when auth-walled.
- **GitHub pivot retry**: on auth-walls, attempts a one-time GitHub `?plain=1` pivot (when applicable) before recommending HITL.

### Changed

- **Sniper mode (`clean_json`)**: now includes `key_points` (first-sentence bullets) and `extraction_score` in metadata.
- **Cache safety**: auth-walled scrape results are not cached (avoids “poisoned” cache after manual login).

### Fixed

- **Crawl correctness**: auth-walled pages are treated as failures; auth-walled start URL aborts early with NEED_HITL.

## v2.4.3 (2026-02-19)

### Chore (build hygiene)
- `web_fetch`: `extract_relevant_sections=true` returns only the most relevant sections for `query` (short output; avoids huge tool responses).

- Fixes cross-target build warnings caused by platform-specific `cfg` blocks:
	- removes `unused_imports` for `Path`/`PathBuf` on non-macOS targets
	- avoids `dead_code` warnings for setup-only helpers on Windows builds


## v2.4.2 (2026-02-19)

### MCP tool naming normalization (agent clarity)

- Standardizes public MCP tool names to consistent verbs:
	- `web_search`, `web_search_json`, `web_fetch`, `web_fetch_batch`, `web_crawl`, `extract_fields`, `memory_search`, `proxy_control`, `hitl_web_fetch`
- Adds intuitive aliases to prevent agent confusion and keep old prompts working:
	- `fetch_url`, `fetch_webpage`, `webpage_fetch` → `web_fetch`
	- `fetch_url_batch` → `web_fetch_batch`
	- `site_crawl` → `web_crawl`
	- `structured_extract` → `extract_fields`
	- `human_web_fetch` → `hitl_web_fetch`

### Notes

- Internal tool routing remains stable; legacy internal names still work (`scrape_url`, `non_robot_search`, etc.).

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
