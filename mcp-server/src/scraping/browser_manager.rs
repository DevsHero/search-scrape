//! Native browser management using `chromiumoxide`.
//!
//! This module is the **single source of truth** for:
//! * Finding a usable browser executable (Brave → Chrome → Chromium, cross-platform).
//! * `BrowserPool` — shared persistent browser instance with tab reuse (Step 2).
//! * Launching a headless browser session.
//! * Lightweight "fetch HTML" primitives + ad-block network filter (Step 3).
//! * Smart `wait_until_stable` / `auto_scroll` for SPA / lazy pages (Step 4).
//!
//! All other modules (cdp.rs, search engines, scrape tool, etc.) use this
//! module. No external headless-browser sidecar is required.
//!
//! Stealth model:
//! - This module provides *process-level* defaults (user-agent rotation, browser flags).
//! - JS-level stealth injection is applied in the CDP pipeline (see `rust_scraper/stealth.rs`
//!   and `rust_scraper/cdp.rs`).

use aho_corasick::AhoCorasick;
use anyhow::{anyhow, Result};
use chromiumoxide::browser::BrowserConfig;
use chromiumoxide::handler::viewport::Viewport;
use chromiumoxide::{Browser, Page};
use futures::StreamExt;
use rand::seq::IndexedRandom;
use std::path::Path;
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{info, warn};

// ── Realistic User-Agent pool (Step 1: Stealth & Evasion) ────────────────────

const DESKTOP_USER_AGENTS: &[&str] = &[
    // Chrome 132 – Windows
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/132.0.0.0 Safari/537.36",
    // Chrome 132 – macOS
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/132.0.0.0 Safari/537.36",
    // Chrome 131 – Linux
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
    // Firefox 133 – Windows
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:133.0) Gecko/20100101 Firefox/133.0",
    // Firefox 133 – macOS
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 14.7; rv:133.0) Gecko/20100101 Firefox/133.0",
    // Safari 17 – macOS
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_7_2) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.4.1 Safari/605.1.15",
    // Edge 132 – Windows
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/132.0.0.0 Safari/537.36 Edg/132.0.0.0",
];

/// Returns a randomly-chosen realistic desktop User-Agent string.
pub fn random_user_agent() -> &'static str {
    let mut rng = rand::rng();
    DESKTOP_USER_AGENTS
        .choose(&mut rng)
        .copied()
        .unwrap_or(DESKTOP_USER_AGENTS[0])
}

// ── Browser executable discovery ─────────────────────────────────────────────

/// Find a usable Chromium-family browser executable.
///
/// Resolution order:
/// 1. `CHROME_EXECUTABLE` env var (explicit override)
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
/// Use this to gate JS rendering / CDP features.
pub fn native_browser_available() -> bool {
    find_chrome_executable().is_some()
}

// ── Headless browser config builder ──────────────────────────────────────────

/// Build a `BrowserConfig` for headless operation with stealth defaults.
///
/// Flags chosen for:
/// * Compatibility with CI / restricted environments (`--no-sandbox`, `--disable-dev-shm-usage`).
/// * Stealth — `--disable-blink-features=AutomationControlled` hides the
///   `navigator.webdriver` flag; UA is randomly drawn from `DESKTOP_USER_AGENTS`.
pub fn build_headless_config(
    exe: &str,
    proxy_url: Option<&str>,
    width: u32,
    height: u32,
) -> Result<BrowserConfig> {
    let ua = random_user_agent();

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
        .arg("--no-sandbox") // often required in CI / restricted environments
        .arg("--disable-setuid-sandbox")
        .arg("--disable-dev-shm-usage") // avoids /dev/shm OOM in constrained environments
        .arg("--disable-extensions")
        .arg("--disable-background-networking")
        .arg("--disable-sync")
        .arg("--disable-translate")
        .arg("--disable-crash-reporter")
        .arg("--disable-breakpad")
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg("--hide-scrollbars")
        .arg("--mute-audio")
        // Stealth: suppress CDP automation fingerprint
        .arg("--disable-blink-features=AutomationControlled")
        .arg(format!("--user-agent={}", ua));

    if let Some(proxy) = proxy_url {
        builder = builder.arg(format!("--proxy-server={}", proxy));
    }

    builder
        .build()
        .map_err(|e| anyhow!("Failed to build browser config: {}", e))
}

// ── Browser Pool (Step 2: tab reuse) ─────────────────────────────────────────

/// A shared, long-lived browser instance that reuses tabs per request.
///
/// Instead of launching/destroying a full browser on every scrape (slow),
/// `BrowserPool` keeps **one** browser alive and opens a fresh **tab** per
/// request.  If the browser crashes, the next `acquire()` restarts it
/// transparently.
///
/// Store `Arc<BrowserPool>` in `AppState` so all handlers share one instance.
pub struct BrowserPool {
    exe: String,
    inner: Mutex<Option<Browser>>,
}

