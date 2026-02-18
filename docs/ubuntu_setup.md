# ShadowCrawl on Ubuntu Desktop — `non_robot_search` / HITL

This document focuses on running ShadowCrawl’s **visible-browser HITL** mode (`non_robot_search`) on **Ubuntu Desktop** (GNOME/KDE).

## What works today (current repo state)

- Linux preflight checks already exist in [mcp-server/src/setup/os/linux.rs](../mcp-server/src/setup/os/linux.rs):
  - Detects X11/Wayland display env.
  - Checks `/dev/input/event*` readability for global input hooks.
- Browser discovery includes common Linux paths via `scraping::browser_manager::find_chrome_executable()`.

## Ubuntu Desktop requirements

### Desktop session

- You must run in a real desktop session.
- X11 is usually the most permissive for global input hooks.
- Wayland can restrict global input capture depending on compositor/security policy.

### Browser

- Install one of:
  - Brave, Google Chrome, or Chromium.

### Build tooling (typical)

These are common packages needed to compile + run desktop integrations:

```bash
sudo apt-get update
sudo apt-get install -y \
  build-essential pkg-config \
  libssl-dev \
  libasound2-dev \
  libx11-dev libxi-dev libxtst-dev \
  libxkbcommon-dev \
  libwayland-dev
```

Notes:
- `rodio` often needs ALSA headers (`libasound2-dev`).
- Input and GUI stacks vary by distro; add packages as compiler errors indicate.

### Desktop notifications

`notify-rust` requires a functional desktop notification stack (DBus + a notification daemon).
If unavailable, HITL still works; you just may not see toast notifications.

## Permissions: `/dev/input` access (kill switch / input locking)

`non_robot_search` relies on global input hooks. On Linux, a common failure mode is lack of read access to `/dev/input/event*`.

Recommended fixes:

- Add your user to the input group (distro dependent):

```bash
sudo usermod -aG input "$USER"
```

- Log out and back in (or reboot).

If you’re on Wayland and hooks still fail:
- Try an X11 session (login screen: gear icon → “Ubuntu on Xorg”).

## Build and run (native, required for HITL)

```bash
cd mcp-server
cargo build --release --bin shadowcrawl-mcp --features non_robot_search
```

## Recommended environment variables for Ubuntu

- `CHROME_EXECUTABLE` — set if auto-discovery fails (e.g. `/usr/bin/google-chrome`).
- `SHADOWCRAWL_RENDER_PROFILE_DIR` — profile directory for persistent cookies/sessions.
- `SHADOWCRAWL_NON_ROBOT_AUTO_ALLOW=1` — bypass consent prompts for local trusted runs.

## Known Ubuntu/Linux blockers (to reach “full support”)

1. **Wayland restrictions**
   - Global input hooks may be restricted.
   - Recommended stance: officially support Ubuntu Desktop with X11 as the “best-effort” mode.

2. **Desktop notifications may be unavailable**
   - `notify-rust` requires a functional desktop notification stack (DBus + notification daemon).
   - The feature should degrade gracefully (no-op if unavailable).

3. **Browser discovery**
  - If auto-discovery fails, set `CHROME_EXECUTABLE` explicitly (e.g., `/usr/bin/google-chrome`).

## Engineering plan (Ubuntu Desktop)

### Phase 1 — Confirm build deps and document them

- Run `cargo build --release --features non_robot_search` on:
  - Ubuntu 22.04 LTS
  - Ubuntu 24.04 LTS
- Capture missing library errors and expand the apt list accordingly.

### Phase 2 — Stabilize input hooks across X11/Wayland

- Keep current checks in [mcp-server/src/setup/os/linux.rs](../mcp-server/src/setup/os/linux.rs).
- Document “X11 recommended” if Wayland blocks hooks.
- Ensure kill-switch remains best-effort (never hard-fail if hooks unavailable).

### Phase 3 — Unify browser discovery

- Use a single helper for browser discovery used by both:
  - `non_robot_search` (visible browser), and
  - CDP-based scraping paths.

### Phase 4 — CI verification

- Add a Linux desktop build job (compile-only) for `--features non_robot_search`.
- Optionally run a smoke test against a local `about:blank` navigation.

## Troubleshooting

- No GUI / dialog doesn’t appear:
  - Confirm `DISPLAY` or `WAYLAND_DISPLAY` is set.
  - Install portal packages: `xdg-desktop-portal` + backend.
- Kill switch doesn’t work:
  - Ensure `/dev/input/event*` is readable.
  - Switch to X11 session if on Wayland.
