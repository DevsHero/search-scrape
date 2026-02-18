//! Native browser management using `chromiumoxide`.
//!
//! This module is the **single source of truth** for:
//! * Finding a usable browser executable (Brave → Chrome → Chromium, cross-platform).
//! * Launching a headless browser session via chromiumoxide.
//! * Providing a lightweight "fetch HTML" primitive for SERP and other
//!   cases that need JS rendering but not full stealth simulation.
//!
//! All other modules (cdp.rs, search engines, scrape tool, etc.) use this
//! module.  The external Browserless Docker service is no longer required.

use anyhow::{anyhow, Result};
use chromiumoxide::browser::BrowserConfig;
use chromiumoxide::handler::viewport::Viewport;
use chromiumoxide::Browser;
use futures::StreamExt;
use std::path::Path;
use std::time::Duration;
use tracing::{error, info, warn};

// ── Browser executable discovery ─────────────────────────────────────────────

/// Find a usable Chromium-family browser executable.
///
/// Resolution order:
/// 1. `CHROME_EXECUTABLE` env var (works great in Docker:
///    `CHROME_EXECUTABLE=/usr/bin/chromium`)
/// 2. PATH scan – finds package-manager installs on all platforms.
/// 3. OS-specific well-known install paths.
pub fn find_chrome_executable() -> Option<String> {
    // 1. Explicit env override
    if let Ok(p) = std::env::var("CHROME_EXECUTABLE") {
        if Path::new(&p).exists() {
            return Some(p);
        }
    }

    // 2. PATH scan (Linux / macOS / Windows package managers)
    if let Ok(path_var) = std::env::var("PATH") {
        let candidates = [
            "brave-browser",
            "brave",
            "google-chrome",
            "chromium",
            "chromium-browser",
            "chrome",
        ];
        for dir in std::env::split_paths(&path_var) {
            for exe in candidates {
                let full = dir.join(exe);
                if full.exists() {
                    return Some(full.to_string_lossy().to_string());
                }
            }
        }
    }

    // 3. Platform-specific well-known paths
    #[cfg(target_os = "macos")]
    {
        // Prioritise Brave for better fingerprinting (non_robot_search)
        let candidates = [
            "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Chromium.app/Contents/MacOS/Chromium",
            "/Applications/Google Chrome Canary.app/Contents/MacOS/Google Chrome Canary",
        ];
        for c in candidates {
            if Path::new(c).exists() {
                return Some(c.to_string());
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        let candidates = [
            "/usr/bin/brave-browser",
            "/usr/bin/brave",
            "/usr/bin/chromium",
            "/usr/bin/chromium-browser",
            "/usr/bin/google-chrome",
            "/usr/local/bin/chromium",
        ];
        for c in candidates {
            if Path::new(c).exists() {
                return Some(c.to_string());
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        let candidates = [
            r"C:\Program Files\BraveSoftware\Brave-Browser\Application\brave.exe",
            r"C:\Program Files\Google\Chrome\Application\chrome.exe",
            r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
            r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        ];
        for c in candidates {
            if Path::new(c).exists() {
                return Some(c.to_string());
            }
        }
    }

    None
}

/// Returns `true` when a usable browser binary is present on this machine.
/// Use this anywhere you previously checked `BROWSERLESS_URL` as a proxy for
/// "can we do JS rendering?".
pub fn native_browser_available() -> bool {
    find_chrome_executable().is_some()
}

// ── Headless browser config builder ──────────────────────────────────────────

/// Build a `BrowserConfig` for headless operation.
///
/// Flags chosen for:
/// * Compatibility inside Docker (no GPU, no sandbox required when running
///   as a non-root user with `--no-sandbox`).
/// * Stealth – disable telemetry, crash reports, background networking.
/// * Stability – shared-memory size hints for container environments.
pub fn build_headless_config(
    exe: &str,
    proxy_url: Option<&str>,
    width: u32,
    height: u32,
) -> Result<BrowserConfig> {
    let mut builder = BrowserConfig::builder()
        .chrome_executable(exe)
        .viewport(Viewport {
            width,
            height,
            device_scale_factor: Some(1.0),
            emulating_mobile: false,
            is_landscape: true,
            has_touch: false,
        })
        .window_size(width, height)
        // Headless flags compatible with both Chrome/Chromium and Brave
        .arg("--disable-gpu")
        .arg("--no-sandbox") // required in Docker / CI environments
        .arg("--disable-setuid-sandbox")
        .arg("--disable-dev-shm-usage") // avoids /dev/shm OOM in Docker
        .arg("--disable-extensions")
        .arg("--disable-background-networking")
        .arg("--disable-sync")
        .arg("--disable-translate")
        .arg("--disable-crash-reporter")
        .arg("--disable-breakpad")
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg("--hide-scrollbars")
        .arg("--mute-audio");

    if let Some(proxy) = proxy_url {
        builder = builder.arg(format!("--proxy-server={}", proxy));
    }

    builder
        .build()
        .map_err(|e| anyhow!("Failed to build browser config: {}", e))
}

// ── Lightweight "fetch HTML" primitive ───────────────────────────────────────

/// Fetch the rendered HTML of `url` using a **native headless browser**.
///
/// This is the lightweight alternative to the old Browserless `/content` HTTP
/// call.  It launches a fresh browser, navigates, waits, captures HTML, then
/// closes.  The full stealth-scraping pipeline (mouse simulation, human scroll,
/// etc.) lives in `cdp.rs`; this function is intentionally minimal and fast.
///
/// `wait_ms` — milliseconds to wait after navigation before capturing HTML.
/// Defaults to 2 000 ms.
///
/// Returns `(status_code, html)` — status is always 200 on success.
pub async fn fetch_html_native(url: &str, wait_ms: Option<u32>) -> Result<(u16, String)> {
    let exe = find_chrome_executable()
        .ok_or_else(|| anyhow!("No browser found. Install Brave, Chrome, or Chromium. Set CHROME_EXECUTABLE if installed in a non-standard location."))?;

    info!("🌐 Native headless fetch: {} (browser: {})", url, exe);

    let wait_time = wait_ms.unwrap_or(2000) as u64;

    let config = build_headless_config(&exe, None, 1280, 900)?;

    let (mut browser, mut handler) = Browser::launch(config)
        .await
        .map_err(|e| anyhow!("Failed to launch browser ({}): {}", exe, e))?;

    let _handle = tokio::spawn(async move {
        while let Some(event) = handler.next().await {
            if let Err(e) = event {
                error!("CDP handler error: {}", e);
            }
        }
    });

    let result: Result<(u16, String)> = async {
        let page = browser
            .new_page(url)
            .await
            .map_err(|e| anyhow!("Failed to open page: {}", e))?;

        tokio::time::sleep(Duration::from_millis(wait_time)).await;

        let html = page
            .content()
            .await
            .map_err(|e| anyhow!("Failed to get page content: {}", e))?;

        info!(
            "✅ Native fetch succeeded: {} chars ({}ms wait)",
            html.len(),
            wait_time
        );

        Ok((200u16, html))
    }
    .await;

    // Best-effort cleanup — don't let a close error shadow the fetch error
    if let Err(e) = browser.close().await {
        warn!("Browser close error (non-fatal): {}", e);
    }

    result
}

/// Like `fetch_html_native` but with a Mobile Safari profile, useful as a
/// fallback when the desktop UA is blocked.
pub async fn fetch_html_native_mobile(url: &str, wait_ms: Option<u32>) -> Result<(u16, String)> {
    let exe = find_chrome_executable()
        .ok_or_else(|| anyhow!("No browser found for mobile fetch fallback"))?;

    let wait_time = wait_ms.unwrap_or(2500) as u64;

    let config = BrowserConfig::builder()
        .chrome_executable(&exe)
        .viewport(Viewport {
            width: 390,
            height: 844,
            device_scale_factor: Some(3.0),
            emulating_mobile: true,
            is_landscape: false,
            has_touch: true,
        })
        .window_size(390, 844)
        .arg("--disable-gpu")
        .arg("--no-sandbox")
        .arg("--disable-dev-shm-usage")
        .arg("--no-first-run")
        .arg("--user-agent=Mozilla/5.0 (iPhone; CPU iPhone OS 17_4 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.4 Mobile/15E148 Safari/604.1")
        .build()
        .map_err(|e| anyhow!("Mobile browser config error: {}", e))?;

    let (mut browser, mut handler) = Browser::launch(config)
        .await
        .map_err(|e| anyhow!("Failed to launch mobile browser: {}", e))?;

    let _handle = tokio::spawn(async move {
        while let Some(event) = handler.next().await {
            if let Err(e) = event {
                error!("CDP mobile handler error: {}", e);
            }
        }
    });

    let result: Result<(u16, String)> = async {
        let page = browser
            .new_page(url)
            .await
            .map_err(|e| anyhow!("Mobile page navigation failed: {}", e))?;

        tokio::time::sleep(Duration::from_millis(wait_time)).await;

        let html = page
            .content()
            .await
            .map_err(|e| anyhow!("Failed to get mobile page content: {}", e))?;

        Ok((200u16, html))
    }
    .await;

    browser.close().await.ok();
    result
}
