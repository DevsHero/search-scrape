# MCP Client Setup (IDE / Apps)

This repo provides an MCP server over **stdio** (recommended) via the `shadowcrawl-mcp` binary.

Most clients support a config that looks like:

- `command`: executable to run
- `args`: arguments
- optional env vars

This doc focuses on what differs from VS Code.

## Prereq

- Docker installed
- Run the stack:

```bash
docker compose -f docker-compose-local.yml up -d --build
```

The stdio server is executed inside the running container:

```bash
docker compose -f docker-compose-local.yml exec -i -T shadowcrawl shadowcrawl-mcp
```

Optional (enable `non_robot_search` / HITL in the container build):

```bash
SHADOWCRAWL_CARGO_FEATURES=non_robot_search \
  docker compose -f docker-compose-local.yml up -d --build
```

Important: `non_robot_search` launches a **local GUI browser** (Brave/Chrome) and is macOS-tested.
Even if you compile it into the container image, typical Docker deployments won’t be able to open the host browser.
For HITL usage, run the MCP server **natively on macOS** and connect it to the Docker stack for SearXNG/Qdrant.

Guide:
- [docs/NON_ROBOT_SEARCH.md](docs/NON_ROBOT_SEARCH.md)

---

## Claude Desktop (macOS)

Claude Desktop uses `claude_desktop_config.json`.

- Open: Claude Desktop → Settings → Developer → Edit Config
- File path (macOS): `~/Library/Application Support/Claude/claude_desktop_config.json`

Example:

```json
{
  "mcpServers": {
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
version: 2.0.0-rc
schema: v1

mcpServers:
  - name: shadowcrawl
    command: docker
    args:
      - compose
      - -f
      - /absolute/path/to/search-scrape/docker-compose-local.yml
      - exec
      - -i
      - -T
      - shadowcrawl
      - shadowcrawl-mcp
```

Notes:
- MCP can only be used in **agent** mode.
- Continue’s MCP server configs are per-workspace.
- If you already have a JSON MCP config (Claude/Cursor/etc), you can drop it into `.continue/mcpServers/` (for example `.continue/mcpServers/mcp.json`) and Continue will pick it up.

---

## Troubleshooting

- If the client shows "no tools found", confirm the container is running and `shadowcrawl-mcp` exists.
- If you use proxies, ensure `ip.txt` is mounted and `IP_LIST_PATH` points to it.
