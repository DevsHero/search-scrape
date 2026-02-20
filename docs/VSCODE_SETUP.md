# VS Code MCP Setup (Zero-Docker)

ShadowCrawl is **pure binary** and exposes an MCP server over **stdio** via the `shadowcrawl-mcp` executable.

## 1) Get the binary

Choose one:

- Download `shadowcrawl-mcp` from GitHub Releases
- Or build it locally:

```bash
cd mcp-server
cargo build --release --features non_robot_search --bin shadowcrawl-mcp
```

## 2) Configure VS Code MCP

In VS Code settings (workspace `settings.json`), add an MCP server pointing at your local binary:

```json
{
  "mcp.servers": {
    "shadowcrawl": {
      "type": "stdio",
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

## 3) Restart VS Code (tool cache)

VS Code caches tool lists. If tools don’t appear or look stale:

- fully quit (Cmd+Q)
- reopen VS Code

## Tool catalog

Tool names shown in VS Code come from the MCP tool catalog. The core agent-facing tools are:

- `memory_search` (semantic research recall)
- `web_search` / `web_search_json`
- `web_fetch` / `web_fetch_batch`
- `web_crawl`
- `extract_fields` (schema/prompt extraction)
- `fetch_then_extract` (one-shot fetch + extract)
- `proxy_control` (rotate/list/test proxies)
- `visual_scout` (headless screenshot for auth-gate confirmation)
- `hitl_web_fetch` / `human_auth_session` (last-resort login/CAPTCHA bypass)

Optional: `non_robot_search` (HITL / visible browser) when built with `--features non_robot_search`.

## non_robot_search (HITL) — important

`non_robot_search` opens a **local visible GUI browser** (Brave/Chrome) and may require user interaction.

- ✅ Tested: macOS
- ⚠️ Requires a local desktop session (GUI browser)

Full guide (Brave/profile/consent/kill-switch):
- [docs/NON_ROBOT_SEARCH.md](docs/NON_ROBOT_SEARCH.md)

## Troubleshooting

- If tools don’t appear: restart VS Code (Cmd+Q).
- macOS preflight (especially for HITL): run `shadowcrawl-mcp --setup` (or `shadowcrawl --setup`).
