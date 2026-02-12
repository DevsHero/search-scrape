# Proxy Manager (ip.txt-first)

**Date:** 2026-02-12

This repo uses **`ip.txt` as the primary proxy list** and exposes a single MCP tool: **`proxy_manager`**.

The older `proxies.yaml` registry approach is deprecated/removed.

## Files

- `ip.txt`
  - One proxy per line.
  - Supports `http://`, `https://`, and `socks5://`.
  - Used by the runtime via `IP_LIST_PATH`.

- `proxy_source.json`
  - Public proxy list sources.
  - Used by `proxy_manager` action `grab` (via `PROXY_SOURCE_PATH`).

## Docker (docker-compose-local.yml)

The container mounts:

- `./ip.txt:/home/appuser/ip.txt`
- `./proxy_source.json:/home/appuser/proxy_source.json`

And sets:

- `IP_LIST_PATH=/home/appuser/ip.txt`
- `PROXY_SOURCE_PATH=/home/appuser/proxy_source.json`
- `IP_LIST_DEFAULT_SCHEME=auto`

## MCP tool

Tool: `proxy_manager`

Actions:

- `grab` — fetch proxies from `proxy_source.json` and optionally write into `ip.txt`
- `list` — list proxies currently in `ip.txt`
- `status` — show proxy manager status (requires `ip.txt` available)
- `switch` — select best proxy from the registry built from `ip.txt`
- `test` — test one proxy against a target URL

## Code pointers

- Runtime proxy manager: `mcp-server/src/features/proxy_manager.rs`
- Proxy grabber: `mcp-server/src/features/proxy_grabber.rs`
- MCP handler: `mcp-server/src/mcp/handlers/proxy_manager.rs`

