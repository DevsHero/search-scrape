# CortexScout (cortex-scout) — Search and Web Extraction Engine for AI Agents

<div align="center">


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
| Fetch| `web_fetch`, `web_fetch_batch` (token-efficient clean output, optional semantic filtering) |
| Crawl | `web_crawl` (bounded discovery for doc sites / sub-pages) |
| Extraction | `extract_fields`, `fetch_then_extract` (schema-driven extraction) |
| Anti-bot handling | CDP rendering, proxy rotation, block-aware retries |
| HITL | `visual_scout` (screenshot for gate confirmation), `human_auth_session` (authenticated fetch with persisted sessions), `non_robot_search` (last resort rendering) |
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

Basic build (search, scrape, deep research, memory):

```bash
git clone https://github.com/cortex-works/cortex-scout.git
cd cortex-scout/mcp-server
cargo build --release
```

Full build (includes `hitl_web_fetch` / visible-browser HITL):

```bash
cargo build --release --all-features
```

---

## MCP Integration (VS Code / Cursor / Claude Desktop)

Add a server entry to your MCP config.

**VS Code** (`mcp.json` — global, or `settings.json` under `mcp.servers`):

```jsonc
// mcp.json (global): top-level key is "servers"
// settings.json (workspace): use "mcp.servers" instead
{
  "servers": {
    "cortex-scout": {
      "type": "stdio",
      "command": "env",
      "args": [
        "RUST_LOG=warn",
        "SEARCH_ENGINES=google,bing,duckduckgo,brave",
        "LANCEDB_URI=/YOUR_PATH/cortex-scout/lancedb",
        "HTTP_TIMEOUT_SECS=30",
        "MAX_CONTENT_CHARS=10000",
        "IP_LIST_PATH=/YOUR_PATH/cortex-scout/ip.txt",
        "PROXY_SOURCE_PATH=/YOUR_PATH/cortex-scout/proxy_source.json",
        "--",
        "/YOUR_PATH/cortex-scout/mcp-server/target/release/cortex-scout-mcp"
      ]
    }
  }
}
```

> **Important:** Always use `RUST_LOG=warn`, not `info`. At `info` level, the server emits hundreds of log lines per request to stderr, which can confuse MCP clients that monitor stderr.

> **Windows:** Windows has no `env` command. Use the `command`+`env` object format instead — see [docs/IDE_SETUP.md](docs/IDE_SETUP.md).

**With deep research (LLM synthesis via OpenRouter / any OpenAI-compatible API):**

```jsonc
{
  "servers": {
    "cortex-scout": {
      "type": "stdio",
      "command": "env",
      "args": [
        "RUST_LOG=warn",
        "SEARCH_ENGINES=google,bing,duckduckgo,brave",
        "LANCEDB_URI=/YOUR_PATH/cortex-scout/lancedb",
        "HTTP_TIMEOUT_SECS=30",
        "MAX_CONTENT_CHARS=10000",
        "IP_LIST_PATH=/YOUR_PATH/cortex-scout/ip.txt",
        "PROXY_SOURCE_PATH=/YOUR_PATH/cortex-scout/proxy_source.json",
        "OPENAI_BASE_URL=https://openrouter.ai/api/v1",
        "OPENAI_API_KEY=sk-or-v1-...",
        "DEEP_RESEARCH_LLM_MODEL=moonshotai/kimi-k2.5",
        "DEEP_RESEARCH_ENABLED=1",
        "DEEP_RESEARCH_SYNTHESIS=1",
        "DEEP_RESEARCH_SYNTHESIS_MAX_TOKENS=4096",
        "--",
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

### Core

| Variable | Default | Description |
|----------|---------|-------------|
| `RUST_LOG` | `warn` | Log level. **Keep `warn` for MCP stdio** — `info` floods stderr and confuses MCP clients |
| `HTTP_TIMEOUT_SECS` | `30` | Per-request read timeout (seconds) |
| `HTTP_CONNECT_TIMEOUT_SECS` | `10` | TCP connect timeout (seconds) |
| `OUTBOUND_LIMIT` | `32` | Max concurrent outbound HTTP connections |
| `MAX_CONTENT_CHARS` | `10000` | Max characters returned per scraped page |

### Browser / Anti-bot

| Variable | Default | Description |
|----------|---------|-------------|
| `CHROME_EXECUTABLE` | auto-detected | Override path to Chromium/Chrome/Brave binary |
| `SEARCH_CDP_FALLBACK` | `true` | Retry search engine fetches via native Chromium CDP when blocked |
| `SEARCH_TIER2_NON_ROBOT` | unset | Set `1` to allow `hitl_web_fetch` as last-resort search escalation |
| `MAX_LINKS` | `100` | Max links followed per page crawl |

### Search

| Variable | Default | Description |
|----------|---------|-------------|
| `SEARCH_ENGINES` | `google,bing,duckduckgo,brave` | Active engines (comma-separated) |
| `SEARCH_MAX_RESULTS_PER_ENGINE` | `10` | Results per engine before merge/dedup |

### Proxy

| Variable | Default | Description |
|----------|---------|-------------|
| `IP_LIST_PATH` | — | Path to `ip.txt` (one proxy per line: `http://`, `socks5://`) |
| `PROXY_SOURCE_PATH` | — | Path to `proxy_source.json` (used by `proxy_control grab`) |

