# VS Code MCP Setup

Cortex Scout is a **pure binary** MCP server (`cortex-scout-mcp`, stdio transport).

> **`RUST_LOG` warning:** Always use `RUST_LOG=warn`. At `info` or `debug` level the
> server emits hundreds of log lines per request to stderr; VS Code treats this as errors
> or times out waiting for valid JSON-RPC responses.

---

## 1. Get the binary

- Download `cortex-scout-mcp` (Linux/macOS) or `cortex-scout-mcp.exe` (Windows) from
  [GitHub Releases](https://github.com/cortex-works/cortex-scout/releases).
- Or build locally:

```bash
cd mcp-server

# Basic build
cargo build --release --bin cortex-scout-mcp

# Full build (adds hitl_web_fetch / visible-browser HITL)
cargo build --release --all-features --bin cortex-scout-mcp
```

---

## 2. Configure VS Code

VS Code reads MCP servers from two places:

| File | Top-level key | Scope |
|------|--------------|-------|
| `~/Library/Application Support/Code/User/mcp.json` (macOS global) | `"servers"` | All workspaces |
| `%APPDATA%\Code\User\mcp.json` (Windows global) | `"servers"` | All workspaces |
| `.vscode/mcp.json` in workspace | `"servers"` | This workspace only |
| `settings.json` in workspace | `"mcp.servers"` | This workspace only |

> **Key names:** The global `mcp.json` and the workspace `.vscode/mcp.json` both use the
> top-level key `"servers"`. The workspace `settings.json` uses `"mcp.servers"`. Claude
> Desktop, Cursor, and Windsurf use `"mcpServers"`.

### macOS / Linux

```jsonc
// ~/Library/Application Support/Code/User/mcp.json  (or .vscode/mcp.json)
{
  "servers": {
    "cortex-scout": {
      "type": "stdio",
      "command": "env",
      "args": [
        "RUST_LOG=warn",
        "SEARCH_ENGINES=google,bing,duckduckgo,brave",
        "LANCEDB_URI=/absolute/path/to/cortex-scout/lancedb",
        "HTTP_TIMEOUT_SECS=30",
        "MAX_CONTENT_CHARS=10000",
        "/absolute/path/to/cortex-scout/mcp-server/target/release/cortex-scout-mcp"
      ]
    }
  }
}
```

Default behavior is direct/no-proxy. Add `IP_LIST_PATH` and `PROXY_SOURCE_PATH` only if you want proxy support available. For an opt-in proxy setup, keep `ip.txt` empty and let `proxy_control grab` populate it only when an agent decides a retry should use proxies.

### Windows

Windows has no `env` command. Pass env vars as an object instead:

```jsonc
// %APPDATA%\Code\User\mcp.json
{
  "servers": {
    "cortex-scout": {
      "type": "stdio",
      "command": "C:\\Users\\YOU\\cortex-scout\\mcp-server\\target\\release\\cortex-scout-mcp.exe",
      "args": [],
      "env": {
        "RUST_LOG": "warn",
        "SEARCH_ENGINES": "google,bing,duckduckgo,brave",
        "LANCEDB_URI": "C:\\Users\\YOU\\cortex-scout\\lancedb",
        "HTTP_TIMEOUT_SECS": "30",
        "MAX_CONTENT_CHARS": "10000"
      }
    }
  }
}
```

After editing any MCP config, **restart VS Code** (Cmd+Q / Alt+F4) to reload the tool list.

---

## 3. Tool catalog

| Tool | Purpose |
|------|---------|
| `web_search` | Multi-engine web search, returns ranked snippets |
| `web_search_json` | Same as `web_search` but returns structured JSON |
| `web_fetch` | Fetch and extract text from a single URL |
| `web_fetch_batch` | Parallel fetch of multiple URLs |
| `web_crawl` | Crawl a site up to N pages deep |
| `deep_research` | Multi-step research: search + fetch + optional LLM synthesis |
| `extract_fields` | Extract structured fields from text with a schema/prompt |
| `fetch_then_extract` | One-shot: fetch URL then extract fields |
| `memory_search` | Semantic recall from past research sessions (LanceDB) |
| `proxy_control` | Rotate / list / test outbound proxies |
| `visual_scout` | Headless screenshot (confirm auth gates) |
| `hitl_web_fetch` | Visible browser fetch for CAPTCHA/login bypass |
| `human_auth_session` | Full human-in-the-loop browser session |

> `hitl_web_fetch` and `human_auth_session` require the binary built with `--all-features`.

---

## Troubleshooting

| Symptom | Fix |
|---------|-----|
| No tools appear | Confirm binary path is correct and executable (`chmod +x`) |
| Tools time out | Set `RUST_LOG=warn` (not `info` or `debug`) |
| `hitl_web_fetch` missing | Rebuild with `--all-features` |
| Config not picked up | Fully restart VS Code after editing mcp.json |
| Proxy tools fail | Proxy support is optional. If you want it, set `IP_LIST_PATH`/`PROXY_SOURCE_PATH`; keep `ip.txt` empty by default and populate it only when needed |
