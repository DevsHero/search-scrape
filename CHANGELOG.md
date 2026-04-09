# Changelog

Policy:
- Keep changes under **Unreleased** during normal development.
- `bash scripts/release.sh` automatically promotes `## Unreleased` → `## vX.Y.Z (YYYY-MM-DD)` and commits the changelog before tagging.

## v3.3.4 (2026-04-09)

### Added
- Added cross-process host guard coordination for search engines and scrape hosts so multiple Cortex Scout processes on the same machine/IP space requests out instead of self-triggering rate limits.
- Added shared cross-process search cache + singleflight locking so concurrent repos/agents can reuse live search results instead of duplicating the same upstream traffic.
- Added regression tests for `deep_research` history URL extraction and timeout helper behavior.

### Changed
- Lowered the default outbound concurrency budget to `16`, staggered search-engine fan-out, and tightened when community expansion runs so normal multi-agent usage is less bursty by default.
- `deep_research` now treats history as a first-class bootstrap source, skips empty history shells, and returns partial results when a hop scrape exceeds the configured timeout instead of hanging until the MCP caller times out.
- Cleaned up post-release compiler hygiene by removing a cross-target unused import warning in the setup permission checks.

### Fixed
- Fixed stdio cold-start memory races so `memory_search`, search duplicate detection, and `deep_research` can wait briefly for LanceDB instead of missing reusable history on first request.
- Fixed `deep_research` history reuse so previously logged search previews and deep-research result objects expose reusable URLs for later runs.
- Fixed `deep_research` tool-level timeout failures under hostile scrape targets by enforcing a hop timeout and preserving partial output.

### Verified
- Re-ran production cleanup passes over the Rust sources to confirm there are no live `todo!`, `unimplemented!`, or compiler-reported dead-code/unused warnings on the default target.

## v3.3.3 (2026-04-08)

### Added
- Added `publish/ci/smoke_deep_research.py` to sweep `deep_research` over MCP with coverage for every public parameter, clamp behavior, and invalid input handling.
- Added `effective_config` to `deep_research` responses so MCP clients and smoke tests can inspect the clamped execution parameters that were actually used.

### Fixed
- Fixed browser launches in root/restricted environments by applying explicit no-sandbox handling across automation sessions, visible auth sessions, and the raw non-robot browser spawns.
- Fixed `deep_research.max_chars_per_source=0` so the handler clamps it to a safe minimum of `1` instead of silently allowing an empty-content configuration.
- Fixed `quality_mode` contract drift by exposing `high` in MCP schemas and validation messages anywhere the runtime parser already supported it.

## v3.3.2 (2026-03-30)

### Added
- Expanded `scout_browser_automate` with broader Playwright-style parity: `navigate_back`, `hover`, `wait_for`, `resize`, `tabs`, `file_upload`, `fill_form`, `handle_dialog`, `pdf_save`, coordinate mouse actions, route inspection/removal, network state toggling, cookie/localStorage/sessionStorage CRUD, and verification helpers (`generate_locator`, `verify_*`).
- Added richer `mock_api` controls with persistent route registry, method matching, custom response headers, delay simulation, one-shot interception mode, and route listing/removal.

### Changed
- Updated automation tool schema metadata so the new actions and parameters are discoverable by MCP clients and agents.
- Refreshed README, VS Code setup docs, and agent instructions to teach the latest omni-tool browser workflow.
- Tightened browser automation smoke coverage so release checks validate concrete outputs, mocked-header stripping, route teardown, and generated artifacts instead of only checking for step execution.

### Fixed
- Hardened MCP tool-name routing compatibility so both public `scout_*` names and internal `browser_*` names resolve correctly for call dispatch.
- Fixed the automation handler/session plumbing so tab switching and the expanded browser action surface execute through the same persistent session safely.
- Fixed `mock_api.remove_headers` so stripped response headers are actually removed from mocked fetch/XHR replies, eliminating a browser-automation false positive in route mocking tests.

## v3.3.0 (2026-03-30)

### Added
- Unified the public MCP tool surface around grouped calls: `web_search(include_content=true)`, `web_fetch(mode="single"|"batch"|"crawl")`, and `hitl_web_fetch(auth_mode="challenge"|"auth")`.
- Expanded browser automation to behave more like a compact Playwright replacement, including nested flows, console capture, storage state helpers, and stronger auto-wait assertions.

### Changed
- Refreshed usage docs, setup guides, and smoke coverage so the repository points agents at the current grouped tools instead of legacy tool names.
- Updated release/version metadata in lockstep for the next minor release.

