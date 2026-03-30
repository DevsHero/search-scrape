# CortexScout (cortex-scout) — Search and Web Extraction Engine for AI Agents

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

  </p>
</div>

---

## Overview

CortexScout provides a single, self-hostable Rust binary that exposes search, extraction, and **stateful browser automation** capabilities over MCP (stdio) and an optional HTTP server. Output formats are structured and optimized for downstream LLM use.

It is built to handle the practical failure modes of web retrieval (rate limits, bot challenges, JavaScript-heavy pages) through progressive fallbacks: native retrieval → Chromium CDP rendering → **Stateful E2E Testing** → HITL workflows.

---

## Tools (Capability Roster)

| Area | MCP Tools / Capabilities |
|------|---------------------------|
| Search | `web_search`, `web_search_json` (parallel meta-search + dedup/scoring) |
| Fetch| `web_fetch`, `web_fetch_batch` (token-efficient clean output) |
| Crawl | `web_crawl` (bounded discovery for doc sites / sub-pages) |
| Extraction | `extract_fields`, `fetch_then_extract` (schema-driven extraction) |
| Automation | `scout_browser_automate` (stateful omni-tool), `scout_agent_profile_auth` (HITL portal), `scout_browser_close` |
| Anti-bot handling | CDP rendering, proxy rotation, block-aware retries |
| HITL | `visual_scout`, `human_auth_session`, `non_robot_search` |
| Memory | `memory_search` (LanceDB-backed research history) |
| Deep research | `deep_research` (multi-hop search + scrape + synthesis) |
---

## Ecosystem Integration

While CortexScout runs as a standalone tool today, it is designed to integrate with CortexDB and CortexStudio for multi-agent scaling, shared retrieval artifacts, and centralized governance.

---

## 🎭 The "Playwright Killer" (Stateful Browser Automation)

CortexScout includes a built-in, stateful CDP automation engine designed specifically for AI Agents, completely replacing heavy frameworks like Playwright or Cypress for E2E testing workflows.

- **The Silent Omni-Tool (`scout_browser_automate`)**: Instead of calling dozens of tools, agents pass an array of `steps` (navigate, click, type, scroll, press_key, snapshot, screenshot). The entire sequence executes in a single LLM turn, saving massive amounts of context tokens.
- **Persistent Agent Profile**: Automation runs silently in the background (`--headless=new`) using a dedicated isolated profile (`~/.cortex-scout/agent_profile`). It maintains cookies, localStorage, and session state across tool calls without causing `SingletonLock` collisions with your active desktop browser.
- **QA Mock & Assert Engine**: Built for enterprise E2E testing. Agents can inject XHR/Fetch network interceptors (`mock_api`) and run fail-fast DOM assertions (`assert`) that immediately halt the sequence if a UI state is incorrect.
- **The Agent Auth Portal (`scout_agent_profile_auth`)**: If the silent agent encounters a CAPTCHA or complex OAuth login (like Google/Microsoft) on a new domain, this tool launches the agent's profile in a **visible** window. You solve the CAPTCHA once, the cookies are saved, and the agent returns to silent automation forever.
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

Install `protoc` first. `lance-encoding` uses Protocol Buffers during the release build, so `protoc` must be on your PATH.

- macOS: `brew install protobuf`
- Ubuntu/Debian: `sudo apt-get install -y protobuf-compiler`
- Fedora: `sudo dnf install -y protobuf-compiler`

Basic build (search, scrape, deep research, memory):

```bash
git clone https://github.com/cortex-works/cortex-scout.git
cd cortex-scout
cargo build --release --manifest-path mcp-server/Cargo.toml --bin cortex-scout-mcp
```

This works from the repository root because the manifest path is explicit.

Full build (includes `hitl_web_fetch` / visible-browser HITL):

```bash
cargo build --release --manifest-path mcp-server/Cargo.toml --all-features --bin cortex-scout-mcp
```

If you also want the optional HTTP server binary, build it explicitly with `cargo build --release --bin cortex-scout`.

Local MCP smoke test:

```bash
python3 publish/ci/smoke_mcp.py
```

This runs a newline-delimited JSON-RPC stdio session against the local `cortex-scout-mcp` binary and exercises the main public tools with safe example inputs.

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
        "/YOUR_PATH/cortex-scout/mcp-server/target/release/cortex-scout-mcp"
      ]
    }
  }
}
```

Default behavior is direct/no-proxy. Add `IP_LIST_PATH` and `PROXY_SOURCE_PATH` only if you want proxy tools available. If you want `proxy_control` available without routing normal traffic through proxies, point `IP_LIST_PATH` at an empty `ip.txt` file and let agents populate it on demand.

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
        "OPENAI_BASE_URL=https://openrouter.ai/api/v1",
        "OPENAI_API_KEY=sk-or-v1-...",
        "DEEP_RESEARCH_LLM_MODEL=moonshotai/kimi-k2.5",
        "DEEP_RESEARCH_ENABLED=1",
        "DEEP_RESEARCH_SYNTHESIS=1",
        "DEEP_RESEARCH_SYNTHESIS_MAX_TOKENS=4096",
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
| `IP_LIST_PATH` | — | Optional path to `ip.txt` (one proxy per line: `http://`, `socks5://`). Leave unset to disable proxy support entirely, or point at an empty file to keep proxy tools available but inactive by default |
| `PROXY_SOURCE_PATH` | — | Optional path to `proxy_source.json` (used by `proxy_control grab`) |

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
8. For UI Automation & E2E Testing: Use `scout_browser_automate` to pass a sequence of actions. If the sequence fails due to a login wall or CAPTCHA, call `scout_agent_profile_auth` to request human assistance, then resume your automated steps.
---

## FAQ

### Why does `deep_research` with Ollama or `qwen3.5` sometimes fail or fall back to heuristic mode?

Some reasoning-capable local models return OpenAI-compatible `/v1/chat/completions` responses with `message.reasoning` populated but `message.content` empty. Cortex Scout now retries local Ollama endpoints through native `/api/chat` with `think:false` when that pattern is detected.

Recommended config for local 4B-class Ollama models:

- `llm_api_key: ""` in `cortex-scout.json` is valid and means "no auth required"
- Keep `synthesis_max_sources` at `1-2`
- Keep `synthesis_max_chars_per_source` around `600-1000`
- Keep `synthesis_max_tokens` around `512-768`

If you still see slow or unstable synthesis, reduce `synthesis_max_sources` before increasing token limits.


### Why do I see Chromium profile lock errors?

Each headless request uses a unique temporary profile, so normal scraping and deep_research are safe from profile lock races. Only HITL flows (like non_robot_search) using a real browser profile can hit a lock if you run them concurrently or have Brave/Chrome open on the same profile. To avoid: run HITL calls one at a time, and close all browser windows before reusing a profile.

Checklist:
1. Use a recent build (2026-03-05 or newer)
2. Avoid persistent profile paths unless you need a logged-in session
3. Run HITL/profile flows sequentially
4. Close all browser windows before reusing a profile
5. Let Cortex Scout use its own temp profiles for concurrent research

### My MCP client connects but tools fail or time out immediately. What should I check first?

Check these before anything else:

1. Use `RUST_LOG=warn`, not `info`.
2. On macOS/Linux `env`-style configs, pass the binary path directly after the env assignments. Do not insert `"--"` in `mcp.json` args.
3. On Windows, do not use `env`; use `command` plus an `env` object.
4. Make sure the binary path points to a current build.

---

## Versioning and Changelog

See [CHANGELOG.md](CHANGELOG.md).

---

## License

MIT. See [LICENSE](LICENSE).

