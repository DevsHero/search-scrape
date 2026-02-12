# Testing Summary (v1.0.0)

## What is covered

This repo validates two layers:

1) **Rust test suite** (`cargo test`, `cargo test --release`)
- Unit tests for core utilities and scraper internals
- Integration-style tests that scrape a few well-known pages
- Some “boss/god” benchmarks are present but `#[ignore]` to avoid flaky CI/network behavior

2) **Release validation harness** (`python3 docs/run_release_validation.py`)
- Validates the production HTTP surface:
  - `GET /health`
  - `GET /mcp/tools`
  - `POST /mcp/call` for a set of tool scenarios
- Writes a machine-readable artifact:
  - `docs/RELEASE_READINESS_2026-02-12.json`

## Current validated tool surface

Tool count: **8**

- `search_web`
- `search_structured`
- `scrape_url`
- `scrape_batch`
- `crawl_website`
- `extract_structured`
- `research_history` (requires `QDRANT_URL` to be enabled)
- `proxy_manager` (requires proxy config / proxy sources)

## Known variability

Because this system depends on external networks/services, results can vary run-to-run:

- Search results depend on SearXNG configuration and engine availability.
- Scrape results depend on target site changes, rate limits, and blocking.
- Some tests are ignored by default to avoid brittleness.

For the authoritative run evidence, use `docs/RELEASE_READINESS_2026-02-12.json`.
