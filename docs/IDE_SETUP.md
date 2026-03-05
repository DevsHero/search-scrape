# MCP Client Setup — IDE / App Guide

Cortex Scout is a **pure binary** MCP server. Use the `cortex-scout-mcp` stdio executable.

> **`RUST_LOG` warning:** Always set `RUST_LOG=warn`. At `info` level the server emits
> hundreds of log lines per request to stderr; many MCP clients treat this as errors or
> time out waiting for valid JSON-RPC output.

---

## Getting the binary

**Option A — Download a prebuilt release**

Download `cortex-scout-mcp` (Linux/macOS) or `cortex-scout-mcp.exe` (Windows) from
[GitHub Releases](https://github.com/cortex-works/cortex-scout/releases).

**Option B — Build from source**

Basic build (search, scrape, deep research, memory):

```bash
cd mcp-server
cargo build --release --bin cortex-scout-mcp
```

Full build (adds `hitl_web_fetch` / visible-browser HITL):

```bash
cd mcp-server
cargo build --release --all-features --bin cortex-scout-mcp
```

## VS Code

VS Code reads MCP servers from two places (both are valid):

| File | Top-level key | Scope |
|------|--------------|-------|
| `~/Library/Application Support/Code/User/mcp.json` (macOS) | `"servers"` | Global (all workspaces) |
| `.vscode/mcp.json` in workspace | `"servers"` | This workspace only |
| `settings.json` in workspace | `"mcp.servers"` | This workspace only |

> **Note:** VS Code uses `"servers"`, not `"mcpServers"`. Claude Desktop, Cursor, and Windsurf use `"mcpServers"`.

### macOS / Linux

```jsonc
// mcp.json  (global: ~/Library/Application Support/Code/User/mcp.json)
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
        "IP_LIST_PATH=/absolute/path/to/cortex-scout/ip.txt",
        "PROXY_SOURCE_PATH=/absolute/path/to/cortex-scout/proxy_source.json",
        "--",
        "/absolute/path/to/cortex-scout/mcp-server/target/release/cortex-scout-mcp"
      ]
    }
  }
}
```

### Windows

Windows has no `env` command. Use the `command` + `env` object format:

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
        "MAX_CONTENT_CHARS": "10000",
        "IP_LIST_PATH": "C:\\Users\\YOU\\cortex-scout\\ip.txt",
        "PROXY_SOURCE_PATH": "C:\\Users\\YOU\\cortex-scout\\proxy_source.json"
      }
    }
  }
}
```

After editing, restart VS Code (Cmd+Q / Alt+F4) to reload the tool list.

---

## Claude Desktop

File location:
- macOS: `~/Library/Application Support/Claude/claude_desktop_config.json`
- Windows: `%APPDATA%\Claude\claude_desktop_config.json`

Claude uses `"mcpServers"` (not `"servers"`).

```jsonc
{
  "mcpServers": {
    "cortex-scout": {
      "command": "env",
      "args": [
        "RUST_LOG=warn",
        "SEARCH_ENGINES=google,bing,duckduckgo,brave",
        "LANCEDB_URI=/absolute/path/to/cortex-scout/lancedb",
        "HTTP_TIMEOUT_SECS=30",
        "MAX_CONTENT_CHARS=10000",
        "IP_LIST_PATH=/absolute/path/to/cortex-scout/ip.txt",
        "PROXY_SOURCE_PATH=/absolute/path/to/cortex-scout/proxy_source.json",
        "--",
        "/absolute/path/to/cortex-scout/mcp-server/target/release/cortex-scout-mcp"
      ]
    }
  }
}
```

> Windows: replace `"command": "env", "args": [...]` with `"command": "<binary path>"` and
> `"env": { "KEY": "value", ... }` (same pattern as the VS Code Windows example above).

---

## Cursor

Cursor stores MCP config in `~/.cursor/mcp.json` (also configurable via UI).

```jsonc
{
  "mcpServers": {
    "cortex-scout": {
      "command": "env",
      "args": [
        "RUST_LOG=warn",
        "SEARCH_ENGINES=google,bing,duckduckgo,brave",
        "LANCEDB_URI=/absolute/path/to/cortex-scout/lancedb",
        "HTTP_TIMEOUT_SECS=30",
        "MAX_CONTENT_CHARS=10000",
        "IP_LIST_PATH=/absolute/path/to/cortex-scout/ip.txt",
        "PROXY_SOURCE_PATH=/absolute/path/to/cortex-scout/proxy_source.json",
        "--",
        "/absolute/path/to/cortex-scout/mcp-server/target/release/cortex-scout-mcp"
      ]
    }
  }
}
```

> If tools show as missing, fully restart Cursor after editing.

---

## Windsurf (Codeium)

Windsurf config file: `~/.codeium/windsurf/mcp_config.json`

```jsonc
{
  "mcpServers": {
    "cortex-scout": {
      "command": "env",
      "args": [
        "RUST_LOG=warn",
        "SEARCH_ENGINES=google,bing,duckduckgo,brave",
        "LANCEDB_URI=/absolute/path/to/cortex-scout/lancedb",
        "HTTP_TIMEOUT_SECS=30",
        "MAX_CONTENT_CHARS=10000",
        "IP_LIST_PATH=/absolute/path/to/cortex-scout/ip.txt",
        "PROXY_SOURCE_PATH=/absolute/path/to/cortex-scout/proxy_source.json",
        "--",
        "/absolute/path/to/cortex-scout/mcp-server/target/release/cortex-scout-mcp"
      ]
    }
  }
}
```

> If Windsurf doesn't pick up changes, fully quit and reopen.

---

## Continue.dev

Create `.continue/mcpServers/cortex-scout.yaml` in your project root:

```yaml
name: cortex-scout
version: 1.0.0
schema: v1