### Fixed
- Hardened schema validation and tool descriptions to remove stale or hallucination-prone references from the current tool catalog.
- Cleaned up several small clippy warnings and codepaths discovered during the release-prep scan.

## v3.2.0 (2026-03-17)

### Added
## 🎭 The "Playwright Killer" (Stateful Browser Automation)

CortexScout includes a built-in, stateful CDP automation engine designed specifically for AI Agents, completely replacing heavy frameworks like Playwright or Cypress for E2E testing workflows.
- **The Silent Omni-Tool (`scout_browser_automate`)**: Instead of calling dozens of tools, agents pass an array of `steps` (navigate, click, type, scroll, press_key, snapshot, screenshot). The entire sequence executes in a single LLM turn, saving massive amounts of context tokens.
- **Persistent Agent Profile**: Automation runs silently in the background (`--headless=new`) using a dedicated isolated profile (`~/.cortex-scout/agent_profile`). It maintains cookies, localStorage, and session state across tool calls without causing `SingletonLock` collisions with your active desktop browser.
- **QA Mock & Assert Engine**: Built for enterprise E2E testing. Agents can inject XHR/Fetch network interceptors (`mock_api`) and run fail-fast DOM assertions (`assert`) that immediately halt the sequence if a UI state is incorrect.
- **The Agent Auth Portal (`scout_agent_profile_auth`)**: If the silent agent encounters a CAPTCHA or complex OAuth login (like Google/Microsoft) on a new domain, this tool launches the agent's profile in a **visible** window. You solve the CAPTCHA once, the cookies are saved, and the agent returns to silent automation forever.

### Changed
- **Scoped extraction confidence calculation.** Confidence is no longer a fixed 0.8 baseline with per-null penalties; it is now a weighted score and still overrides to 0.0 for placeholder/JS-only pages.

### Fixed
- **Hallucination-proof extraction scoring.** `extract_structured`/`fetch_then_extract` now computes confidence using a 3-factor score (non-null ratio, source grounding, type validation), with strict type checking and fuzzy grounding to ensure extracted strings actually appear in the source page.
- **New grounding and type warnings.** Extraction now emits `grounding_fail` and `type_mismatch` warnings when values cannot be verified in the source or do not match the schema.
- **`extract_fields` / `fetch_then_extract` hallucination risk.** Non-null but incorrect extracted values no longer automatically imply high confidence.


## v3.1.3 (2026-03-14)

### Fixed

- **Auth-wall false positives on public pages with login modals.**
  Pages like Discourse forum threads were incorrectly blocked with `NEED_HITL`
  because the password input in the header login modal triggered auth detection.
  Rewrote `detect_auth_wall_html` with a high/low-confidence selector split:
  - **High-confidence selectors** (e.g. `#login_field`, `.auth-form`, `#loginForm`)
    fire unconditionally.
  - **Low-confidence selectors** (e.g. `[type='password']`, generic `/login` form
    actions) now require corroboration from the page title or URL to fire.
  Added a word-count gate in `mod.rs`, `cdp.rs`, and `scrape_url.rs`: if a page
  has more than 100 words, any auth signal is downgraded from blocking to an
  advisory prepended to the content. Crawl aborts are now gated on
  `word_count < 50` (was: any auth signal).

- **rodio 0.22 build failure in `non_robot_search.rs`.**
  `OutputStreamBuilder` and `Sink` were removed in rodio 0.22.2. Updated to
  `DeviceSinkBuilder::open_default_sink()` and `Player::connect_new()`.

- **rmcp 1.2 `#[non_exhaustive]` build failure in `stdio.rs`.**
  `Tool`, `ServerInfo`, and `Implementation` are now `#[non_exhaustive]` in
  rmcp 1.2.0, preventing struct-literal construction outside the crate. Replaced
  struct literals with `Default::default()` + field mutation or the provided
  builder-chain methods.

## v3.1.2 (2026-03-05)

### Fixed

