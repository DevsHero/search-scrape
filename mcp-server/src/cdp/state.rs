//! Global stateful CDP session for browser automation (Phase 18/19 — Playwright Killer).
//!
//! A single `AutomationSession` is kept alive across MCP tool calls via a
//! process-global `LazyLock<Mutex<Option<…>>>`.  The session lazy-starts on
//! first use (prioritising Brave, then Chrome/Chromium) and remains alive
//! until `close_session()` is called explicitly via `scout_browser_close`.
//!
//! Phase 19 additions:
//! * Uses `--headless=new` for full rendering fidelity (no visible window).
//! * Dedicated persistent agent profile at `~/.cortex-scout/agent_profile` —
//!   cookies and site state survive across `close_session` / re-open cycles
//!   and never touch the user's personal browser profile.
//!
//! Thread-safety: `Browser` and `Page` are both `Arc`-backed in chromiumoxide
//! and are `Send + Sync`, making the whole `AutomationSession` `Send`.

use crate::scraping::browser_manager;
use anyhow::{anyhow, Result};
use chromiumoxide::browser::BrowserConfig;
use chromiumoxide::handler::viewport::Viewport;
use chromiumoxide::{Browser, Page};
use futures::StreamExt;
use std::path::PathBuf;
use std::sync::LazyLock;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::info;

// ── Session struct ────────────────────────────────────────────────────────────

/// A persistent Brave/Chrome session kept open between automation tool calls.
pub struct AutomationSession {
    pub browser: Browser,
    /// The single long-lived tab used for all automation steps.
    pub page: Page,
    handler_task: JoinHandle<()>,
    /// Profile directory.  Only removed on shutdown when `cleanup_profile` is true
    /// (i.e., when we fell back to a temporary dir because the home dir was unavailable).
    data_dir: PathBuf,
    cleanup_profile: bool,
}

impl AutomationSession {
    /// Gracefully close the browser.  The persistent agent profile is preserved.
    pub async fn shutdown(mut self) {
        let _ =
            tokio::time::timeout(std::time::Duration::from_secs(3), self.browser.close()).await;
        self.handler_task.abort();
        let _ = self.handler_task.await;
        if self.cleanup_profile {
            let _ = tokio::fs::remove_dir_all(&self.data_dir).await;
        }
        info!("🛑 Automation session closed");
    }
}

// ── Agent profile path ────────────────────────────────────────────────────────

/// Returns the persistent agent profile directory, creating it if needed.
///
/// Path: `~/.cortex-scout/agent_profile`
/// Falls back to a UUID temp dir if the home directory cannot be determined.
fn agent_profile_dir() -> (PathBuf, bool) {
    if let Some(home) = dirs::home_dir() {
        let dir = home.join(".cortex-scout").join("agent_profile");
        if std::fs::create_dir_all(&dir).is_ok() {
            return (dir, false); // persistent — do not remove on shutdown
        }
    }
    // Fallback: temp dir (cleaned up on shutdown)
    let dir = std::env::temp_dir().join(format!(
        "cortex-scout-agent-{}",
        uuid::Uuid::new_v4()
    ));
    let _ = std::fs::create_dir_all(&dir);
    (dir, true)
}

// ── Config builder for automation sessions ───────────────────────────────────

/// Build a `BrowserConfig` for the silent automation session.
///
/// Differences from `browser_manager::build_headless_config`:
/// * Uses a **persistent** agent profile dir instead of a random temp dir.
/// * Passes `--headless=new` for modern full-fidelity headless rendering.
fn build_agent_config(exe: &str) -> Result<(BrowserConfig, PathBuf, bool)> {
    let (profile_dir, cleanup) = agent_profile_dir();
    let ua = browser_manager::random_user_agent();

    let config = BrowserConfig::builder()
        .chrome_executable(exe)
        .viewport(Viewport {
            width: 1280,
            height: 800,
            device_scale_factor: Some(1.0),
            emulating_mobile: false,
            is_landscape: true,
            has_touch: false,
        })
        .window_size(1280, 800)
        // Modern headless — full rendering fidelity, no window shown.
        // Overrides the default --headless that chromiumoxide injects.
        .arg("--headless=new")
        .arg("--disable-gpu")
        .arg("--no-sandbox")
        .arg("--disable-setuid-sandbox")
        .arg("--disable-dev-shm-usage")
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
        .arg("--disable-blink-features=AutomationControlled")
        // Persistent isolated profile — avoids SingletonLock conflicts with
        // the user's active browser while retaining cookies/session state.
        .arg(format!("--user-data-dir={}", profile_dir.display()))
        .arg(format!("--user-agent={}", ua))
        .build()
        .map_err(|e| anyhow!("Failed to build agent browser config: {}", e))?;

    Ok((config, profile_dir, cleanup))
}

// ── Global singleton ──────────────────────────────────────────────────────────

static AUTOMATION_SESSION: LazyLock<Mutex<Option<AutomationSession>>> =
    LazyLock::new(|| Mutex::new(None));

/// Returns a reference to the global session mutex.
pub fn session_lock() -> &'static Mutex<Option<AutomationSession>> {
    &AUTOMATION_SESSION
}

// ── Session lifecycle helpers ─────────────────────────────────────────────────

/// Ensure a healthy session is in the guard.
///
/// * If a session already exists and its CDP channel responds, it is reused.
/// * If the session is dead or absent, a new browser is launched and stored.
///
/// The caller **must** hold the mutex guard.  We accept it by `&mut MutexGuard`
/// so we can take/replace the contained `Option` without double-locking.
pub async fn ensure_session(
    guard: &mut tokio::sync::MutexGuard<'_, Option<AutomationSession>>,
) -> Result<()> {
    // Probe liveness: clone Page (cheap Arc clone) so we don't hold a borrow
    // across the await point, which would conflict with the later mutable take.
    let probe_page: Option<Page> = guard.as_ref().map(|s| s.page.clone());

    let needs_launch = match probe_page {
        Some(page) => page.evaluate("1").await.is_err(),
        None => true,
    };

    if !needs_launch {
        return Ok(());
    }

    // Tear down dead session if one exists.
    if guard.is_some() {
        if let Some(dead) = guard.take() {
            dead.shutdown().await;
        }
    }

    // Resolve browser executable — Brave is first in the candidate list.
    let exe = browser_manager::find_chrome_executable()
        .ok_or_else(|| anyhow!("No browser found. Install Brave, Chrome, or Chromium."))?;

    info!("🚀 Launching automation session ({}) with --headless=new + persistent profile", exe);

    let (config, data_dir, cleanup_profile) = build_agent_config(&exe)?;

    let (browser, mut handler) =
        browser_manager::launch_browser_serialized(config, "automation").await?;

    let handler_task = tokio::spawn(async move {
        while let Some(event) = handler.next().await {
            if let Err(e) = event {
                browser_manager::log_cdp_handler_error("automation handler", &e.to_string());
            }
        }
    });

    let page = browser
        .new_page("about:blank")
        .await
        .map_err(|e| anyhow!("Failed to open initial tab: {}", e))?;

    **guard = Some(AutomationSession {
        browser,
        page,
        handler_task,
        data_dir,
        cleanup_profile,
    });

    Ok(())
}

/// Explicitly close and clear the global automation session.
/// Called by `scout_browser_close`.
pub async fn close_session() -> Result<()> {
    let mut guard = AUTOMATION_SESSION.lock().await;
    if let Some(session) = guard.take() {
        session.shutdown().await;
    }
    Ok(())
}
