# ShadowCrawl Windows Installation Guide (Complete Set)

This guide provides a comprehensive, step-by-step procedure to set up **ShadowCrawl** on Windows 10/11, enabling full **Human-in-the-Loop (HITL)** capabilities.

---

## 🏗️ 1. Prerequisites (Do this first)

1.  **Install Rust:**
    Download and install `rustup-init.exe` from [rust-lang.org](https://www.rust-lang.org/tools/install). Select the default installation (Microsoft C++ build tools).

2.  **Install Build Tools & Windows SDK:**
    ShadowCrawl uses modern AI libraries (ONNX Runtime) that require specific DirectX libraries.
    - Install **Visual Studio Build Tools 2022**.
    - Ensure **"Desktop development with C++"** workload is checked.
    - **CRITICAL:** Ensure a recent Windows 10/11 SDK is installed (version **10.0.19041.0** or higher is REQUIRED for `DXCORE.lib`).
    
    *If you get linker errors involving `DXCORE.lib`, run:*
    ```powershell
    winget install Microsoft.WindowsSDK.10.0.22621
    ```

3.  **Install Docker Desktop:**
  Required for the support services (Browserless).
    - [Download Docker Desktop for Windows](https://www.docker.com/products/docker-desktop/)

4.  **Install a Browser (Brave Recommended):**
  - [Brave Browser](https://brave.com/) (Best for `non_robot_search` / HITL)
    - OR Google Chrome / Microsoft Edge.

---

## 🛠️ 2. Build the Project

Open PowerShell (Run as Administrator recommended for best experience with global hooks).

```powershell
# 1. Clone the repository
git clone https://github.com/DevsHero/ShadowCrawl.git
cd ShadowCrawl

# 2. Enter the server directory
cd mcp-server

# 3. Build with Windows HITL support
# This may take 5-10 minutes initially to compile dependencies like `ort` and `sysinfo`.
cargo build --release --bin shadowcrawl-mcp --features non_robot_search
```

**Verify the build:**
```powershell
.\target\release\shadowcrawl-mcp.exe --version
# Should output: 2.0.0-rc (or similar)
```

---

## 🐳 3. Start Support Services

Go back to the project root and start the Docker stack.

```powershell
cd ..  # Back to ShadowCrawl root
docker compose -f docker-compose-local.yml up -d
```

**Verify services are running:**
- **Browserless**: http://localhost:3010

---

## ⚙️ 4. Configure MCP in VS Code (The "Windows Set")

This configuration connects VS Code to your local ShadowCrawl binary.

1.  Open VS Code.
2.  Open your MCP settings file:
    - **Method A:** Press `F1` or `Ctrl+Shift+P` -> type `MCP: Configure MCP Servers` -> select `Open configuration file`.
    - **Method B:** Manually open `%APPDATA%\Code\User\mcp.json`.
3.  Add the following implementation. **Update paths to match your actual folders.**

```json
{
  "servers": {
    "shadowcrawl": {
      "type": "stdio",
      "command": "c:\\Users\\YOUR_USER\\Downloads\\ShadowCrawl\\mcp-server\\target\\release\\shadowcrawl-mcp.exe",
      "args": [],
      "env": {
        "RUST_LOG": "info",
        "BROWSERLESS_URL": "http://localhost:3010",
        "BROWSERLESS_TOKEN": "mcp_stealth_session",
        "LANCEDB_URI": "c:\\Users\\YOUR_USER\\Downloads\\ShadowCrawl\\lancedb",
        "HTTP_TIMEOUT_SECS": "30",
        "HTTP_CONNECT_TIMEOUT_SECS": "10",
        "OUTBOUND_LIMIT": "32",
        "MAX_CONTENT_CHARS": "10000",
        "MAX_LINKS": "100",
        "SHADOWCRAWL_NON_ROBOT_AUTO_ALLOW": "1",
        "IP_LIST_PATH": "c:\\Users\\YOUR_USER\\Downloads\\ShadowCrawl\\ip.txt",
        "PROXY_SOURCE_PATH": "c:\\Users\\YOUR_USER\\Downloads\\ShadowCrawl\\proxy_source.json"
      }
    }
  }
}
```

> **Note:** `SHADOWCRAWL_NON_ROBOT_AUTO_ALLOW="1"` enables "Agent Mode" where the browser opens automatically without a confirmation popup for every action. Set to `0` if you want manual approval for every browser launch.

---

## 🧪 5. Verification

1.  **Restart VS Code** to reload the MCP configuration.
2.  Open the MCP servers view (icon in sidebar) to confirm `shadowcrawl` is connected (green dot).
3.  Open a Chat in VS Code (e.g., using GitHub Copilot or an MCP-enabled chat agent).
4.  Ask: *"Search the web for 'Rust programming 2026' using ShadowCrawl"*
  - **Result:** Should return search results from the built-in Rust metasearch.
5.  Ask: *"Go to https://example.com using non_robot_search and extract the content"*
    - **Result:** 
      - A browser window (Brave/Chrome) should visibly open on your desktop.
      - It will navigate to example.com.
      - It will hold for a few seconds (simulating human behavior).
      - It will close automatically (or wait for timeout/finish).
      - The agent should receive the extracted text.

---

## ❓ Troubleshooting

- **Build error `cannot find -lDXCORE`**:
  - You are missing the Windows 10 SDK (version 2004 / 10.0.19041.0 or newer). Run the `winget` command in Prerequisites.
- **Browser path not found**:
  - Add `"CHROME_EXECUTABLE": "C:\\Path\\To\\Your\\Browser.exe"` to the `env` section in `mcp.json`.
- **Proxy errors**:
  - Create an empty `ip.txt` and `proxy_source.json` in your ShadowCrawl root folder if you are not using proxies.
- **"Access Denied" on kill switch**:
  - VS Code or the terminal launching the process needs to be run as **Administrator** to use global input hooks required for the safety kill switch.

---

## 📜 Feature & Config Reference (Windows)

| Feature | Windows Status | Implementation Details |
|---------|----------------|------------------------|
| **Browser Control** | ✅ Working | Uses `sysinfo` to manage processes (replaces Unix `ps`/`kill`). |
| **Notifications** | ✅ Working | Uses `notify-rust` (native Windows 10/11 Toasts). |
| **Local Proxy** | ✅ Working | Reads native Windows paths from JSON config. |
| **Consent Dialog** | ✅ Working | Uses `rfd` crate for native message boxes. |