impl BrowserPool {
    /// Create a pool for the given executable. Browser is lazy-started.
    pub fn new(exe: impl Into<String>) -> Arc<Self> {
        Arc::new(Self {
            exe: exe.into(),
            inner: Mutex::new(None),
        })
    }

    /// Create a pool using the auto-discovered executable.
    /// Returns `None` if no browser is installed on this machine.
    pub fn new_auto() -> Option<Arc<Self>> {
        find_chrome_executable().map(Self::new)
    }

    /// Acquire a fresh tab from the persistent browser.
    ///
    /// * Lazy-starts the browser on first call.
    /// * Restarts transparently if the process has crashed.
    /// * Close the returned `Page` when done — the browser stays alive.
    pub async fn acquire(&self, proxy_url: Option<&str>) -> Result<Page> {
        let mut guard = self.inner.lock().await;

        // Probe: try opening a blank tab to test if browser is still alive.
        let alive = match guard.as_mut() {
            Some(b) => b.new_page("about:blank").await.is_ok(),
            None => false,
        };

        if !alive {
            if guard.is_some() {
                warn!("🔄 Browser pool: instance dead, restarting...");
                if let Some(mut old) = guard.take() {
                    let _ = old.close().await;
                }
            }
            info!("🚀 Browser pool: launching new instance ({})", self.exe);
            let config = build_headless_config(&self.exe, proxy_url, 1920, 1080)?;
            let (new_browser, mut handler) = Browser::launch(config)
                .await
                .map_err(|e| anyhow!("Pool: failed to launch ({}): {}", self.exe, e))?;
            tokio::spawn(async move {
                while let Some(event) = handler.next().await {
                    if let Err(e) = event {
                        warn!("Pool CDP handler error: {}", e);
                    }
                }
            });
            *guard = Some(new_browser);
        }

        let b = guard.as_mut().expect("browser present after init");
        b.new_page("about:blank")
            .await
            .map_err(|e| anyhow!("Pool: failed to open tab: {}", e))
    }

    /// Gracefully close the pooled browser instance.
    pub async fn shutdown(&self) {
        let mut guard = self.inner.lock().await;
        if let Some(mut b) = guard.take() {
            let _ = b.close().await;
            info!("🛑 Browser pool shut down");
        }
    }
}

// ── Ad-block / network interception (Step 3) ─────────────────────────────────

const AD_BLOCK_PATTERNS: &[&str] = &[
    "doubleclick.net",
    "googlesyndication.com",
    "googletagmanager.com",
    "googletagservices.com",
    "adservice.google.",
    "amazon-adsystem.com",
    "ads.twitter.com",
    "ads.linkedin.com",
    "advertising.com",
    "criteo.com",
    "taboola.com",
    "outbrain.com",
    "moatads.com",
    "adnxs.com",
    "google-analytics.com",
    "analytics.google.com",
    "segment.com/v1/t",
    "segment.io/v1",
    "mixpanel.com/track",
    "hotjar.com",
    "mouseflow.com",
    "fullstory.com",
    "newrelic.com/",
    "nr-data.net",
    "connect.facebook.net",
    "platform.twitter.com/widgets",
    "cookielaw.org",
    "cookiebot.com",
    "onetrust.com",
];

static AD_BLOCK_MATCHER: OnceLock<AhoCorasick> = OnceLock::new();

fn ad_block_matcher() -> &'static AhoCorasick {
    AD_BLOCK_MATCHER.get_or_init(|| {
        // Patterns are simple substrings; Aho-Corasick gives linear-time scan.
        AhoCorasick::new(AD_BLOCK_PATTERNS).expect("valid ad-block patterns")
    })
}

/// Returns `true` if this URL should be blocked.
///
/// Set `block_images = true` to also drop image/video/font URLs for maximum
/// speed when you only need text content.
pub fn should_block_url(url: &str, block_images: bool) -> bool {
    if ad_block_matcher().is_match(url) {
        return true;
    }
    if block_images {
        let lower = url.to_lowercase();
        for ext in [
            ".jpg", ".jpeg", ".png", ".gif", ".webp", ".svg", ".ico", ".mp4", ".webm", ".ogg",
            ".mp3", ".woff", ".woff2", ".ttf",
        ] {
            if lower.contains(ext) {
                return true;
            }
        }
    }
    false
}

/// Returns `true` for CDP resource types that are always unnecessary for text
/// extraction (media, font).
pub fn should_block_resource_type(resource_type: &str) -> bool {
    ["media", "font"]
        .iter()
        .any(|t| resource_type.eq_ignore_ascii_case(t))
}

