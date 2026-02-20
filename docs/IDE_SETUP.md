# MCP Client Setup (IDE / Apps) — Zero-Docker

ShadowCrawl is **pure binary**. Use the `shadowcrawl-mcp` executable (stdio MCP server).

## Prereq

Get a `shadowcrawl-mcp` binary:

- Download from GitHub Releases, or
- Build locally:

```bash
cd mcp-server
cargo build --release --features non_robot_search --bin shadowcrawl-mcp
```

## Claude Desktop (macOS)

Claude Desktop uses `claude_desktop_config.json`.

- Open: Claude Desktop → Settings → Developer → Edit Config
- File path (macOS): `~/Library/Application Support/Claude/claude_desktop_config.json`

Example:

```json
{
  "mcpServers": {
    "shadowcrawl": {
      "command": "env",
      "args": [
        "RUST_LOG=info",
        "SEARCH_ENGINES=google,bing,duckduckgo,brave",
        "SEARCH_CDP_FALLBACK=true",
        "SEARCH_TIER2_NON_ROBOT=true",
        "LANCEDB_URI=/absolute/path/to/search-scrape/lancedb",
        "HTTP_TIMEOUT_SECS=30",
        "HTTP_CONNECT_TIMEOUT_SECS=10",
        "OUTBOUND_LIMIT=32",
        "MAX_CONTENT_CHARS=10000",
        "MAX_LINKS=100",
        "IP_LIST_PATH=/absolute/path/to/search-scrape/ip.txt",
        "PROXY_SOURCE_PATH=/absolute/path/to/search-scrape/proxy_source.json",
        "/absolute/path/to/search-scrape/mcp-server/target/release/shadowcrawl-mcp"
      ]
    }
  }
}
```

Notes:
- Claude’s top-level key is `mcpServers` (not `servers`).

---

## Cursor

Cursor stores MCP config in `~/.cursor/mcp.json` (and also supports UI configuration).

Example:

```json
{
  "mcpServers": {
    "shadowcrawl": {
      "command": "env",
      "args": [
        "RUST_LOG=info",
        "SEARCH_ENGINES=google,bing,duckduckgo,brave",
        "SEARCH_CDP_FALLBACK=true",
        "SEARCH_TIER2_NON_ROBOT=true",
        "LANCEDB_URI=/absolute/path/to/search-scrape/lancedb",
        "HTTP_TIMEOUT_SECS=30",
        "HTTP_CONNECT_TIMEOUT_SECS=10",
        "OUTBOUND_LIMIT=32",
        "MAX_CONTENT_CHARS=10000",
        "MAX_LINKS=100",
        "IP_LIST_PATH=/absolute/path/to/search-scrape/ip.txt",
        "PROXY_SOURCE_PATH=/absolute/path/to/search-scrape/proxy_source.json",
        "/absolute/path/to/search-scrape/mcp-server/target/release/shadowcrawl-mcp"
      ]
    }
  }
}
```

Notes:
- Cursor uses `mcpServers`.
- If tools show as missing, restart Cursor after editing.

---

## Windsurf (Codeium)

Windsurf uses a separate config file (commonly named `mcp_config.json`).

Typical locations (varies by OS/version):
- `~/.codeium/windsurf/mcp_config.json`

Example:

```json
{
  "mcpServers": {
    "shadowcrawl": {
      "command": "env",
      "args": [
        "RUST_LOG=info",
        "SEARCH_ENGINES=google,bing,duckduckgo,brave",
        "SEARCH_CDP_FALLBACK=true",
        "SEARCH_TIER2_NON_ROBOT=true",
        "LANCEDB_URI=/absolute/path/to/search-scrape/lancedb",
        "HTTP_TIMEOUT_SECS=30",
        "HTTP_CONNECT_TIMEOUT_SECS=10",
        "OUTBOUND_LIMIT=32",
        "MAX_CONTENT_CHARS=10000",
        "MAX_LINKS=100",
        "IP_LIST_PATH=/absolute/path/to/search-scrape/ip.txt",
        "PROXY_SOURCE_PATH=/absolute/path/to/search-scrape/proxy_source.json",
        "/absolute/path/to/search-scrape/mcp-server/target/release/shadowcrawl-mcp"
      ]
    }
  }
}
```

Notes:
- If Windsurf doesn’t pick up changes, fully quit and reopen.

---

## Continue.dev

Continue MCP config is file-based and commonly expects **YAML** MCP server definitions under:

- `.continue/mcpServers/*.yaml`

Create: `.continue/mcpServers/shadowcrawl.yaml`

```yaml
name: shadowcrawl
version: 3.0.0
schema: v1

mcpServers:
  - name: shadowcrawl
    command: env
    args:
      - RUST_LOG=info
      - SEARCH_ENGINES=google,bing,duckduckgo,brave
      - SEARCH_CDP_FALLBACK=true
      - SEARCH_TIER2_NON_ROBOT=true
      - LANCEDB_URI=/absolute/path/to/search-scrape/lancedb
      - HTTP_TIMEOUT_SECS=30
      - HTTP_CONNECT_TIMEOUT_SECS=10
      - OUTBOUND_LIMIT=32
      - MAX_CONTENT_CHARS=10000
      - MAX_LINKS=100
      - IP_LIST_PATH=/absolute/path/to/search-scrape/ip.txt
      - PROXY_SOURCE_PATH=/absolute/path/to/search-scrape/proxy_source.json
      - /absolute/path/to/search-scrape/mcp-server/target/release/shadowcrawl-mcp
```

Notes:
- MCP can only be used in **agent** mode.
- Continue’s MCP server configs are per-workspace.

---

## Troubleshooting

- If the client shows "no tools found", confirm your `shadowcrawl-mcp` path is correct and executable.
- If you use proxies, ensure `ip.txt` and `proxy_source.json` exist and `IP_LIST_PATH` / `PROXY_SOURCE_PATH` point to them.
