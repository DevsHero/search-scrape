# ShadowCrawl on Windows (Desktop) — `non_robot_search` / HITL

This document covers running ShadowCrawl's **visible-browser HITL** mode on **Windows 10/11**.

## ✅ Current status

All previously-known Windows blockers have been resolved:

| Feature | Status | Details |
|---------|--------|---------|
| Process cleanup | ✅ Done | `force_kill_all_debug_browsers()`, `kill_debug_browser_zombies()`, `remove_stale_singleton_lock()` — all rewritten with `sysinfo` crate for cross-platform support |
| Browser discovery | ✅ Done | `find_chrome_executable()` now checks Brave, Chrome, and Edge at common Windows install paths |
| Notifications | ✅ Done | `notify-rust` builds and works on Windows (uses WinRT toast notifications) |
| Consent dialog | ✅ Done | `rfd` cross-platform dialog already handles Windows |
| Sound playback | ✅ Done | `rodio` already cross-platform |

## Windows requirements (Desktop HITL)

- **Windows 10/11** with an interactive desktop session (not Windows Server Core).
- **Brave or Chrome installed** (Edge also works as fallback).
- **Rust toolchain**:
  - Recommended: `stable-x86_64-pc-windows-msvc`.
  - Install Visual Studio Build Tools (C++ workload).
  - **Windows SDK ≥ 10.0.19041.0** (required for `ort`/ONNX Runtime DirectX libs — `DXCORE.lib`, `D3D12.lib`, etc.). Install via:
    ```powershell
    winget install Microsoft.WindowsSDK.10.0.22621
    ```
- **Administrator privileges** (recommended):
  - Global input hooks (kill switch / input locking) can be blocked by policy unless the process is elevated.
  - The built-in setup check warns when not elevated.

## Build and run (native, required for HITL)

1. Build the MCP stdio server with HITL:

```powershell
cd mcp-server
cargo build --release --bin shadowcrawl-mcp --features non_robot_search
```

2. Run preflight checks:

```powershell
.\target\release\shadowcrawl-mcp.exe --setup --json
```

3. Configure your MCP client (VS Code `mcp.json` example):

```json
{
  "servers": {
    "shadowcrawl-local": {
      "type": "stdio",
      "command": "C:\\path\\to\\shadowcrawl-mcp.exe",
      "args": [],
      "env": {
        "RUST_LOG": "info",
        "SHADOWCRAWL_NON_ROBOT_AUTO_ALLOW": "1",
        "SEARXNG_URL": "http://localhost:8890",
        "BROWSERLESS_URL": "http://localhost:3010",
        "QDRANT_URL": "http://localhost:6343"
      }
    }
  }
}
```

## Recommended environment variables for Windows

- `CHROME_EXECUTABLE` — override browser path (e.g., `C:\Program Files\BraveSoftware\Brave-Browser\Application\brave.exe`).
- `SHADOWCRAWL_RENDER_PROFILE_DIR` — a real profile directory to reuse cookies/sessions.
- `SHADOWCRAWL_NON_ROBOT_AUTO_ALLOW=1` — bypass consent prompts for local trusted runs.

## Technical implementation notes

### Cross-platform process management (`sysinfo`)

All process management functions now use the `sysinfo` crate (optional, gated behind `non_robot_search` feature with `default-features = false, features = ["system"]`):

- **`force_kill_all_debug_browsers(port)`** — enumerates all processes, matches `--remote-debugging-port=<port>` in command line, kills matches.
- **`kill_debug_browser_zombies(port, user_data_dir)`** — same as above but also matches `--user-data-dir=<path>`.
- **`remove_stale_singleton_lock(user_data_dir)`** — checks for stale `SingletonLock` files and removes them if no process is using the user-data-dir.

### Browser discovery priority

1. `CHROME_EXECUTABLE` env var (highest priority)
2. Windows candidate paths (in order):
   - `C:\Program Files\BraveSoftware\Brave-Browser\Application\brave.exe`
   - `C:\Program Files\Google\Chrome\Application\chrome.exe`
   - `C:\Program Files (x86)\Google\Chrome\Application\chrome.exe`
   - `C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe`
3. `chromiumoxide` default discovery (fallback)

## Troubleshooting

- **`DXCORE.lib` linker error** during `cargo build`:
  - Your Windows SDK is too old. Install SDK ≥ 10.0.19041.0.
  - `winget install Microsoft.WindowsSDK.10.0.22621`
- **Kill switch doesn't respond**:
  - Re-run elevated (Run Terminal / VS Code as Administrator).
  - Check whether endpoint security blocks global hooks.
- **Browser doesn't launch**:
  - Set `CHROME_EXECUTABLE` explicitly.
  - Ensure the executable path is correctly escaped in JSON configs.
- **Setup check shows `chrome_installed: fail`**:
  - This checks PATH only. The browser discovery function checks filesystem paths directly and will still find your browser.