// ── Smart wait / networkidle (Step 4) ────────────────────────────────────────

/// Wait until the page network goes idle (no new resource entries for `quiet_ms`
/// consecutive ms) or until `timeout_ms` has elapsed.
///
/// Polls `performance.getEntriesByType("resource").length` every 250 ms —
/// a Playwright-style networkidle heuristic that works without CDP Network events.
pub async fn wait_until_stable(page: &Page, quiet_ms: u64, timeout_ms: u64) -> Result<()> {
    let poll_ms = 250u64;
    let start = std::time::Instant::now();
    let mut last_count: u64 = 0;
    let mut stable_since = std::time::Instant::now();

    loop {
        if start.elapsed().as_millis() as u64 >= timeout_ms {
            info!("wait_until_stable: timeout after {}ms", timeout_ms);
            break;
        }

        let count: u64 = page
            .evaluate("performance.getEntriesByType('resource').length")
            .await
            .ok()
            .and_then(|v| v.into_value::<serde_json::Value>().ok())
            .and_then(|j| j.as_u64())
            .unwrap_or(0);

        let ready_complete: bool = page
            .evaluate("document.readyState")
            .await
            .ok()
            .and_then(|v| v.into_value::<serde_json::Value>().ok())
            .and_then(|j| j.as_str().map(|s| s == "complete"))
            .unwrap_or(false);

        if !ready_complete {
            // DOM not fully loaded; keep waiting and do not allow "idle" to trigger.
            stable_since = std::time::Instant::now();
            last_count = count;
        } else if count != last_count {
            last_count = count;
            stable_since = std::time::Instant::now();
        } else if stable_since.elapsed().as_millis() as u64 >= quiet_ms {
            info!(
                "wait_until_stable: idle after {}ms ({} resources)",
                start.elapsed().as_millis(),
                count
            );
            break;
        }

        tokio::time::sleep(Duration::from_millis(poll_ms)).await;
    }
    Ok(())
}

impl Drop for BrowserPool {
    fn drop(&mut self) {
        // Best-effort cleanup. Drop cannot await; if we're inside a tokio runtime,
        // spawn a task to close the browser to avoid zombie Chromium processes.
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };

        if let Ok(mut guard) = self.inner.try_lock() {
            if let Some(mut browser) = guard.take() {
                handle.spawn(async move {
                    let _ = browser.close().await;
                });
            }
        }
    }
}

/// Auto-scroll the full page height to trigger lazy-loaded / intersection-observer
/// content before HTML capture.
pub async fn auto_scroll(page: &Page) -> Result<()> {
    let height: u64 = page
        .evaluate(
            "() => Math.max(document.body.scrollHeight, document.documentElement.scrollHeight)",
        )
        .await
        .ok()
        .and_then(|v| v.into_value::<serde_json::Value>().ok())
        .and_then(|j| j.as_u64())
        .unwrap_or(3000);

    let step = 600u64;
    let steps = (height / step).min(20); // cap to avoid infinite-scroll traps
    for i in 0..=steps {
        let y = i * step;
        if let Err(e) = page
            .evaluate(format!(
                "window.scrollTo({{top: {y}, behavior: 'smooth'}});"
            ))
            .await
        {
            warn!("auto_scroll: step {} error: {}", i, e);
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    Ok(())
}

// ── Lightweight "fetch HTML" primitives ──────────────────────────────────────

/// Fetch the rendered HTML of `url` using a **native headless browser**.
///
/// This is the lightweight alternative to the old HTTP headless-browser sidecar.
/// It launches a fresh browser, navigates, waits, captures HTML, then
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
                warn!("CDP handler error: {}", e);
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
        .arg("--disable-blink-features=AutomationControlled")
        .arg("--user-agent=Mozilla/5.0 (iPhone; CPU iPhone OS 17_4 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.4 Mobile/15E148 Safari/604.1")
        .build()
        .map_err(|e| anyhow!("Mobile browser config error: {}", e))?;

    let (mut browser, mut handler) = Browser::launch(config)
        .await
        .map_err(|e| anyhow!("Failed to launch mobile browser: {}", e))?;

    let _handle = tokio::spawn(async move {
        while let Some(event) = handler.next().await {
            if let Err(e) = event {
                warn!("CDP mobile handler error: {}", e);
            }
        }
    });

    let result: Result<(u16, String)> = async {
        let page = browser
            .new_page(url)
            .await
            .map_err(|e| anyhow!("Mobile page navigation failed: {}", e))?;

        wait_until_stable(&page, wait_time.min(3000), wait_time + 5000).await?;

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
