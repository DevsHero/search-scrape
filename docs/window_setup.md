# Cortex Scout — Windows Setup Guide

This guide covers building and configuring Cortex Scout on Windows 10/11.
For macOS/Linux see [docs/ubuntu_setup.md](ubuntu_setup.md) and the main [README](../README.md).

---

## Prerequisites

1. **Install Rust** from [rust-lang.org/tools/install](https://www.rust-lang.org/tools/install).
   Choose the default MSVC toolchain during setup.

2. **Install Visual Studio Build Tools 2022** with the
   "Desktop development with C++" workload
   (required for the Rust MSVC linker).

3. **Install a Chromium-based browser** (Brave, Chrome, or Edge) if you plan to use
   CDP-based anti-bot bypass (`SEARCH_CDP_FALLBACK=true`) or the HITL visible-browser
  tools (`hitl_web_fetch`).

---

## Build

Open PowerShell and run:

```powershell
git clone https://github.com/cortex-works/cortex-scout.git
cd cortex-scout\mcp-server

# Basic build (search, scrape, deep research, memory)
cargo build --release --bin cortex-scout-mcp

# Full build (adds hitl_web_fetch and other optional HITL features)
cargo build --release --all-features --bin cortex-scout-mcp
```

The output binary is at:

```
cortex-scout\mcp-server\target\release\cortex-scout-mcp.exe
```

---

## Configure VS Code

Windows has no `env` command, so pass environment variables as an object.

Open `%APPDATA%\Code\User\mcp.json` (create if it does not exist):

```jsonc
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

Restart VS Code after saving.

---

## Configure other clients (Claude Desktop, Cursor, Windsurf)

These clients use the `"mcpServers"` top-level key (not `"servers"`).

```jsonc
// claude_desktop_config.json / ~/.cursor/mcp.json / etc.
{
  "mcpServers": {
    "cortex-scout": {
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

---

## Feature support on Windows

| Feature | Status |
|---------|--------|
| Web search (all engines) | Supported |
| Web fetch / crawl | Supported |
| Deep research + LLM synthesis | Supported |
| Proxy rotation | Supported |
| CDP anti-bot (`SEARCH_CDP_FALLBACK`) | Supported (requires Brave/Chrome installed) |
| `hitl_web_fetch` (`auth_mode=challenge|auth`) | Supported — build with `--all-features` |
| `visual_scout` (headless screenshot) | Supported |
| Semantic memory (LanceDB) | Supported |

---

## Troubleshooting

| Symptom | Fix |
|---------|-----|
| Build fails with linker errors | Verify VS Build Tools 2022 + C++ workload installed |
| No tools appear in VS Code | Check binary path in mcp.json; restart VS Code |
| Tools time out immediately | Set `RUST_LOG=warn` (not `info`) |
| CDP fetch fails | Ensure Brave/Chrome is installed and discoverable |
| `hitl_web_fetch` not listed | Rebuild with `--all-features` |

Legacy compatibility alias note: `human_auth_session` remains callable but maps to `hitl_web_fetch(auth_mode="auth")`.
