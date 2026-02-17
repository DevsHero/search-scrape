# Non-Robot Search (HITL / High-Fidelity Renderer) — v2.0.0-rc

`non_robot_search` (internal feature/handler name: `non_robot_search`) is the “nuclear option” tool for targets that are:

- heavily JavaScript-driven (content appears only after client-side hydration)
- protected by anti-bot / verification gates (captcha, interstitials)
- best handled with a real, visible browser + human assistance

This tool is intentionally interactive (HITL = Human-In-The-Loop). It is **not** a normal headless scraper.

## Support / OS

- ✅ **Tested:** macOS (this release)
- ✅ **Tested:** Windows 10/11 (see verified setup guide below)
- ⚠️ **Not the primary test target:** Linux (may work, but expect rough edges)
  - Ubuntu Desktop notes: docs/ubuntu_setup.md
  - Windows notes: docs/WINDOWS_SETUP.md
- ⚠️ **Container note:** `non_robot_search` launches a **local GUI browser** (Brave/Chrome). Running it inside the Docker container is not supported for typical setups.

If you only run Docker-based MCP, you still get the other tools (`web_search`, `scrape_url`, `crawl_website`, …). Use a native desktop for HITL.

## What it does (at a high level)

1. Requests explicit user consent (TTY prompt or macOS dialog)
2. Launches a visible Chromium-based browser via CDP (Brave preferred)
3. Navigates to your URL
4. Runs “janitor” cleanup to remove overlays / scroll-lock / popups
5. If a verification gate is detected, it asks you to complete it
6. Extracts HTML → cleaned markdown/text/JSON (same output model as other scrapers)

## Safety / Interactivity model (must read)

- Consent is required unless `SHADOWCRAWL_NON_ROBOT_AUTO_ALLOW=1` is set.
- Emergency abort: **hold `Esc` for ~3 seconds**.
- A global key listener is used for the kill-switch.
  - On macOS this may require **Accessibility permission**.
- Input locking may be requested by the tool, but the current implementation is best-effort (and may be a noop depending on platform). Still: don’t fight the browser while it’s working.

## Build requirements

`non_robot_search` is behind a Cargo feature flag (`non_robot_search`).

Build the binaries (native macOS):

```bash
cd mcp-server
cargo build --release --features non_robot_search --bin shadowcrawl --bin shadowcrawl-mcp
```

## Run recommendations (macOS)

### Option A (recommended): run Docker stack for dependencies + run HITL tool natively

1) Start the stack (SearXNG / Qdrant / Browserless):

```bash
docker compose -f docker-compose-local.yml up -d --build
```

2) Run the MCP stdio server locally (so it can open Brave/Chrome):

```bash
cd mcp-server
SEARXNG_URL=http://localhost:8890 \
QDRANT_URL=http://localhost:6344 \
RUST_LOG=info \
./target/release/shadowcrawl-mcp
```

Notes:
- Host ports in `docker-compose-local.yml` are `8890` (SearXNG) and `6344` (Qdrant gRPC).
- `non_robot_search` itself does not require Browserless.

### Option B: run HTTP server locally

```bash
cd mcp-server
SEARXNG_URL=http://localhost:8890 RUST_LOG=info ./target/release/shadowcrawl
```

Then call `POST /mcp/call` with `tool=non_robot_search`.

## Brave / Chrome configuration

### Browser executable selection

The tool resolves the browser executable in this order:

1) `CHROME_EXECUTABLE` (explicit override)
2) macOS common install paths (prefers Brave):
   - `/Applications/Brave Browser.app/Contents/MacOS/Brave Browser`
   - `/Applications/Google Chrome.app/Contents/MacOS/Google Chrome`
   - `/Applications/Chromium.app/Contents/MacOS/Chromium`

If you want to force Brave:

```bash
export CHROME_EXECUTABLE="/Applications/Brave Browser.app/Contents/MacOS/Brave Browser"
```

### Persisted login session (profiles)

To use your existing logged-in session (e.g. LinkedIn), you must use a real browser profile.

You can pass it either as:

- tool argument: `user_profile_path`
- env var: `SHADOWCRAWL_RENDER_PROFILE_DIR`

Common macOS profile locations:

- Brave:
  - Base dir: `~/Library/Application Support/BraveSoftware/Brave-Browser/`
  - Default profile: `~/Library/Application Support/BraveSoftware/Brave-Browser/Default`
  - Profile 1: `~/Library/Application Support/BraveSoftware/Brave-Browser/Profile 1`
- Chrome:
  - Base dir: `~/Library/Application Support/Google/Chrome/`
  - Default profile: `~/Library/Application Support/Google/Chrome/Default`

Important:
- If you point at `.../Default` (or `.../Profile 1`), ShadowCrawl automatically maps it to:
  - `--user-data-dir=<parent>` and `--profile-directory=<basename>`
- **Do not run two calls concurrently** with the same profile directory. (Profile locks like `SingletonLock` are a real thing.)

## Consent controls

- `SHADOWCRAWL_NON_ROBOT_AUTO_ALLOW=1` skips consent prompts (use carefully).
- `SHADOWCRAWL_NON_ROBOT_CONSENT=tty` forces terminal Enter/Esc flow.
- `SHADOWCRAWL_NON_ROBOT_CONSENT=dialog` forces GUI dialog.

If your MCP client runs without a TTY, ShadowCrawl will use a dialog by default.

## Tool arguments (MCP)

`non_robot_search` accepts:

- `url` (required)
- `output_format`: `json` (default) or `text`
- `max_chars`: default `10000`
- `quality_mode`: `balanced` | `aggressive` | `high`
- `human_timeout_seconds`: how long to wait for you during HITL (default 60)
- `captcha_grace_seconds`: initial grace period (default 5)
- `user_profile_path`: see above
- `auto_scroll`: enable lazy-load scrolling
- `wait_for_selector`: wait for a selector before extraction

Example MCP call payload (HTTP transport):

```json
{
  "tool": "non_robot_search",
  "arguments": {
    "url": "https://www.linkedin.com/jobs/view/1234567890",
    "quality_mode": "high",
    "auto_scroll": true,
    "wait_for_selector": "main",
    "human_timeout_seconds": 120,
    "user_profile_path": "~/Library/Application Support/BraveSoftware/Brave-Browser/Default"
  }
}
```

## Troubleshooting

### “Browser executable not found (tried Brave, Chrome, Chromium)”

- Install Brave (recommended) or Chrome
- Or set `CHROME_EXECUTABLE` explicitly

### CDP connect fails / keeps retrying

- Ensure no other debug instances are occupying the port
- Close Brave/Chrome and retry
- Avoid running multiple HITL calls at once

### macOS permission problems (kill-switch / global keys)

Run the guided preflight:

```bash
cd mcp-server
./target/release/shadowcrawl --setup
```

Then enable:
- System Settings → Privacy & Security → Accessibility
- Add/enable the app that launches ShadowCrawl (Terminal / VS Code)

### Profile lock issues (SingletonLock)

- Close Brave/Chrome fully
- Run HITL calls sequentially
- If you use a live profile, do not keep a second Brave instance open on the same profile
