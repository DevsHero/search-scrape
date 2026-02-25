---
title: CortexScout
---

# CortexScout (cortex-scout) — Search and Web Extraction Engine for AI Agents

<div align="center">
  <img src="media/logo.svg" alt="CortexScout Logo" width="180" />

  <p>
    CortexScout is the Deep Research & Web Extraction module within the Cortex-Works ecosystem.
  </p>

  <p>
    Designed for agent workloads that require token-efficient web retrieval, reliable anti-bot handling, and optional Human-in-the-Loop (HITL) fallback.
  </p>

  <p>
    <a href="https://opensource.org/licenses/MIT"><img src="https://img.shields.io/badge/License-MIT-yellow.svg" alt="MIT License" /></a>
    <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/Built%20with-Rust-orange.svg" alt="Built with Rust" /></a>
    <a href="https://modelcontextprotocol.io/"><img src="https://img.shields.io/badge/Protocol-MCP-blue.svg" alt="MCP" /></a>
    <a href="https://github.com/cortex-works/cortex-scout/actions/workflows/ci.yml"><img src="https://github.com/cortex-works/cortex-scout/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
  </p>
</div>

---

## Overview

CortexScout provides a single, self-hostable Rust binary that exposes search and extraction capabilities over MCP (stdio) and an optional HTTP server. Output formats are structured and optimized for downstream LLM use.

It is built to handle the practical failure modes of web retrieval (rate limits, bot challenges, JavaScript-heavy pages) through progressive fallbacks: native retrieval → Chromium CDP rendering → HITL workflows.

---

## Tools (Capability Roster)

| Area | MCP Tools / Capabilities |
|------|---------------------------|
| Search | `web_search`, `web_search_json` (parallel meta-search + dedup/scoring) |
| Fetch / Scrape | `web_fetch`, `web_fetch_batch` (token-efficient clean output, optional semantic filtering) |
| Crawl | `web_crawl` (bounded discovery for doc sites / sub-pages) |
| Extraction | `extract_fields`, `fetch_then_extract` (schema-driven extraction) |
| Anti-bot handling | CDP rendering, proxy rotation, block-aware retries |
| HITL fallback | `visual_scout` (screenshot for gate confirmation), `human_auth_session` (authenticated fetch with persisted sessions), `non_robot_search` (last resort rendering) |
| Memory | `memory_search` (LanceDB-backed research history) |
| Deep research | `deep_research` (multi-hop search + scrape + synthesis via OpenAI-compatible APIs) |

---

## Ecosystem Integration

While CortexScout runs as a standalone tool today, it is designed to integrate with CortexDB and CortexStudio for multi-agent scaling, shared retrieval artifacts, and centralized governance.

---

## Anti-Bot Efficacy & Validation

This repository includes captured evidence artifacts that validate extraction and HITL flows against representative protected targets.

| Target | Protection | Evidence | Notes |
|--------|-----------|----------|-------|
| LinkedIn | Cloudflare + Auth | [JSON](proof/linkedin_evidence.json) · [Snippet](proof/linkedin_raw_snippet.txt) | Auth-gated listings extraction |
| Ticketmaster | Cloudflare Turnstile | [JSON](proof/ticketmaster_evidence.json) · [Snippet](proof/ticketmaster_raw_snippet.txt) | Challenge-handled extraction |
| Airbnb | DataDome | [JSON](proof/airbnb_evidence.json) · [Snippet](proof/airbnb_raw_snippet.txt) | Large result sets under bot controls |
| Upwork | reCAPTCHA | [JSON](proof/upwork_evidence.json) · [Snippet](proof/upwork_raw_snippet.txt) | Protected listings retrieval |
| Amazon | AWS Shield | [JSON](proof/amazon_evidence.json) · [Snippet](proof/amazon_raw_snippet.txt) | Search result extraction |
| nowsecure.nl | Cloudflare | [JSON](proof/nowsecure_evidence.json) | Manual return path validated |

See [proof/README.md](proof/README.md) for methodology and raw outputs.

---

## Quick Start

### Option A — Prebuilt binaries

Download the latest release assets from GitHub Releases and run one of:

- `cortex-scout-mcp` — MCP stdio server (recommended for VS Code / Cursor / Claude Desktop)
- `cortex-scout` — optional HTTP server (default port `5000`; override via `--port`, `PORT`, or `CORTEX_SCOUT_PORT`)

