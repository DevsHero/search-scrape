# Changelog

Policy:
- Keep changes under **Unreleased** during normal development.
- Only bump version + move Unreleased entries into a new version section when you run `bash scripts/release.sh`.

## Unreleased


## v3.0.2 (2026-02-21)
### Added

- **`skip_live_fetch` machine-readable boolean** in `research_history` response — each result entry now includes:
  - `skip_live_fetch` (`bool`): `true` only when the entry is a Scrape (not Search), similarity ≥ 0.60, `word_count` ≥ 50, and no sparse-content warnings. Agents should consume this field directly rather than re-implementing the cache-quality guard.
  - `word_count` (`u64 | null`): extracted from the stored `ScrapeResponse`; `null` for Search-type entries.
- **GitHub repo root → raw README auto-rewrite** — `scrape_url` now redirects `github.com/{owner}/{repo}` (exactly 2 path segments) to `raw.githubusercontent.com/{owner}/{repo}/HEAD/README.md` before scraping. Avoids `NEED_HITL` on public repos caused by GitHub's React SPA rendering.

### Changed

- `research_history` default similarity threshold raised from `0.5` to `0.60` — aligns server default with the `≥ 0.60` threshold documented in agent rules and prevents coarse low-score cache hits from being trusted as full-quality results.
- Auth-wall `github_raw_url` hint in `scrape_url` extended to also cover repo root URLs (not just `/blob/` paths).
- Agent rules (`.github/copilot-instructions.md`): cache-quality guard updated to document the machine-readable `skip_live_fetch` field — agents must check this field instead of manually evaluating the multi-condition guard.
- Agent rules (`.github/copilot-instructions.md`): add `entry_type == "search"` check to the cache-quality guard — search-index cache entries carry no `word_count` metadata, so a high similarity score on a search entry must never cause agents to skip `scrape_url` on the top result URL.
- Agent rules: cache-quality guard expanded with word_count < 50 guard and placeholder-warnings check (canonical example: `crates.io` JS-render pages); private/internal tools note added (skip `search_structured`, go directly to `scrape_url` on known URL).
- README `🤖 Agent Optimal Setup` section fully refreshed: updated tool names (`research_history`, `search_structured`, `scrape_url`, `proxy_manager`, `non_robot_search`), full 7-rule block, new decision flow diagram with cache-quality guard and confidence-escalation path, removed stale `memory_search`/`hitl_web_fetch`/`extract_fields` references.
## v3.0.1 (2026-02-21)

### Added

- **Dynamic payload cap (`max_chars`)** — `web_fetch` (`scrape_url`) now applies `max_chars` to the **total serialized JSON payload** in both `json` and `clean_json` modes, not just the `clean_content` text field. Prevents 93 KB workspace-storage spills caused by unbounded `links[]`, `images[]`, `code_blocks[]`. A `⚠️ JSON_PAYLOAD_TRUNCATED` / `CLEAN_JSON_PAYLOAD_TRUNCATED` notice is appended when truncation occurs.
- **Media-Aware Extraction (`clean_json` mode)** — `web_fetch` auto-detects raw file URLs (`.md`, `.mdx`, `.rst`, `.txt`, `.csv`, `.toml`, `.yaml`, `.yml`) and skips the HTML extraction pipeline entirely. Content is returned as-is to eliminate duplicate frontmatter. Response includes a `raw_markdown_url` warning.
- **`raw_markdown_url` auto-warn** — `extract_structured` and `fetch_then_extract` automatically inject `raw_markdown_url` into `warnings[]` when called on raw text/markdown files, alerting agents that schema fields will likely return `null`.
- **Agent-tunable dynamic parameters** — previously hardcoded values are now per-call overridable:
  - `short_content_threshold` (default `50`) — word-count floor for `short_content` warning
  - `extraction_score_threshold` (default `0.4`) — quality floor for `low_extraction_score` warning
  - `max_headings` (default `10`) — heading count in `text` mode output
  - `max_images` (default `3`) — image markdown hints in `text` mode output
  - `snippet_chars` (default `120` NeuroSiphon / `200` standard) — search result snippet length
