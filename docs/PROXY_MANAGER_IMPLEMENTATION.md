# Proxy Manager (ip.txt-first)

**Date:** 2026-02-12

This repo uses **`ip.txt` as the primary proxy list** and exposes the MCP tool: **`proxy_control`**.

The older `proxies.yaml` registry approach is deprecated/removed.

## Files

- `ip.txt`
  - One proxy per line.
  - Supports `http://`, `https://`, and `socks5://`.
  - Used by the runtime via `IP_LIST_PATH`.

- `proxy_source.json`
  - Public proxy list sources.
  - Used by `proxy_control` action `grab` (via `PROXY_SOURCE_PATH`).

## Runtime config (Zero-Docker)

Set env vars for the local binary:

- `IP_LIST_PATH=/absolute/path/to/ip.txt`
- `PROXY_SOURCE_PATH=/absolute/path/to/proxy_source.json`

Example:

```bash
IP_LIST_PATH="$PWD/ip.txt" \
PROXY_SOURCE_PATH="$PWD/proxy_source.json" \
./mcp-server/target/release/cortex-scout-mcp
```

## MCP tool

Tool: `proxy_control`

Actions:

- `grab` — fetch proxies from `proxy_source.json` and optionally write into `ip.txt`
- `list` — list proxies currently in `ip.txt`
- `status` — show proxy manager status (requires `ip.txt` available)
- `switch` — select best proxy from the registry built from `ip.txt`
- `test` — test one proxy against a target URL

## Code pointers

- Runtime proxy manager: `mcp-server/src/features/proxy_manager.rs`
- Proxy grabber: `mcp-server/src/features/proxy_grabber.rs`
- MCP handler (internal routing): `mcp-server/src/mcp/handlers/proxy_manager.rs`

