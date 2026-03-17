//! Global stateful CDP session for browser automation (Phase 18 — Playwright Killer).
//!
//! A single `AutomationSession` is kept alive across MCP tool calls via a
//! process-global `LazyLock<Mutex<Option<…>>>`.  The session lazy-starts on
//! first use (prioritising Brave, then Chrome/Chromium) and remains alive
//! until `close_session()` is called explicitly via `scout_browser_close`.
//!
//! Thread-safety: `Browser` and `Page` are both `Arc`-backed in chromiumoxide
//! and are `Send + Sync`, making the whole `AutomationSession` `Send`.

use crate::scraping::browser_manager;
use anyhow::{anyhow, Result};
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
    data_dir: PathBuf,
}

impl AutomationSession {
    /// Gracefully close the browser and remove the temporary profile directory.
    pub async fn shutdown(mut self) {
        let _ =
            tokio::time::timeout(std::time::Duration::from_secs(3), self.browser.close()).await;
        self.handler_task.abort();
        let _ = self.handler_task.await;
        let _ = tokio::fs::remove_dir_all(&self.data_dir).await;
        info!("🛑 Automation session closed");
    }
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

    info!("🚀 Launching automation session ({})", exe);

    let (config, data_dir) =
        browser_manager::build_headless_config(&exe, None, 1280, 800)?;

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