Health check (HTTP server):

```bash
./cortex-scout --port 5000
curl http://localhost:5000/health
```

### Option B — Build from source

```bash
git clone https://github.com/cortex-works/cortex-scout.git
cd cortex-scout

cd mcp-server
cargo build --release --all-features
```

---

## MCP Integration (VS Code / Cursor / Claude Desktop)

Add a server entry to your MCP config. Example for VS Code (stdio transport):

```jsonc
{
  "servers": {
    "cortex-scout": {
      "type": "stdio",
      "command": "env",
      "args": [
        "RUST_LOG=info",
        "SEARCH_ENGINES=google,bing,duckduckgo,brave",
        "LANCEDB_URI=/YOUR_PATH/cortex-scout/lancedb",
        "HTTP_TIMEOUT_SECS=30",
        "MAX_CONTENT_CHARS=10000",
        "IP_LIST_PATH=/YOUR_PATH/cortex-scout/ip.txt",
        "PROXY_SOURCE_PATH=/YOUR_PATH/cortex-scout/proxy_source.json",
        "/YOUR_PATH/cortex-scout/mcp-server/target/release/cortex-scout-mcp"
      ]
    }
  }
}
```

Multi-IDE guide: [docs/IDE_SETUP.md](docs/IDE_SETUP.md)

---

## Configuration (cortex-scout.json)

Create `cortex-scout.json` in the same directory as the binary (or repository root). All fields are optional; environment variables act as fallback.

```json
{
  "deep_research": {
    "enabled": true,
    "llm_base_url": "http://localhost:1234/v1",
    "llm_api_key": "",
    "llm_model": "lfm2-2.6b",
    "synthesis_enabled": true,
    "synthesis_max_sources": 3,
    "synthesis_max_chars_per_source": 800,
    "synthesis_max_tokens": 1024
  }
}
```

---

## Key Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `CHROME_EXECUTABLE` | auto-detected | Override path to Chromium/Chrome/Brave |
| `SEARCH_ENGINES` | `google,bing,duckduckgo,brave` | Active engines (comma-separated) |
| `SEARCH_MAX_RESULTS_PER_ENGINE` | `10` | Results per engine before merge |
| `SEARCH_CDP_FALLBACK` | auto | Retry blocked retrieval via native Chromium CDP |
| `LANCEDB_URI` | — | Path for semantic memory (optional) |
| `CORTEX_SCOUT_MEMORY_DISABLED` | `0` | Set `1` to disable memory features |
| `HTTP_TIMEOUT_SECS` | `30` | Per-request timeout |
| `OUTBOUND_LIMIT` | `32` | Max concurrent outbound connections |
| `MAX_CONTENT_CHARS` | `10000` | Max chars per scraped document |
| `IP_LIST_PATH` | — | Proxy IP list path |
| `PROXY_SOURCE_PATH` | — | Proxy source definition path |
| `DEEP_RESEARCH_ENABLED` | `1` | Disable the `deep_research` tool at runtime by setting `0` |
| `OPENAI_API_KEY` | — | API key for synthesis (omit for key-less local endpoints) |
| `OPENAI_BASE_URL` | `https://api.openai.com/v1` | OpenAI-compatible endpoint (Ollama/LM Studio supported) |
| `DEEP_RESEARCH_LLM_MODEL` | `gpt-4o-mini` | Model name (OpenAI-compatible) |
| `DEEP_RESEARCH_SYNTHESIS_MAX_TOKENS` | `1024` | Response token budget for synthesis |

---

## Agent Best Practices

Recommended operational flow:

1) Use `memory_search` before new research runs to avoid re-fetching.
2) Prefer `web_search_json` for initial discovery (search + content summaries).
3) Use `web_fetch` for known URLs; use `output_format="clean_json"` and set `query` + `strict_relevance=true` for token efficiency.
4) On 403/429/rate-limit: call `proxy_control` with `action:"grab"`, then retry with `use_proxy:true`.
5) For auth walls: `visual_scout` to confirm gating, then `human_auth_session` to complete login and persist sessions under `~/.cortex-scout/sessions/`.

Full agent rules: [/.github/copilot-instructions.md](.github/copilot-instructions.md)

---

## Versioning and Changelog

See [CHANGELOG.md](CHANGELOG.md).

---

## License

MIT. See [LICENSE](LICENSE).