- **Copilot/agent instructions hardened** (`.github/copilot-instructions.md`):
  - **Rule 1** extended: `memory_search`-first applies to `web_fetch` too, not just `web_search`
  - **Rule 1a** (new): _Dynamic Parameters_ table documents all new tunable params
  - **Rule 4a** (new): _Auto-Escalation on Low Confidence_ — agents must retry with `quality_mode: aggressive` → `visual_scout` → `human_auth_session` autonomously when `confidence < 0.3` or `extraction_score < 0.4`
  - **Decision flow diagram** updated with confidence-check branches, raw markdown path, and corrected tool names throughout
  - **Tool Quick-Reference** updated to use actual MCP tool names (`research_history`, `search_web`, `search_structured`, `scrape_url`, `crawl_website`, `extract_structured`, `proxy_manager`, `non_robot_search`) and includes `visual_scout` and `human_auth_session`

### Changed

- `scrape_url` tool schema: `max_chars` description updated to clarify it caps the full serialized payload, not just the text field.
- `extract_structured` tool description: added `⚠️ AUTO-WARN` note about `raw_markdown_url` injection.
- Copilot instructions: Rule 7 renamed from `hitl_web_fetch` to `non_robot_search` (correct MCP tool name); session persistence note added.

## v3.0.0 (2026-02-20)

### Added

- **`human_auth_session` (The Nuclear Option)**: Launches a visible browser for human login/CAPTCHA solving. Captures and persists full authentication cookies to `~/.shadowcrawl/sessions/{domain}.json`. Enables full automation for protected URLs after a single manual session.
- **Instruction Overlay**: `human_auth_session` now displays a custom green "ShadowCrawl" instruction banner on top of the browser window to guide users through complex auth walls.
- **Persistent Session Auto-Injection**: `web_fetch`, `web_crawl`, and `visual_scout` now automatically check for and inject matching cookies from the local session store.
- **`extract_structured` / `fetch_then_extract`**: new optional params `placeholder_word_threshold` (int, default 10) and `placeholder_empty_ratio` (float 0–1, default 0.9) allow agents to tune placeholder detection sensitivity per-call.
- **`web_crawl`**: new optional `max_chars` param (default 10 000) caps total JSON output size to prevent workspace storage spill.
- **Rustdoc module extraction**: `extract_structured` / `fetch_then_extract` correctly populate `modules: [...]` on docs.rs pages using the `NAME/index.html` sub-directory convention.
- **GitHub Discussions & Issues hydration**: `fetch_via_cdp` detects `github.com/*/discussions/*` and `/issues/*` URLs; extends network-idle window to 2.5 s / 12 s max and polls for `.timeline-comment`, `.js-discussion`, `.comment-body` DOM nodes.
- **Contextual code blocks** (`clean_json` mode): `SniperCodeBlock` gains a `context: Option<String>` field. Performs two-pass extraction for prose preceding fenced blocks and Markdown sentences containing inline snippets.
- **IDE copilot-instructions guide** (README): new `🤖 Agent Optimal Setup` section.
- **`.clinerules`** workspace file: all 7 priority rules + decision-flow diagram + per-tool quick-reference table.
- **Agent priority rules in tool schemas**: every MCP tool description now carries machine-readable `⚠️ AGENT RULE` / `✅ BEST PRACTICE`.

### Changed

- **Placeholder detection (Scalar-Only Logic)**: Confidence override to 0.0 now only considers **scalar (non-array)** fields. Pure-array schemas (headers, modules, structs) never trigger fake placeholder warnings, fixing false-positives on rich but list-heavy documentation pages.
- `web_fetch(output_format="clean_json")`: applies a `max_chars`-based paragraph budget and emits `clean_json_truncated` when output is clipped.
- `extract_fields` / `fetch_then_extract`: placeholder/unrendered pages (very low content + mostly empty schema fields) force `confidence=0.0`.
- **Short-content bypass** (`strict_relevance` / `extract_relevant_sections`): early exit with a descriptive warning when `word_count < 200`. Short pages (GitHub Discussions, Q&A threads) are returned whole.

### Fixed

- **BUG-6**: `modules: []` always empty on rustdoc pages — refactored regex to support both absolute and simple relative module links (`init/index.html`, `optim/index.html`).
- **BUG-7**: false-positive `confidence=0.0` on real docs.rs pages; replaced whole-schema empty ratio with scalar-only ratio + raised threshold.
- **BUG-9**: `web_crawl` could spill 16 KB+ of JSON into VS Code workspace storage; handler now truncates response to `max_chars` (default 10 000).
- `web_fetch(output_format="clean_json")`: paragraph filter now adapts for `word_count < 200`.
- `fetch_then_extract`: prevents false-high confidence on JS-only placeholder pages (e.g. crates.io) by overriding confidence to 0.0.
- **`cdp_fallback_failed` on GitHub Discussions**: extended CDP hydration window and selector polling ensures full thread capture.

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
