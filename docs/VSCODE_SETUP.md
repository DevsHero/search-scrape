# VS Code MCP Setup (macOS-tested)

This repo exposes an MCP server over **stdio** via the `shadowcrawl-mcp` binary.

Recommended setup: run via Docker Compose so SearXNG/Browserless are already wired.

## 1) Start the stack

```bash
docker compose -f docker-compose-local.yml up -d --build
```

## 2) Configure VS Code MCP

In VS Code settings (workspace `settings.json`), add an MCP server pointing at the running container:

```json
{
  "mcp.servers": {
    "shadowcrawl": {
      "command": "docker",
      "args": [
        "compose",
        "-f",
        "/absolute/path/to/search-scrape/docker-compose-local.yml",
        "exec",
        "-i",
        "-T",
        "shadowcrawl",
        "shadowcrawl-mcp"
      ],
      "env": {
        "RUST_LOG": "info"
      }
    }
  }
}
```

## 3) Restart VS Code (tool cache)

VS Code caches tool lists. If tools don’t appear or look stale:
- fully quit (Cmd+Q)
- reopen VS Code

## Tool catalog

- Default build: 8 tools (`search_web`, `search_structured`, `scrape_url`, `scrape_batch`, `crawl_website`, `extract_structured`, `research_history`, `proxy_manager`)
- Optional: `non_robot_search` (HITL / visible browser) when built with `--features non_robot_search`

## non_robot_search (HITL) — important

`non_robot_search` opens a **local visible GUI browser** (Brave/Chrome) and may require user interaction.

- ✅ Tested: macOS
- ⚠️ Not supported via `docker compose exec ... shadowcrawl-mcp` for typical setups (no GUI browser in the container)

If you want to use HITL in VS Code, run a **native** MCP stdio server built with the feature flag and point VS Code to the local binary.

Full guide (Brave/profile/consent/kill-switch):
- [docs/NON_ROBOT_SEARCH.md](docs/NON_ROBOT_SEARCH.md)

## Troubleshooting

- Container logs: `docker compose -f docker-compose-local.yml logs -f shadowcrawl`
- Health check: `curl -fsS http://localhost:5001/health`
- macOS preflight (especially for HITL): `docker compose -f docker-compose-local.yml exec -T shadowcrawl shadowcrawl --setup`