mcpServers:
  - name: cortex-scout
    command: env
    args:
      - RUST_LOG=warn
      - SEARCH_ENGINES=google,bing,duckduckgo,brave
      - LANCEDB_URI=/absolute/path/to/cortex-scout/lancedb
      - HTTP_TIMEOUT_SECS=30
      - MAX_CONTENT_CHARS=10000
      - IP_LIST_PATH=/absolute/path/to/cortex-scout/ip.txt
      - PROXY_SOURCE_PATH=/absolute/path/to/cortex-scout/proxy_source.json
      - --
      - /absolute/path/to/cortex-scout/mcp-server/target/release/cortex-scout-mcp
```

> MCP tools are only available in **agent** mode. Config is per-workspace.

---

## Deep research config

To enable AI-synthesized research reports, add these optional env vars to any config above:

| Variable | Example | Purpose |
|----------|---------|----------|
| `DEEP_RESEARCH_SYNTHESIS` | `true` | Enable LLM synthesis step |
| `DEEP_RESEARCH_SYNTHESIS_MAX_TOKENS` | `8192` | Max tokens in synthesis output |
| `DEEP_RESEARCH_SYNTHESIS_MAX_SOURCES` | `10` | Max sources fed to LLM |
| `DEEP_RESEARCH_SYNTHESIS_MAX_CHARS_PER_SOURCE` | `4000` | Max chars extracted per source |

These require an `OPENAI_API_KEY` (or compatible endpoint) in the environment.

---

## Troubleshooting

| Symptom | Fix |
|---------|-----|
| "no tools found" | Check binary path is correct and executable (`chmod +x`) |
| Tools time out immediately | Ensure `RUST_LOG=warn` — not `info` or `debug` |
| Proxy tools fail | Confirm `ip.txt` and `proxy_source.json` exist and path vars are set |
| `hitl_web_fetch` not available | Binary must be built with `--all-features` |
| Config not picked up | Fully restart the client app after editing the JSON/YAML |
