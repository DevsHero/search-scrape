# ShadowCrawl on Windows (Desktop) — `non_robot_search` / HITL

This document focuses on running ShadowCrawl’s **visible-browser HITL** mode on **Windows 10/11**.

## What works today (current repo state)

- The project already has a Windows setup check at [mcp-server/src/setup/os/windows.rs](../mcp-server/src/setup/os/windows.rs).
- The `non_robot_search` flow already uses a cross-platform dialog fallback (`rfd`) for non-macOS consent prompts in [mcp-server/src/features/non_robot_search.rs](../mcp-server/src/features/non_robot_search.rs).

## Known Windows blockers (must address for “full support”)

1. **Force-kill browser cleanup is Unix-only**
   - Current implementation scans processes via `ps` and kills via `kill -9` in `force_kill_all_debug_browsers()`.
   - On Windows this must be replaced with either:
     - A Windows-specific implementation (`tasklist` + `taskkill`), or
     - A cross-platform process library (recommended: `sysinfo`) that can match processes by command line and terminate by PID.

2. **Notification dependency may not be Windows-safe**
   - The `non_robot_search` feature enables `notify-rust` unconditionally via Cargo features.
   - If `notify-rust` does not build/run on Windows in your environment, the fix is to introduce a small `Notifier` abstraction and use:
     - Windows: a Windows toast notification crate (`windows` / WinRT), or
     - Windows: no-op notifier (safe default).

3. **Browser executable discovery is incomplete**
   - `find_chrome_executable()` intentionally defers on Windows. This can work if Chromium discovery succeeds, but it’s brittle.
   - “Full support” should add common Windows install paths for Brave/Chrome/Edge, plus allow overriding with `CHROME_EXECUTABLE`.

## Windows requirements (Desktop HITL)

- **Windows 10/11** with an interactive desktop session (not Windows Server Core).
- **Brave or Chrome installed**.
- **Rust toolchain**:
  - Recommended: `stable-x86_64-pc-windows-msvc`.
  - Install Visual Studio Build Tools (C++ workload) if any native deps require MSVC.
- **Administrator privileges** (recommended):
  - Global input hooks (kill switch / input locking) can be blocked by policy unless the process is elevated.
  - The built-in setup check warns when not elevated.

## Build and run (native, required for HITL)

1. Build the MCP stdio server with HITL:

```powershell
cd mcp-server
cargo build --release --bin shadowcrawl-mcp --features non_robot_search
```

2. Configure your MCP client to run the native binary (example pattern):

- Prefer using `env`-style args (or set env vars in your MCP client).
- Required override if discovery fails:
  - `CHROME_EXECUTABLE=C:\\Program Files\\BraveSoftware\\Brave-Browser\\Application\\brave.exe`

## Recommended environment variables for Windows

- `CHROME_EXECUTABLE` — full path to `brave.exe` / `chrome.exe`.
- `SHADOWCRAWL_RENDER_PROFILE_DIR` — a real profile directory to reuse cookies/sessions.
- `SHADOWCRAWL_NON_ROBOT_AUTO_ALLOW=1` — bypass consent prompts for local trusted runs.

## Engineering plan (Windows)

### Phase 1 — Make it compile with `--features non_robot_search`

- Confirm whether `notify-rust` is compatible on Windows.
  - If not: move notifications behind a platform abstraction or compile-time `cfg`.

### Phase 2 — Make cleanup reliable (no zombie browsers)

- Replace `force_kill_all_debug_browsers()` with a cross-platform implementation:
  - Add `sysinfo` crate.
  - Enumerate processes and match `--remote-debugging-port=<port>` in the cmdline.
  - Terminate matching PIDs (and optionally their children).

### Phase 3 — Make browser discovery deterministic

- Expand `find_chrome_executable()` Windows branch with common install paths:
  - `C:\\Program Files\\BraveSoftware\\Brave-Browser\\Application\\brave.exe`
  - `C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe`
  - `C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe`
  - `C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe`
- Keep `CHROME_EXECUTABLE` override as top priority.

### Phase 4 — Validate HITL ergonomics

- Verify:
  - Consent dialog works in MCP stdio environment.
  - Manual Return Button works.
  - Emergency “hold ESC” kill switch works reliably under UAC/AV restrictions.

## Troubleshooting

- If the kill switch doesn’t respond:
  - Re-run elevated (Run Terminal / VS Code as Administrator).
  - Check whether endpoint security blocks global hooks.
- If the browser doesn’t launch:
  - Set `CHROME_EXECUTABLE` explicitly.
  - Ensure the executable path is correctly escaped in JSON configs.