### Semantic Memory (LanceDB)

| Variable | Default | Description |
|----------|---------|-------------|
| `LANCEDB_URI` | — | Directory path for persistent research memory. Omit to disable |
| `CORTEX_SCOUT_MEMORY_DISABLED` | `0` | Set `1` to disable memory even when `LANCEDB_URI` is set |
| `MODEL2VEC_MODEL` | built-in | HuggingFace model ID or local path for embedding (e.g. `minishlab/potion-base-8M`) |

### Deep Research

| Variable | Default | Description |
|----------|---------|-------------|
| `DEEP_RESEARCH_ENABLED` | `1` | Set `0` to disable the `deep_research` tool at runtime |
| `OPENAI_API_KEY` | — | API key for LLM synthesis. Omit for key-less local endpoints (Ollama) |
| `OPENAI_BASE_URL` | `https://api.openai.com/v1` | OpenAI-compatible endpoint (OpenRouter, Ollama, LM Studio, etc.) |
| `DEEP_RESEARCH_LLM_MODEL` | `gpt-4o-mini` | Model identifier (must be supported by the endpoint) |
| `DEEP_RESEARCH_SYNTHESIS` | `1` | Set `0` to skip LLM synthesis (search+scrape only) |
| `DEEP_RESEARCH_SYNTHESIS_MAX_TOKENS` | `1024` | Max tokens for synthesis response. Use `4096`+ for large-context models |
| `DEEP_RESEARCH_SYNTHESIS_MAX_SOURCES` | `8` | Max source documents fed to LLM synthesis |
| `DEEP_RESEARCH_SYNTHESIS_MAX_CHARS_PER_SOURCE` | `2500` | Max characters extracted per source for synthesis |

### HTTP Server only

| Variable | Default | Description |
|----------|---------|-------------|
| `CORTEX_SCOUT_PORT` / `PORT` | `5000` | Listening port for the HTTP server binary (`cortex-scout`) |

---

## Agent Best Practices

Recommended operational flow:

1. Call `memory_search` before any new research run — skip live fetching if similarity ≥ 0.60 and `skip_live_fetch` is `true`.
2. For initial topic discovery use `web_search_json` (returns structured snippets, lower token cost than full scrape).
3. For known URLs use `web_fetch` with `output_format="clean_json"`, set `query` + `strict_relevance=true` to truncate irrelevant content.
4. On 403/429: call `proxy_control` with `action:"grab"` to refresh the proxy list, then retry with `use_proxy:true`.
5. For auth-gated pages: `visual_scout` to confirm the gate type → `human_auth_session` to complete login (cookies persisted under `~/.cortex-scout/sessions/`).
6. For deep research: `deep_research` handles multi-hop search + scrape + LLM synthesis automatically. Tune `depth` (1–3) and `max_sources` per run cost budget.
7. For CAPTCHA or heavy JS pages that all other paths fail: `hitl_web_fetch` opens a visible Brave/Chrome window for human completion (requires `--all-features` build and a local desktop session).

---

## Versioning and Changelog

See [CHANGELOG.md](CHANGELOG.md).

---

## License

MIT. See [LICENSE](LICENSE).

