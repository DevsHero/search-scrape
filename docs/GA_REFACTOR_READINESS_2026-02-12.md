# GA Refactor Readiness (2026-02-12)

## Objective
Prepare the MCP server codebase for GA by reducing god-code risk, removing tool-surface drift, and enforcing a cleaner architecture boundary for tool definitions.

## What was refactored

### 1) Single Source of Truth for MCP Tool Definitions
- Added shared catalog module: `mcp-server/src/mcp_tooling.rs`
- Centralized tool metadata:
  - tool name
  - title
  - description
  - input JSON schema
- Added helper conversion for stdio schema map generation.

### 2) Removed HTTP vs stdio Tool Drift
- HTTP endpoint `list_tools` now reads from shared catalog.
- stdio `list_tools` now reads from shared catalog.
- Result: both transport surfaces expose the same tools.

### 3) Added Missing Tool Exposure
- `scrape_batch` now appears in HTTP MCP `list_tools` (already callable before).
- This fixes discoverability mismatch for MCP clients.

### 4) Fixed Proxy Display Formatting
- Added shared proxy display formatter to avoid malformed output such as `http://@host:port`.
- Used in HTTP `proxy_manager` status/switch outputs.

## Validation

### Build Validation
- `cargo check` passed after refactor.

### Runtime Validation
- Rebuilt container with updated image.
- `/mcp/tools` returns 8 tools after refactor.
- Full validation suite re-run via `docs/run_release_validation.py`.

### Evidence
- `docs/RELEASE_READINESS_2026-02-12.json`
  - `tool_count: 8`
  - proxy format output fixed (`http://43.134.238.25:443`)

## Architecture Assessment (remaining hotspots)

### High priority god-code candidates
1. `mcp-server/src/rust_scraper.rs` (~2383 LOC)
   - Mixed concerns: fetching, parsing, cleanup, scoring, extraction rules.
   - Recommendation: split into
     - `fetch/` (network + retries + anti-bot)
     - `parse/` (DOM + metadata)
     - `clean/` (boilerplate removal)
     - `quality/` (scoring/warnings)

2. `mcp-server/src/mcp.rs` (~1000+ LOC)
   - HTTP tool dispatch and response rendering are still dense.
   - Recommendation: per-tool handlers under `mcp_handlers/` with shared arg-parser utilities.

3. `mcp-server/src/stdio_service.rs` (~700+ LOC)
   - Tool dispatch still monolithic.
   - Recommendation: mirror HTTP handler modules to reduce divergence risk.

## GA recommendation
- Status: **Ready for guarded GA rollout** after this refactor set.
- Before full-scale GA:
  1. Complete `rust_scraper.rs` modular split.
  2. Unify argument parsing and response formatting across HTTP/stdio dispatch layers.
  3. Add contract tests that assert identical tool catalogs for both transports.

---

## Execution Update (Phase 2 complete)

### Completed now
1. Split HTTP MCP handlers from `mcp.rs` into per-tool modules under `mcp-server/src/mcp_handlers/`:
  - `search_web.rs`
  - `search_structured.rs`
  - `scrape_url.rs`
  - `crawl_website.rs`
  - `scrape_batch.rs`
  - `extract_structured.rs`
  - `research_history.rs`
  - `proxy_manager.rs`
2. Simplified `mcp-server/src/mcp.rs` to transport/router responsibility only (tool dispatch + request/response types).
3. Exported `mcp_handlers` from `mcp-server/src/lib.rs`.

### Validation after split
- `cargo check` passed (`mcp-server`)
- `python3 docs/run_release_validation.py` passed (`cases=14`)

### GA closeout checklist
- [x] Remove `mcp.rs` god-code hotspot by per-tool split
- [x] Keep MCP tool surface unchanged (8 tools)
- [x] Rebuild and compile verification pass
- [x] Post-refactor release validation pass
- [ ] Next phase (non-blocking): split `rust_scraper.rs`
- [ ] Next phase (non-blocking): mirror stdio dispatch to shared per-tool handlers
