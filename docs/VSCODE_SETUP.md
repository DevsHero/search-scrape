# VS Code MCP Setup (macOS-tested)

This repo exposes an MCP server over **stdio** via the `shadowcrawl-mcp` binary.

Recommended setup: run via Docker Compose so SearXNG/Qdrant/Browserless are already wired.

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

## Troubleshooting

- Container logs: `docker compose -f docker-compose-local.yml logs -f shadowcrawl`
- Health check: `curl -fsS http://localhost:5001/health`
- macOS preflight (especially for HITL): `docker compose -f docker-compose-local.yml exec -T shadowcrawl shadowcrawl --setup`