- **CDP concurrent launches — `SingletonLock` race condition (closes #7).**  
  When multiple MCP tools triggered headless browser fetches simultaneously, all
  launched into the same default Chrome user-data dir, causing every instance
  after the first to fail with `"SingletonLock"`. Each CDP launch now gets an
  isolated `--user-data-dir` under a unique `/tmp/cortex-scout-cdp-XXXXXXXX`
  directory (cleaned up automatically after each request). Concurrent headless
  scraping now works correctly under load.

- **`release.sh` — Version Guard used relative path for `server.json`.**  
  `python3 -c "... pathlib.Path('server.json') ..."` resolved relative to the
  shell's working directory, not the repo root, causing the guard to fail when
  the script was invoked from any directory other than the repo root. Fixed to
  use the absolute `$REPO_ROOT/server.json` path.

- **`release.sh` — empty `## Unreleased` section causes silent commit failure.**  
  When `## Unreleased` was absent or empty, `promote_changelog` left the file
  unchanged but the script still ran `git commit`, which exited 1 ("nothing to
  commit"), aborting the release via `set -e`. Script now checks for a
  `## Unreleased` section and exits early with a clear error if absent.

### Changed

- **Documentation overhaul — all configuration guides updated for v3.1.x.**  
  All example configs previously used `RUST_LOG=info` (floods MCP stdio clients)
  and were missing the required `--` separator before the binary path on
  macOS/Linux. All path examples referenced the old `search-scrape` repo name.
  Updated across README, IDE_SETUP.md, VSCODE_SETUP.md, window_setup.md, and
  ubuntu_setup.md:
  - `RUST_LOG=warn` in every example (was `info`)
  - `"--"` added before binary path in `env`-command args arrays
  - All paths use `cortex-scout` (was `search-scrape`)
  - Build command updated to `--all-features` (was `--features non_robot_search`)
  - New VS Code section with macOS/Linux vs Windows config split
  - Deep research env vars table added (`DEEP_RESEARCH_SYNTHESIS_*`)
  - 8 previously undocumented environment variables added to README table
  - `window_setup.md`: removed `[DEPRECATED]` tag, removed Browserless/ONNX
    references from v2 architecture, rewritten for current binary-only setup
  - `docs/SAFETY_KILL_SWITCH_SUMMARY.md`: deleted (stale internal dev note,
    superseded by `docs/SAFETY_KILL_SWITCH.md`)

## v3.1.1 (2026-02-27)

### Fixed

- **Release script (`scripts/release.sh`) — BSD `sed` crash on macOS.**  
  The trailing-blank-line trimmer used GNU `sed` syntax (`-e :a -e '/^\n*$/{$d;N;ba}'`) that does not work on macOS BSD `sed`, causing the release to abort at the "Release Notes" step with `sed: 2: ... extra characters at the end of d command`. Replaced with `python3 -c "import sys; print(sys.stdin.read().strip())"` (python3 is already a required tool) — portable on all platforms.

- **HTTP MCP server — `inputSchema` field serialized as `input_schema` (snake_case).**  
  The `McpTool` struct lacked `#[serde(rename = "inputSchema")]`. Every MCP client that validates tool schemas (MetaMCP, LiteLLM, MCP Inspector) received `inputSchema: undefined` for all tools and refused to list or call them. Added the rename attribute — tools now pass Zod/schema validation in all tested clients.

- **HTTP MCP server (`POST /mcp`) — `tools/call` was missing from JSON-RPC dispatcher.**  
  The `mcp_rpc_handler` only handled `initialize` and `tools/list`; any `tools/call` request fell through to `-32601 Method not found`. LiteLLM and other Streamable HTTP MCP clients could connect but never successfully invoke tools.  Implemented full `tools/call` dispatch by extracting a shared `call_tool_inner()` function in `http.rs` reused by both the Axum route and the JSON-RPC handler.

- **HTTP MCP server — wrong `protocolVersion` in `initialize` response.**  
  Was returning `"2025-11-05"` (nonexistent). Corrected to `"2024-11-05"` (current MCP spec). Strict clients validate this field and would refuse to proceed.

- **HTTP MCP server — notifications returned JSON-RPC error instead of HTTP 202.**  
  JSON-RPC notifications (no `id` field, e.g. `notifications/initialized`) must not receive a response body. Handler now returns `HTTP 202 Accepted` with an empty body for all notification messages.

- **`McpCallResponse.is_error` field name mismatch.**  
  Was serialized as `is_error` (snake_case). MCP spec requires `isError` (camelCase). Fixed with `#[serde(rename = "isError")]`.

- **VS Code / Cursor MCP panel — `EOF` error on connect.**  
  `McpService::new()` was blocking the tokio thread for 2–4 s while LanceDB rebuilt its IVF/KMeans index. VS Code's stdio timeout fired before `serve()` was called, producing `Error: failed to get tools: calling "tools/list": EOF`.  Fix: LanceDB initialisation moved to a `tokio::spawn` background task; `Arc<AppState>` is created before the spawn so `serve()` starts immediately (~0.08 s). `AppState.memory` changed from `Option<Arc<...>>` to `Arc<RwLock<Option<Arc<...>>>>` for interior-mutability background init. Added `get_memory()` helper that acquires+drops the read-guard in one step, returning a `Send`-safe `Option<Arc<MemoryManager>>` that is safe to use across `await` points in spawned futures.

### Changed

- **Project renamed: ShadowCrawl → cortex-scout** (full codebase sweep):
  - Binary names: `shadowcrawl` / `shadowcrawl-mcp` → `cortex-scout` / `cortex-scout-mcp`
  - Crate name: `shadowcrawl` → `cortex_scout`
  - Config file: `~/.shadowcrawl/shadowcrawl.json` → `~/.cortex-scout/cortex-scout.json`
  - Session cookies: `~/.shadowcrawl/sessions/` → `~/.cortex-scout/sessions/`
  - Environment-variable prefix: `SHADOWCRAWL_*` → `CORTEX_SCOUT_*`
  - GitHub repository: `DevsHero/ShadowCrawl` → `cortex-works/cortex-scout`

- **`deep_research` result — adds `synthesis_model` and `synthesis_endpoint` fields.**  
  Agents can now verify which LLM backend + endpoint was used for synthesis (audit trail). `synthesis_method` distinguishes `"openai_chat_completions"` from `"heuristic_v1"` (extractive fallback). Documented in agent rules (`.github/copilot-instructions.md`).

- **`deep_research` config — adds `synthesis_max_tokens` tunable.**  
  Controls the maximum token budget for the LLM synthesis call via `cortex-scout.json` or `SYNTHESIS_MAX_TOKENS` env var; default `1024`.

- **`mcp.json` (VS Code MCP config) — removed dead `CORTEX_SCOUT_VSCODE_RELOAD=1` env var** (was never referenced in Rust source). Changed `RUST_LOG=info` → `RUST_LOG=warn` to suppress LanceDB/IVF rebuild log lines that pollute stderr during MCP stdio communication.

- **Release script (`scripts/release.sh`)** now automatically promotes `## Unreleased` to the versioned section in `CHANGELOG.md`, commits it, and uses that section verbatim as GitHub release notes. `--dry-run` skips all git mutations.

## v3.1.0 (2026-02-24)
### Added

- **`cortex-scout.json` file-based config loader** — `ShadowConfig` struct loaded at startup from `cortex-scout.json` (cwd → `../cortex-scout.json` → `CORTEX_SCOUT_CONFIG` env path). All fields optional with env-var + hardcoded fallbacks; missing file is silently ignored.
- **`ShadowDeepResearchConfig` sub-struct** with typed resolver methods providing 3-tier priority: JSON value → env var → hardcoded default for all 6 deep-research tunables (`llm_base_url`, `llm_api_key`, `llm_model`, `synthesis_max_sources`, `synthesis_max_chars_per_source`, `synthesis_enabled`).
- **Local LLM support via `cortex-scout.json`** — configure Ollama, LM Studio, or any OpenAI-compatible endpoint without env vars. Example: `{"deep_research": {"llm_base_url": "http://localhost:1234/v1", "llm_model": "lfm2-2.6b", "llm_api_key": ""}}`.
- **`deep_research` tool** — multi-hop research pipeline: — multi-hop research pipeline:
  - `QueryRewriter` expands the query into focused sub-queries before searching.
  - Multi-engine search → `Reranker` (BM25-style) selects top candidate URLs.
  - Concurrent `scrape_batch` ingests selected sources (proxy-aware, configurable concurrency).
  - Model2Vec semantic shave filters each page to only query-relevant chunks (requires LanceDB/memory; gracefully degrades when unavailable).
  - Optional deeper hops (`depth 2-3`): links extracted from first-hop pages are scraped in subsequent rounds, capped to prevent runaway fetching.
  - Results logged to `research_history` for `research_history` recall across sessions.
  - Parameters: `query` (required), `depth` (1-3, default 1), `max_sources` (default 5), `max_chars_per_source` (default 8000), `max_concurrent` (default 3), `use_proxy`, `relevance_threshold`, `quality_mode`.
  - Returns `DeepResearchResult`: `key_findings` (semantically filtered, sorted by content density), `all_urls`, `sub_queries`, `warnings`, `total_duration_ms`.


## v3.0.2 (2026-02-24)
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

- **Media-Aware Extraction (`clean_json` mode)** — `web_fetch` auto-detects raw file URLs (`.md`, `.mdx`, `.rst`, `.txt`, `.csv`, `.toml`, `.yaml`, `.yml`) and skips the HTML extraction pipeline entirely. Content is returned as-is to eliminate duplicate frontmatter. Response includes a `raw_markdown_url` warning.
- **`raw_markdown_url` auto-warn** — `extract_structured` and `fetch_then_extract` automatically inject `raw_markdown_url` into `warnings[]` when called on raw text/markdown files, alerting agents that schema fields will likely return `null`.
- **Agent-tunable dynamic parameters** — previously hardcoded values are now per-call overridable:
  - `short_content_threshold` (default `50`) — word-count floor for `short_content` warning
  - `extraction_score_threshold` (default `0.4`) — quality floor for `low_extraction_score` warning
  - `max_headings` (default `10`) — heading count in `text` mode output
  - `max_images` (default `3`) — image markdown hints in `text` mode output
- **Copilot/agent instructions hardened** (`.github/copilot-instructions.md`):
  - **Rule 1** extended: `memory_search`-first applies to `web_fetch` too, not just `web_search`
  - **Rule 1a** (new): _Dynamic Parameters_ table documents all new tunable params
  - **Rule 4a** (new): _Auto-Escalation on Low Confidence_ — agents must retry with `quality_mode: aggressive` → `visual_scout` → `human_auth_session` autonomously when `confidence < 0.3` or `extraction_score < 0.4`
  - **Decision flow diagram** updated with confidence-check branches, raw markdown path, and corrected tool names throughout

### Changed

- `scrape_url` tool schema: `max_chars` description updated to clarify it caps the full serialized payload, not just the text field.
## v3.1.3 (2026-03-08)

### Added

- **Workspace and global MCP config sync** — `.vscode/mcp.json` and global `mcp.json` now match production best practices, including `HTTP_CONNECT_TIMEOUT_SECS=10` and direct/no-proxy default.
- **Local validation and smoke test scripts** — Added `publish/ci/validate.py` (docs/config sanity) and `publish/ci/smoke_mcp.py` (end-to-end MCP stdio tool coverage) to the repo. README documents their use.
- **Regression test for proxy retry gating** — Ensures proxy fallback only occurs when `use_proxy=true`.

### Changed

- **Direct/no-proxy is now the default** — All fetch/search/deep_research tools run direct by default; proxy is opt-in and only used after block/rate-limit or explicit request. `ip.txt` is empty by default for opt-in proxy population.
- **Runtime proxy fallback logic hardened** — `scrape.rs` and related code now strictly require `use_proxy=true` for proxy retry; no more hidden fallback.
- **Reduced log noise** — Downgraded non-fatal CDP/browserless logs to info/debug, suppressed `html5ever`, `lance_index::vector::kmeans`, and `lance::dataset::scanner` warnings in tracing filter.
- **Documentation overhaul** — README, IDE_SETUP.md, VSCODE_SETUP.md, and related docs updated: removed obsolete proxy defaults, clarified direct-first, removed `--` from env args, and added explicit build/smoke/validation instructions.
- **Removed obsolete/unused test scripts and workflows** — All legacy smoke/test scripts and unused GitHub Actions workflows deleted; only `publish/ci/smoke_mcp.py` and `publish/ci/validate.py` remain.
- **Cleaned up deprecated/obsolete info in docs** — Removed references to old binaries, profile lock warnings, and deprecated proxy registry notes.

### Fixed

- **GitHub repo-root rewrite bug** — `rewrite_url_for_clean_content()` now correctly rewrites `github.com/{owner}/{repo}` to `raw.githubusercontent.com/{owner}/{repo}/HEAD/README.md` (test and runtime match).
- **Dead code warning** — Removed unused fields from `CookieHealth` struct in `non_robot_search.rs`.
- **All tests pass cleanly** — `cargo test --all-features` is green after all changes.

- `extract_structured` tool description: added `⚠️ AUTO-WARN` note about `raw_markdown_url` injection.
- Copilot instructions: Rule 7 renamed from `hitl_web_fetch` to `non_robot_search` (correct MCP tool name); session persistence note added.

## v3.0.0 (2026-02-20)

### Added

- **`human_auth_session` (The Nuclear Option)**: Launches a visible browser for human login/CAPTCHA solving. Captures and persists full authentication cookies to `~/.cortex-scout/sessions/{domain}.json`. Enables full automation for protected URLs after a single manual session.
- **Instruction Overlay**: `human_auth_session` now displays a custom green "Cortex Scout" instruction banner on top of the browser window to guide users through complex auth walls.
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
- Updates tool titles/descriptions to explicitly steer agents to Cortex Scout tools (token-efficient) over IDE fetch.


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

- Kill switch: `CORTEX_SCOUT_NEUROSIPHON=0` disables all NeuroSiphon behaviors.
