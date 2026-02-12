# Release Notes v1.0.0

Date: 2026-02-12
Status: GA (`v1.0.0`)

## Highlights

- Standardized MCP platform surface across HTTP and stdio.
- Confirmed 8-tool runtime catalog:
  - `search_web`
  - `search_structured`
  - `scrape_url`
  - `scrape_batch`
  - `crawl_website`
  - `extract_structured`
  - `research_history`
  - `proxy_manager`
- Hardened stdio lifecycle handling to prevent premature server cancellation.
- Eliminated tool-catalog drift by introducing shared MCP tooling catalog.
- Improved proxy output normalization in status/switch messages.

## Major improvements since v0.3.x

### Platform & Reliability
- Improved MCP stdio runtime behavior and shutdown handling.
- Removed stdout contamination risk in runtime paths for MCP transport safety.
- Added repeatable release validation workflow and artifacts.

### Architecture & Maintainability
- Introduced centralized tool catalog module for both transports.
- Reduced duplicated MCP schema definitions.
- Fixed previously mismatched discoverability between list and call surfaces.

### Operations
- Proxy manager workflow supports list/status/switch/test/grab flows.
- Service health and readiness endpoints validated against local deployment.
- Release-readiness evidence generated in JSON for auditability.

## Validation artifacts

- Release readiness JSON: [docs/RELEASE_READINESS_2026-02-12.json](docs/RELEASE_READINESS_2026-02-12.json)
- GA refactor readiness report: [docs/GA_REFACTOR_READINESS_2026-02-12.md](docs/GA_REFACTOR_READINESS_2026-02-12.md)

## Known limitations

- Free/public proxies can be unstable; proxy test may fail for some endpoints.
- Heavily protected websites (advanced anti-bot systems) may still require premium proxy strategy.

## Recommended next milestone

- Split monolithic handler flows into module-per-tool architecture (`mcp_handlers/*`) while preserving API compatibility.
- Add contract test asserting tool catalog parity between HTTP and stdio surfaces.
