use crate::types::ScrapeResponse;
use crate::AppState;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

#[cfg(feature = "non_robot_search")]
use crate::types::EmbeddedDataSource;

#[cfg(feature = "non_robot_search")]
use crate::rust_scraper::RustScraper;
#[cfg(feature = "non_robot_search")]
use anyhow::anyhow;
#[cfg(feature = "non_robot_search")]
use rand::distr::{Distribution, Uniform};
#[cfg(feature = "non_robot_search")]
use std::path::Path;
#[cfg(feature = "non_robot_search")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "non_robot_search")]
use std::time::Instant;
#[cfg(feature = "non_robot_search")]
use tokio::sync::watch;
#[cfg(feature = "non_robot_search")]
use tracing::{info, warn};

#[cfg(feature = "non_robot_search")]
#[derive(Clone, Copy, Debug)]
enum NonRobotState {
    Initial,
    VisibleBrowserLaunch,
    Interaction,
    ChallengeDetection,
    HitlTrigger,
    UserActionCompletionDetection,
    ResumeAndExtract,
    Unlocking,
    Done,
}

#[cfg(feature = "non_robot_search")]
fn log_state(state: NonRobotState) {
    info!("non_robot_search_state={:?}", state);
}

#[cfg(feature = "non_robot_search")]
use chromiumoxide::Browser;
#[cfg(feature = "non_robot_search")]
use futures::StreamExt;

#[cfg(feature = "non_robot_search")]
use crossterm::event::{self, Event as TermEvent, KeyCode, KeyEventKind};
#[cfg(feature = "non_robot_search")]
use atty::Stream as AttyStream;
#[cfg(feature = "non_robot_search")]
use notify_rust::Notification;
#[cfg(all(feature = "non_robot_search", not(target_os = "macos")))]
use rfd::{MessageButtons, MessageDialog, MessageLevel};
#[cfg(feature = "non_robot_search")]
use rodio::{OutputStreamBuilder, Sink, Source};

#[cfg(feature = "non_robot_search")]
use std::time::SystemTime;

/// Configuration for `non_robot_search`.
///
/// This feature is intentionally conservative:
/// - It requires explicit user consent before launching.
/// - It supports an emergency abort (hold ESC ~3s).
/// - It times out if the user doesn't respond during HITL.
#[derive(Clone, Debug)]
pub struct NonRobotSearchConfig {
    pub url: String,
    pub max_chars: usize,
    pub use_proxy: bool,
    pub quality_mode: crate::rust_scraper::QualityMode,
    pub captcha_grace: Duration,
    pub human_timeout: Duration,
    pub user_profile_path: Option<String>,
    // Deep extraction features
    pub auto_scroll: bool,
    pub wait_for_selector: Option<String>,
}

#[derive(Debug, Error)]
pub enum NonRobotSearchError {
    #[error("interactive permission prompt required (no TTY attached)")]
    InteractiveRequired,

    #[error("user cancelled")]
    Cancelled,

    #[error("emergency abort triggered")]
    EmergencyAbort,

    #[error("timeout waiting for human intervention")]
    HumanUnavailable,

    #[error("browser window was closed during HITL")]
    BrowserClosed,

    #[error("browser launch failed: {0}")]
    BrowserLaunchFailed(String),

    #[error("automation failed: {0}")]
    AutomationFailed(String),
}

pub async fn execute_non_robot_search(
    state: &Arc<AppState>,
    cfg: NonRobotSearchConfig,
) -> Result<ScrapeResponse, NonRobotSearchError> {
    #[cfg(feature = "non_robot_search")]
    {
        execute_non_robot_search_impl(state, cfg).await
    }

    #[cfg(not(feature = "non_robot_search"))]
    {
        let _ = (state, cfg);
        Err(NonRobotSearchError::AutomationFailed(
            "feature not enabled".to_string(),
        ))
    }
}

#[cfg(feature = "non_robot_search")]
async fn execute_non_robot_search_impl(
    state: &Arc<AppState>,
    cfg: NonRobotSearchConfig,
) -> Result<ScrapeResponse, NonRobotSearchError> {
    // Global timeout: human_timeout + 30s safety margin
    let global_timeout = cfg.human_timeout + Duration::from_secs(30);
    
    match tokio::time::timeout(global_timeout, execute_non_robot_search_inner(state, cfg)).await {
        Ok(result) => result,
        Err(_) => {
            warn!("non_robot_search: global timeout exceeded ({}s), force-killing browser", global_timeout.as_secs());
            // Emergency cleanup: kill all debug browsers on port 9222
            force_kill_all_debug_browsers(9222);
            Err(NonRobotSearchError::AutomationFailed(
                format!("global timeout exceeded ({}s)", global_timeout.as_secs())
            ))
        }
    }
}

#[cfg(feature = "non_robot_search")]
async fn execute_non_robot_search_inner(
    state: &Arc<AppState>,
    cfg: NonRobotSearchConfig,
) -> Result<ScrapeResponse, NonRobotSearchError> {
    log_state(NonRobotState::Initial);

    // Ensure sequential execution of non-robot tool calls.
    // This avoids Chromium profile lock conflicts (e.g., SingletonLock) when callers reuse a live profile.
    let _serial_guard = state.non_robot_search_lock.lock().await;

    notify_and_prompt_user(&cfg)?;

    let (abort_tx, mut abort_rx) = watch::channel(false);
    let killswitch = KillSwitch::start(abort_tx);

    // Best-effort input controller. Full OS-level input blocking is platform-specific and requires
    // elevated permissions (e.g., Accessibility on macOS). We keep the interface so a stricter
    // implementation can be plugged in later.
    let input_controller: Box<dyn InputController> = Box::new(NoopInputController);

    // Acquire proxy (if requested) before input locking.
    let proxy_arg = if cfg.use_proxy {
        if let Some(manager) = &state.proxy_manager {
            match manager.switch_to_best_proxy().await {
                Ok(proxy_url) => Some(proxy_url),
                Err(e) => {
                    warn!("non_robot_search: proxy requested but unavailable: {}", e);
                    None
                }
            }
        } else {
            None
        }
    } else {
        None
    };

    // Lock inputs for the automation phase.
    input_controller.lock().ok();
    let lock_guard = InputLockGuard::new(&*input_controller);

    let mut session =
        BrowserSession::launch(proxy_arg.as_deref(), cfg.user_profile_path.as_deref())
            .await
            .map_err(|e| NonRobotSearchError::BrowserLaunchFailed(e.to_string()))?;

    log_state(NonRobotState::VisibleBrowserLaunch);

    // Drive main flow; if the browser transport drops mid-run (common during manual interaction),
    // relaunch once using the same profile directory and retry.
    let mut result = run_flow(
        state,
        &cfg,
        &mut session,
        input_controller.as_ref(),
        &mut abort_rx,
    )
    .await;

    if should_attempt_recovery(&result) {
        warn!("non_robot_search: transport dropped; attempting one recovery relaunch");
        if let Err(e) = session.relaunch().await {
            result = Err(NonRobotSearchError::AutomationFailed(format!(
                "recovery relaunch failed: {}",
                e
            )));
        } else {
            // Best-effort: re-run the flow. If the interactive step is still present, HITL triggers again.
            result = run_flow(
                state,
                &cfg,
                &mut session,
                input_controller.as_ref(),
                &mut abort_rx,
            )
            .await;
        }
    }

    drop(lock_guard);
    log_state(NonRobotState::Unlocking);
    session.close().await;
    killswitch.stop();

    log_state(NonRobotState::Done);

    // Record proxy result.
    if let (Some(proxy_url), Some(manager)) = (proxy_arg.as_ref(), state.proxy_manager.as_ref()) {
        let success = result.is_ok();
        let _ = manager.record_proxy_result(proxy_url, success, None).await;
    }

    result
}

#[cfg(feature = "non_robot_search")]
async fn wait_for_network_idle_heuristic(page: &chromiumoxide::Page, timeout: Duration) {
    let timeout_ms = timeout.as_millis().min(u128::from(u64::MAX)) as u64;
    let js = format!(
        r#"(async () => {{
            const timeoutMs = {timeout_ms};
            const idleMs = 1000;
            const interval = 250;

            const start = Date.now();
            let lastCount = 0;
            let stableMs = 0;

            try {{ lastCount = performance.getEntriesByType('resource').length; }} catch (_) {{ lastCount = 0; }}

            while (Date.now() - start < timeoutMs) {{
                await new Promise(r => setTimeout(r, interval));
                let curCount = lastCount;
                try {{ curCount = performance.getEntriesByType('resource').length; }} catch (_) {{ curCount = lastCount; }}

                const ready = (document.readyState === 'complete');
                if (ready && curCount === lastCount) {{
                    stableMs += interval;
                    if (stableMs >= idleMs) {{
                        return {{ ok: true, readyState: document.readyState, resourceCount: curCount, waitedMs: (Date.now() - start) }};
                    }}
                }} else {{
                    stableMs = 0;
                }}
                lastCount = curCount;
            }}

            return {{ ok: false, readyState: document.readyState, resourceCount: lastCount, waitedMs: (Date.now() - start) }};
        }})()"#,
        timeout_ms = timeout_ms
    );

    match page.evaluate(js).await {
        Ok(val) => {
            if let Ok(info) = val.into_value::<serde_json::Value>() {
                let ok = info.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
                let waited = info.get("waitedMs").and_then(|v| v.as_u64()).unwrap_or(0);
                let rs = info
                    .get("readyState")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                let rc = info
                    .get("resourceCount")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                if ok {
                    info!(
                        "non_robot_search: network-idle heuristic reached (readyState={}, resources={}, waitedMs={})",
                        rs, rc, waited
                    );
                } else {
                    warn!(
                        "non_robot_search: network-idle heuristic timed out (readyState={}, resources={}, waitedMs={})",
                        rs, rc, waited
                    );
                }
            }
        }
        Err(e) => warn!("non_robot_search: network-idle heuristic failed: {}", e),
    }
}

#[cfg(feature = "non_robot_search")]
async fn run_pre_scrape_janitor_v2(page: &chromiumoxide::Page) {
    // Janitor v2.0: closes click-outside popups, forcibly re-enables scroll, and hides
    // screen-locking overlays via z-index + viewport coverage heuristics.
    let janitor_script = r#"
        (() => {
            const result = {
                ghost_click: 0,
                clicked: 0,
                hidden: 0,
                removed: 0,
                errors: []
            };

            const safe = (fn) => {
                try { return fn(); } catch (e) {
                    result.errors.push(String(e && e.message ? e.message : e));
                    return null;
                }
            };

            const isVisible = (el) => {
                if (!el) return false;
                if (el.disabled) return false;
                const style = window.getComputedStyle(el);
                if (!style || style.display === 'none' || style.visibility === 'hidden' || style.opacity === '0') return false;
                const rect = el.getBoundingClientRect();
                if (!rect || rect.width < 2 || rect.height < 2) return false;
                return true;
            };

            // 1) The Ghost Click: click (5,5) to close "click-outside" popups.
            safe(() => {
                const x = 5, y = 5;
                const target = document.elementFromPoint(x, y);
                if (!target) return false;
                const opts = { bubbles: true, cancelable: true, view: window, clientX: x, clientY: y };
                target.dispatchEvent(new MouseEvent('mousedown', opts));
                target.dispatchEvent(new MouseEvent('mouseup', opts));
                target.dispatchEvent(new MouseEvent('click', opts));
                result.ghost_click++;
                return true;
            });

            // 2) Force Visible UI: unlock scroll + reset common scroll locks.
            safe(() => {
                document.body?.style?.setProperty('overflow', 'visible', 'important');
                document.documentElement?.style?.setProperty('overflow', 'visible', 'important');
                document.body?.style?.setProperty('position', 'relative', 'important');
                // Some sites lock scroll on <html> via fixed height.
                document.documentElement?.style?.setProperty('height', 'auto', 'important');
                document.body?.style?.setProperty('height', 'auto', 'important');
                return true;
            });

            // 3) Smart Modal Removal: hide large, high-z overlays that cover the viewport.
            safe(() => {
                const vw = Math.max(1, window.innerWidth || 1);
                const vh = Math.max(1, window.innerHeight || 1);
                const vArea = vw * vh;

                const coversViewportEnough = (rect) => {
                    const left = Math.max(0, rect.left);
                    const top = Math.max(0, rect.top);
                    const right = Math.min(vw, rect.right);
                    const bottom = Math.min(vh, rect.bottom);
                    const w = Math.max(0, right - left);
                    const h = Math.max(0, bottom - top);
                    const area = w * h;
                    return (area / vArea) > 0.5;
                };

                const all = Array.from(document.querySelectorAll('body *'));
                for (const el of all) {
                    if (!el || el.id === '__shadowcrawl_hitl_overlay__') continue;
                    const style = window.getComputedStyle(el);
                    if (!style) continue;

                    const ziRaw = (style.zIndex || '').trim();
                    const z = Number.parseInt(ziRaw, 10);
                    if (!Number.isFinite(z) || z <= 1000) continue;

                    // Heuristic: overlays are typically fixed/absolute and intercept pointer events.
                    const pos = (style.position || '').toLowerCase();
                    if (!(pos === 'fixed' || pos === 'absolute' || pos === 'sticky')) continue;
                    if ((style.pointerEvents || '').toLowerCase() === 'none') continue;

                    const rect = el.getBoundingClientRect();
                    if (!rect || rect.width < 2 || rect.height < 2) continue;
                    if (!coversViewportEnough(rect)) continue;

                    // Hide instead of remove (safer when event listeners are attached elsewhere).
                    try {
                        el.style.setProperty('display', 'none', 'important');
                        el.style.setProperty('visibility', 'hidden', 'important');
                        result.hidden++;
                    } catch (_) {}
                }

                return true;
            });

            // 3.5) CSS Force-Clear: remove common blocking layers (LinkedIn + generic overlays).
            safe(() => {
                const selectors = [
                    '.artdeco-modal-overlay',
                    '.login-modal',
                    '#base-contextual-sign-in-modal',
                    '.contextual-sign-in-modal',
                    '[aria-modal="true"]',
                    '[role="dialog"]',
                    '[class*="modal-backdrop"]',
                    '[class*="overlay"]',
                    '[class*="Overlay"]'
                ];
                const nodes = document.querySelectorAll(selectors.join(','));
                nodes.forEach(el => {
                    try {
                        el.remove();
                        result.removed++;
                    } catch (_) {
                        // Fallback: hide and disable interactions.
                        try {
                            el.style.setProperty('display', 'none', 'important');
                            el.style.setProperty('visibility', 'hidden', 'important');
                            el.style.setProperty('pointer-events', 'none', 'important');
                            result.hidden++;
                        } catch (_) {}
                    }
                });
                return true;
            });

            // 4) Auto-close / accept common modals and consent banners.
            safe(() => {
                const needles = [
                    'close',
                    'accept',
                    'accept all',
                    'i agree',
                    'agree',
                    'got it',
                    'dismiss',
                    'continue'
                ];

                const candidates = Array.from(document.querySelectorAll(
                    'button, [role="button"], a[role="button"], input[type="button"], input[type="submit"]'
                ));

                for (const el of candidates) {
                    if (!isVisible(el)) continue;
                    const text = (el.innerText || el.value || el.getAttribute('aria-label') || el.getAttribute('title') || '').trim();
                    if (!text) continue;
                    const t = text.toLowerCase();
                    const isX = text === '×' || t === 'x';
                    const match = isX || needles.some(n => t === n || t.startsWith(n + ' ') || t.includes(n));
                    if (!match) continue;

                    const href = (el.getAttribute && el.getAttribute('href')) ? el.getAttribute('href') : '';
                    if (href && href.startsWith('javascript:')) continue;

                    try {
                        el.click();
                        result.clicked++;
                    } catch (_) {}
                }

                return true;
            });

            // 5) Aggressive DOM pruning of non-essential semantic tags and common overlays.
            safe(() => {
                const selectors = [
                    'nav',
                    'footer',
                    'aside',
                    'iframe',
                    '.cookie-banner',
                    '.modal-backdrop'
                ];
                const nodes = Array.from(document.querySelectorAll(selectors.join(',')));
                for (const el of nodes) {
                    try {
                        el.remove();
                        result.removed++;
                    } catch (_) {}
                }
                return true;
            });

            // 6) Tag editing: remove style/noscript only.
            // IMPORTANT: do NOT remove scripts here; some sites (e.g., LinkedIn) embed JSON-LD/JobPosting
            // and other state in script tags, and stripping them can cause missing provenance.
            safe(() => {
                const styles = Array.from(document.querySelectorAll('style, noscript'));
                for (const el of styles) {
                    try {
                        el.remove();
                        result.removed++;
                    } catch (_) {}
                }
                return true;
            });

            return result;
        })()
    "#;

    match page.evaluate(janitor_script).await {
        Ok(val) => {
            if let Ok(info) = val.into_value::<serde_json::Value>() {
                let ghost_click = info
                    .get("ghost_click")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                let clicked = info.get("clicked").and_then(|v| v.as_i64()).unwrap_or(0);
                let hidden = info.get("hidden").and_then(|v| v.as_i64()).unwrap_or(0);
                let removed = info.get("removed").and_then(|v| v.as_i64()).unwrap_or(0);
                if ghost_click > 0 || clicked > 0 || hidden > 0 || removed > 0 {
                    info!(
                        "non_robot_search: janitor v2 applied (ghost_click={}, clicked={}, hidden={}, removed={})",
                        ghost_click, clicked, hidden, removed
                    );
                }
            }
        }
        Err(e) => warn!("non_robot_search: janitor v2 script failed: {}", e),
    }
}

#[cfg(feature = "non_robot_search")]
fn should_attempt_recovery(result: &Result<ScrapeResponse, NonRobotSearchError>) -> bool {
    match result {
        Ok(_) => false,
        Err(NonRobotSearchError::BrowserClosed) => true,
        Err(NonRobotSearchError::AutomationFailed(msg)) => msg.contains("receiver is gone"),
        _ => false,
    }
}

#[cfg(feature = "non_robot_search")]
async fn run_flow(
    state: &Arc<AppState>,
    cfg: &NonRobotSearchConfig,
    session: &mut BrowserSession,
    input_controller: &dyn InputController,
    abort_rx: &mut watch::Receiver<bool>,
) -> Result<ScrapeResponse, NonRobotSearchError> {
    info!("non_robot_search: navigating to {}", cfg.url);

    log_state(NonRobotState::Interaction);

    // Professional, non-blocking notice inside the browser before we navigate.
    // This ensures operators see what is happening even when consent is auto-allowed.
    let _ = session
        .page
        .evaluate(get_prelaunch_overlay_script(&cfg.url))
        .await;

    session
        .page
        .goto(&cfg.url)
        .await
        .map_err(|e| NonRobotSearchError::AutomationFailed(format!("goto failed: {}", e)))?;

    human_like_idle(abort_rx).await?;
    human_like_scroll(&session.page, abort_rx).await?;
    maybe_human_like_mouse(&session.page, abort_rx).await?;

    // Detect interstitial/verification gates; if it persists beyond grace period, enter HITL.
    log_state(NonRobotState::ChallengeDetection);
    let start = Instant::now();
    loop {
        abort_if_needed(abort_rx)?;
        if session.is_closed() {
            return Err(NonRobotSearchError::BrowserClosed);
        }

        // 🚀 Check if user clicked manual return button
        if check_manual_return_triggered(&session.page).await {
            info!("non_robot_search: 🚀 Manual return button clicked during challenge detection");
            break;
        }

        let challenged = detect_challenge_dom(&session.page).await.unwrap_or(false)
            || detect_interstitial_like(&session.page)
                .await
                .unwrap_or(false);
        if !challenged {
            break;
        }

        if start.elapsed() >= cfg.captcha_grace {
            log_state(NonRobotState::HitlTrigger);
            request_human_help(session, input_controller).await?;
            log_state(NonRobotState::UserActionCompletionDetection);
            wait_for_human_resolution(session, cfg.human_timeout, abort_rx).await?;
            // After resolved, re-lock input and continue.
            input_controller.lock().ok();
            break;
        }

        tokio::time::sleep(Duration::from_millis(250)).await;
    }

    abort_if_needed(abort_rx)?;
    if session.is_closed() {
        return Err(NonRobotSearchError::BrowserClosed);
    }

    // Final content extraction.
    log_state(NonRobotState::ResumeAndExtract);

    // 🚀 CHECK FOR MANUAL RETURN BUTTON CLICK
    // Poll for button click signal with short timeout
    info!("non_robot_search: checking for manual return button click...");
    let manual_triggered = check_manual_return_triggered(&session.page).await;
    
    if manual_triggered {
        info!("non_robot_search: 🚀 MANUAL RETURN BUTTON CLICKED - Extracting immediately");
        play_tone(Tone::Success);
        // Skip wait steps, extract immediately
    } else {
        // Default auto-extraction flow
        info!("non_robot_search: No manual trigger, proceeding with auto-extraction");
        // Wait for the page to reach a best-effort "network idle" state before we run cleanup.
        // This is intentionally heuristic (JS polling) because chromiumoxide doesn't provide a stable
        // cross-version NetworkIdle API surface here.
        wait_for_network_idle_heuristic(&session.page, Duration::from_secs(20)).await;
    }

    // Janitor v2.0: more aggressive overlay cleanup + scroll unlock.
    // Runs immediately after the network-idle wait so we clear screen-locking popups early.
    run_pre_scrape_janitor_v2(&session.page).await;

    // Wait for specific selector if requested
    if let Some(selector) = &cfg.wait_for_selector {
        info!("non_robot_search: waiting for selector: {}", selector);
        let wait_script = format!(
            r#"
            (async () => {{
                const maxAttempts = 60; // 30 seconds max
                for (let i = 0; i < maxAttempts; i++) {{
                    const el = document.querySelector('{}');
                    if (el && el.offsetParent !== null) {{
                        return true;
                    }}
                    await new Promise(r => setTimeout(r, 500));
                }}
                return false;
            }})()
        "#,
            selector
        );

        match session.page.evaluate(wait_script).await {
            Ok(result) => {
                if let Ok(found) = result.into_value::<bool>() {
                    if found {
                        info!("non_robot_search: selector found: {}", selector);
                    } else {
                        warn!(
                            "non_robot_search: selector not found after 30s: {}",
                            selector
                        );
                    }
                }
            }
            Err(e) => warn!("non_robot_search: wait_for_selector failed: {}", e),
        }
    }

    // Auto-scroll to trigger lazy loading if requested
    if cfg.auto_scroll {
        info!("non_robot_search: auto-scrolling to trigger lazy loading...");
        let scroll_script = r#"
            (async () => {
                const scrollStep = 500;
                const scrollDelay = 300;
                const maxScrolls = 20;
                
                let lastHeight = document.documentElement.scrollHeight;
                let scrollCount = 0;
                
                while (scrollCount < maxScrolls) {
                    window.scrollBy(0, scrollStep);
                    await new Promise(r => setTimeout(r, scrollDelay));
                    
                    const newHeight = document.documentElement.scrollHeight;
                    if (window.scrollY + window.innerHeight >= newHeight - 100) {
                        // Reached bottom
                        break;
                    }
                    scrollCount++;
                }
                
                // Scroll back to top
                window.scrollTo(0, 0);
                return scrollCount;
            })()
        "#;

        match session.page.evaluate(scroll_script).await {
            Ok(result) => {
                if let Ok(count) = result.into_value::<i32>() {
                    info!("non_robot_search: completed {} scroll steps", count);
                }
            }
            Err(e) => warn!("non_robot_search: auto-scroll failed: {}", e),
        }
    }

    // Universal hydration nudge: a small scroll down/up to trigger lazy rendering.
    if matches!(
        cfg.quality_mode,
        crate::rust_scraper::QualityMode::Aggressive | crate::rust_scraper::QualityMode::High
    ) {
        let smart_scroll = r#"
            (async () => {
                window.scrollBy(0, 1000);
                await new Promise(r => setTimeout(r, 500));
                window.scrollBy(0, -1000);
                return true;
            })()
        "#;
        if let Err(e) = session.page.evaluate(smart_scroll).await {
            warn!("non_robot_search: smart scroll nudge failed: {}", e);
        }
    }

    // Dynamic settlement detection: wait until document.body.innerText.length stabilizes for 1.5s.
    let mut settle_time_ms: Option<u64> = None;
    if !matches!(cfg.quality_mode, crate::rust_scraper::QualityMode::Balanced) {
        info!("non_robot_search: waiting for innerText length to settle...");
        let settle_script = r#"
            (async () => {
                const maxChecks = 60;   // 15s max
                const interval = 250;   // ms
                const requiredStableMs = 1500;

                let lastLen = (document.body && document.body.innerText) ? document.body.innerText.length : 0;
                let stableMs = 0;

                for (let i = 0; i < maxChecks; i++) {
                    await new Promise(r => setTimeout(r, interval));
                    const curLen = (document.body && document.body.innerText) ? document.body.innerText.length : 0;
                    if (curLen === lastLen) {
                        stableMs += interval;
                        if (stableMs >= requiredStableMs) {
                            return { settled: true, checks: i + 1, len: curLen, elapsedMs: (i + 1) * interval };
                        }
                    } else {
                        stableMs = 0;
                    }
                    lastLen = curLen;
                }
                return { settled: false, checks: maxChecks, len: lastLen, elapsedMs: maxChecks * interval };
            })()
        "#;

        match session.page.evaluate(settle_script).await {
            Ok(result) => {
                if let Ok(info) = result.into_value::<serde_json::Value>() {
                    let settled = info
                        .get("settled")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let checks = info.get("checks").and_then(|v| v.as_i64()).unwrap_or(0);
                    let len = info.get("len").and_then(|v| v.as_i64()).unwrap_or(0);
                    let elapsed_ms = info.get("elapsedMs").and_then(|v| v.as_u64()).unwrap_or(0);
                    if elapsed_ms > 0 {
                        settle_time_ms = Some(elapsed_ms);
                    }
                    if settled {
                        info!(
                            "non_robot_search: innerText settled after {} checks (~{}s), len={}",
                            checks,
                            checks as f64 * 0.25,
                            len
                        );
                    } else {
                        warn!(
                            "non_robot_search: innerText did not fully settle after {} checks (len={})",
                            checks, len
                        );
                    }
                }
            }
            Err(e) => warn!("non_robot_search: innerText settle check failed: {}", e),
        }
    }

    // Run Janitor again immediately before extraction; some flows (auto-scroll, settle) can re-trigger overlays.
    wait_for_network_idle_heuristic(&session.page, Duration::from_secs(10)).await;
    run_pre_scrape_janitor_v2(&session.page).await;

    // Wait for click handlers/animations to finish.
    tokio::time::sleep(Duration::from_millis(500)).await;

    info!("non_robot_search: extracting HTML");
    // After HITL, the connection can be more fragile; prefer an evaluation-based snapshot.
    let html = get_html_snapshot(&session.page, session.is_closed())
        .await
        .map_err(|e| {
            NonRobotSearchError::AutomationFailed(format!("html extraction failed: {}", e))
        })?;

    let scraper = RustScraper::new_with_quality_mode(Some(cfg.quality_mode.as_str()));
    let mut scraped = scraper.process_html(&html, &cfg.url).await.map_err(|e| {
        NonRobotSearchError::AutomationFailed(format!("process_html failed: {}", e))
    })?;

    // Deep Metadata Hunt: if the normal extractor missed JSON-LD / JobPosting evidence, do a fallback scan.
    apply_metadata_fallback_from_html(&mut scraped, &html);

    if let Some(ms) = settle_time_ms {
        scraped.hydration_status.settle_time_ms = Some(ms);
    }

    // Cache + history, mirroring scrape_url.
    state
        .scrape_cache
        .insert(cfg.url.clone(), scraped.clone())
        .await;
    if let Some(memory) = &state.memory {
        let summary = format!("{} words (non_robot_search)", scraped.word_count);
        let domain = url::Url::parse(&cfg.url)
            .ok()
            .and_then(|u| u.host_str().map(|s| s.to_string()));
        let result_json = serde_json::to_value(&scraped).unwrap_or_default();
        let _ = memory
            .log_scrape(
                cfg.url.clone(),
                Some(scraped.title.clone()),
                summary,
                domain,
                &result_json,
            )
            .await;
    }

    // Ensure max_chars in handler layer; keep full here.
    Ok(scraped)
}

#[cfg(feature = "non_robot_search")]
fn apply_metadata_fallback_from_html(scraped: &mut ScrapeResponse, html: &str) {
    // Only run fallback if we appear to have missed embedded JSON evidence.
    if scraped.hydration_status.json_found
        || scraped.embedded_state_json.is_some()
        || !scraped.embedded_data_sources.is_empty()
    {
        return;
    }

    let mut sources = deep_metadata_hunt_jobposting_scripts(html);
    if sources.is_empty() {
        sources = deep_metadata_hunt_linkedin_job_evidence(html);
        if sources.is_empty() {
            return;
        }
    }

    // Prefer JSON-LD-ish sources, then any JobPosting-containing JSON.
    sources.sort_by(|a, b| {
        let a_ld = a.source_type.contains("ld+json") || a.source_type.contains("jsonld");
        let b_ld = b.source_type.contains("ld+json") || b.source_type.contains("jsonld");
        b_ld.cmp(&a_ld)
    });

    scraped.embedded_state_json = Some(sources[0].content.clone());
    scraped.embedded_data_sources.extend(sources);
    scraped.hydration_status.json_found = true;
    scraped
        .warnings
        .push("metadata_fallback_embedded_evidence_extracted".to_string());
}

#[cfg(feature = "non_robot_search")]
fn deep_metadata_hunt_jobposting_scripts(html: &str) -> Vec<EmbeddedDataSource> {
    // Keep this conservative and bounded: extract script bodies that look like JSON-LD or mention JobPosting.
    const MAX_SCRIPT_CHARS: usize = 120_000;

    let mut out: Vec<EmbeddedDataSource> = Vec::new();

    let script_re =
        match regex::Regex::new(r#"(?is)<script(?P<attrs>[^>]*)>(?P<body>.*?)</script>"#) {
            Ok(r) => r,
            Err(_) => return out,
        };
    let type_re = regex::Regex::new(r#"(?is)\btype\s*=\s*['\"](?P<t>[^'\"]+)['\"]"#).ok();

    for cap in script_re.captures_iter(html) {
        let attrs = cap.name("attrs").map(|m| m.as_str()).unwrap_or("");
        let body_raw = cap.name("body").map(|m| m.as_str()).unwrap_or("");
        let body = body_raw.trim();
        if body.len() < 20 {
            continue;
        }

        let typ = type_re
            .as_ref()
            .and_then(|r| r.captures(attrs))
            .and_then(|c| c.name("t").map(|m| m.as_str().to_lowercase()));

        let body_lc = body.to_lowercase();
        let looks_jsonld = typ
            .as_deref()
            .map(|t| t.contains("ld+json") || t.contains("json"))
            .unwrap_or(false);
        let mentions_jobposting = body_lc.contains("jobposting") || body_lc.contains("\"@type\"");

        if !(looks_jsonld || mentions_jobposting) {
            continue;
        }

        // Keep only likely-relevant candidates to avoid bloating outputs.
        if !body_lc.contains("jobposting") && !(typ.as_deref().unwrap_or("").contains("ld+json")) {
            continue;
        }

        let mut content = body.to_string();
        if content.len() > MAX_SCRIPT_CHARS {
            content.truncate(MAX_SCRIPT_CHARS);
        }

        let source_type = match typ {
            Some(t) => format!("script:{}:fallback", t),
            None => "script:unknown:fallback".to_string(),
        };

        out.push(EmbeddedDataSource {
            source_type,
            content,
        });

        if out.len() >= 8 {
            break;
        }
    }

    out
}

#[cfg(feature = "non_robot_search")]
fn deep_metadata_hunt_linkedin_job_evidence(html: &str) -> Vec<EmbeddedDataSource> {
    // LinkedIn guest job pages sometimes omit JSON-LD entirely, but still embed stable identifiers
    // in the HTML (e.g., `urn:li:jobPosting:<id>` and hidden <code> comment payloads).
    let mut job_ids: Vec<String> = Vec::new();
    let mut company_id: Option<String> = None;

    if let Ok(re) = regex::Regex::new(r#"urn:li:jobPosting:(\d+)"#) {
        for cap in re.captures_iter(html) {
            if let Some(m) = cap.get(1) {
                job_ids.push(m.as_str().to_string());
            }
        }
    }
    job_ids.sort();
    job_ids.dedup();

    // Hidden <code> tags with comment payloads.
    let code_capture = |id: &str| -> Option<String> {
        let pat = format!(
            r#"(?is)<code[^>]*\bid\s*=\s*['\"]{}['\"][^>]*>\s*<!--\s*\"?([^\"<>]+)\"?\s*-->\s*</code>"#,
            regex::escape(id)
        );
        regex::Regex::new(&pat)
            .ok()
            .and_then(|re| re.captures(html))
            .and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
    };

    let decorated_job_posting_id = code_capture("decoratedJobPostingId");
    let reference_id = code_capture("referenceId");

    // Meta companyId appears on many pages.
    if let Ok(re) = regex::Regex::new(r#"(?is)<meta\s+name=\"companyId\"\s+content=\"(\d+)\""#) {
        company_id = re
            .captures(html)
            .and_then(|c| c.get(1).map(|m| m.as_str().to_string()));
    }

    if job_ids.is_empty() && decorated_job_posting_id.is_none() && reference_id.is_none() {
        return Vec::new();
    }

    let evidence = serde_json::json!({
        "evidence_type": "linkedin_html_embedded_ids",
        "job_posting_ids": job_ids,
        "decoratedJobPostingId": decorated_job_posting_id,
        "referenceId": reference_id,
        "companyId": company_id,
    });

    vec![EmbeddedDataSource {
        source_type: "linkedin:html-evidence:fallback".to_string(),
        content: evidence.to_string(),
    }]
}

#[cfg(feature = "non_robot_search")]
async fn get_html_snapshot(page: &chromiumoxide::Page, closed: bool) -> anyhow::Result<String> {
    if closed {
        return Err(anyhow!("browser session closed"));
    }

    // First attempt: JS snapshot (often more reliable than Page::content()).
    if let Ok(val) = page.evaluate("document.documentElement.outerHTML").await {
        if let Ok(html) = val.into_value::<String>() {
            if !html.is_empty() {
                return Ok(html);
            }
        }
    }

    // Second attempt: Page::content with retries.
    let delays = [200u64, 500, 1200, 2000];
    for (i, delay_ms) in delays.iter().enumerate() {
        match page.content().await {
            Ok(html) => return Ok(html),
            Err(e) => {
                tracing::warn!("page.content attempt {} failed: {}", i + 1, e);
                tokio::time::sleep(std::time::Duration::from_millis(*delay_ms)).await;
            }
        }
    }

    // Last attempt: JS snapshot again.
    let val = page.evaluate("document.documentElement.outerHTML").await?;
    Ok(val.into_value::<String>().unwrap_or_default())
}

#[cfg(feature = "non_robot_search")]
fn notify_and_prompt_user(cfg: &NonRobotSearchConfig) -> Result<(), NonRobotSearchError> {
    let auto_allow = std::env::var("SHADOWCRAWL_NON_ROBOT_AUTO_ALLOW")
        .ok()
        .as_deref()
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("yes"))
        .unwrap_or(false)
    ;

    // Consent mode override:
    // - SHADOWCRAWL_NON_ROBOT_CONSENT=dialog => always dialog (blocking)
    // - SHADOWCRAWL_NON_ROBOT_CONSENT=tty    => always tty prompt
    // - default/auto => dialog by default (server-safe)
    let consent_mode = std::env::var("SHADOWCRAWL_NON_ROBOT_CONSENT")
        .ok()
        .unwrap_or_else(|| "auto".to_string())
        .to_lowercase();

    let force_dialog = matches!(consent_mode.as_str(), "dialog" | "gui");
    let force_tty = matches!(consent_mode.as_str(), "tty" | "terminal");

    // Always do a best-effort user-facing notice. This is intentionally non-blocking.
    // (Some environments do not have desktop notifications; ignore failures.)
    let target_line = format!("Target URL: {}", cfg.url);

    let notification_body = if auto_allow && !force_dialog {
        format!(
            "ShadowCrawl will open a visible browser for HITL rendering.\n\n{}\n\nTip: If a site blocks automation, solve it in the browser and click FINISH & RETURN.\nEmergency stop: hold ESC for ~3 seconds.",
            target_line
        )
    } else {
        format!(
            "ShadowCrawl is requesting permission to open a visible browser and navigate to the target page.\n\n{}\n\nClick OK to continue or Cancel to abort.\nEmergency stop: hold ESC for ~3 seconds.",
            target_line
        )
    };

    let _ = Notification::new()
        .summary("ShadowCrawl: HITL browser control")
        .body(&notification_body)
        .show();

    play_tone(Tone::Attention);

    // AUTO_ALLOW bypasses the blocking consent dialog *unless* the operator forces dialog mode.
    if auto_allow && !force_dialog {
        info!("non_robot_search: auto-allow enabled via SHADOWCRAWL_NON_ROBOT_AUTO_ALLOW");
        return Ok(());
    }

    let stdin_is_tty = atty::is(AttyStream::Stdin);

    // Default to a GUI popup for a more professional HITL experience.
    // Fallback to TTY only when explicitly requested, or when GUI dialogs are unavailable.
    let use_tty_prompt = force_tty;

    // If we have an interactive TTY and consent mode allows it, use strict Enter/Esc flow.
    if use_tty_prompt {
        println!(
            "ShadowCrawl needs screen access for high-fidelity rendering. Press [Enter] to allow or [Esc] to cancel."
        );

        loop {
            if event::poll(Duration::from_millis(100)).map_err(|e| {
                NonRobotSearchError::AutomationFailed(format!(
                    "failed to poll terminal input: {}",
                    e
                ))
            })? {
                if let TermEvent::Key(key) = event::read().map_err(|e| {
                    NonRobotSearchError::AutomationFailed(format!(
                        "failed to read terminal input: {}",
                        e
                    ))
                })? {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    match key.code {
                        KeyCode::Enter => return Ok(()),
                        KeyCode::Esc => return Err(NonRobotSearchError::Cancelled),
                        _ => {}
                    }
                }
            }
        }
    }

    let title = "ShadowCrawl — Screen Access";
    let message = format!(
        "ShadowCrawl will open a visible browser window and navigate to:\n\n{}\n\nYou may need to complete CAPTCHA/login manually in the browser.\n\nClick OK to continue or Cancel to abort.\n\nEmergency stop: hold ESC for ~3 seconds.",
        cfg.url
    );

    // Otherwise use a blocking desktop dialog. This MUST wait for user input.
    // If desktop dialogs fail (headless / no portal), fall back to TTY when possible.
    info!(
        "non_robot_search: waiting for OS consent dialog (mode={}, auto_allow={})",
        consent_mode,
        auto_allow
    );

    match blocking_ok_cancel_dialog(title, &message) {
        Ok(()) => Ok(()),
        Err(NonRobotSearchError::Cancelled) => Err(NonRobotSearchError::Cancelled),
        Err(e @ NonRobotSearchError::AutomationFailed(_)) => {
            if stdin_is_tty && !force_dialog {
                warn!("non_robot_search: GUI consent failed; falling back to TTY prompt: {}", e);
                return notify_and_prompt_user_tty();
            }
            Err(NonRobotSearchError::InteractiveRequired)
        }
        Err(other) => Err(other),
    }
}

#[cfg(feature = "non_robot_search")]
fn notify_and_prompt_user_tty() -> Result<(), NonRobotSearchError> {
    println!(
        "ShadowCrawl needs screen access for high-fidelity rendering. Press [Enter] to allow or [Esc] to cancel."
    );

    loop {
        if event::poll(Duration::from_millis(100)).map_err(|e| {
            NonRobotSearchError::AutomationFailed(format!("failed to poll terminal input: {}", e))
        })? {
            if let TermEvent::Key(key) = event::read().map_err(|e| {
                NonRobotSearchError::AutomationFailed(format!("failed to read terminal input: {}", e))
            })? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match key.code {
                    KeyCode::Enter => return Ok(()),
                    KeyCode::Esc => return Err(NonRobotSearchError::Cancelled),
                    _ => {}
                }
            }
        }
    }
}

#[cfg(feature = "non_robot_search")]
fn blocking_ok_cancel_dialog(title: &str, message: &str) -> Result<(), NonRobotSearchError> {
    #[cfg(target_os = "macos")]
    {
        return macos_osascript_ok_cancel(title, message);
    }

    #[cfg(target_os = "windows")]
    {
        // Prefer a TopMost native dialog to avoid “popup disappeared behind other windows”.
        if let Ok(()) = windows_powershell_ok_cancel(title, message) {
            return Ok(());
        }

        // Fallback to rfd if PowerShell is unavailable.
        return rfd_ok_cancel(title, message);
    }

    #[cfg(target_os = "linux")]
    {
        // Prefer desktop dialogs (zenity/kdialog/xmessage) because they reliably block.
        if let Ok(()) = linux_gui_ok_cancel(title, message) {
            return Ok(());
        }

        // Fallback to rfd portal dialog.
        return rfd_ok_cancel(title, message);
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        rfd_ok_cancel(title, message)
    }
}

#[cfg(all(feature = "non_robot_search", not(target_os = "macos")))]
fn rfd_ok_cancel(title: &str, message: &str) -> Result<(), NonRobotSearchError> {
    // rfd may panic in some environments; treat that as AutomationFailed.
    let res = std::panic::catch_unwind(|| {
        MessageDialog::new()
            .set_level(MessageLevel::Warning)
            .set_title(title)
            .set_description(message)
            .set_buttons(MessageButtons::OkCancel)
            .show()
    });

    match res {
        Ok(rfd::MessageDialogResult::Ok) | Ok(rfd::MessageDialogResult::Yes) => Ok(()),
        Ok(_) => Err(NonRobotSearchError::Cancelled),
        Err(_) => Err(NonRobotSearchError::AutomationFailed(
            "GUI dialog failed (panic)".to_string(),
        )),
    }
}

#[cfg(all(feature = "non_robot_search", target_os = "windows"))]
fn windows_powershell_ok_cancel(title: &str, message: &str) -> Result<(), NonRobotSearchError> {
    // Use PowerShell + WinForms to guarantee a blocking, TopMost prompt.
    // This avoids situations where rfd dialogs are hidden behind other windows.
    let esc_ps_single = |s: &str| s.replace('\'', "''");
    let title_esc = esc_ps_single(title);
    let message_esc = esc_ps_single(message);

    let script = format!(
        "$ErrorActionPreference = 'Stop';\n"
            + "Add-Type -AssemblyName System.Windows.Forms;\n"
            + "$form = New-Object System.Windows.Forms.Form;\n"
            + "$form.TopMost = $true;\n"
            + "$form.WindowState = 'Minimized';\n"
            + "$form.StartPosition = 'CenterScreen';\n"
            + "$form.Width = 1; $form.Height = 1;\n"
            + "$form.Show(); $form.Activate() | Out-Null;\n"
            + "$msg = '{}';\n"
            + "$ttl = '{}';\n"
            + "$result = [System.Windows.Forms.MessageBox]::Show($form, $msg, $ttl, [System.Windows.Forms.MessageBoxButtons]::OKCancel, [System.Windows.Forms.MessageBoxIcon]::Warning);\n"
            + "if ($result -eq [System.Windows.Forms.DialogResult]::OK) {{ exit 0 }} else {{ exit 1 }}\n",
        message_esc,
        title_esc
    );

    let run_ps = |exe: &str| {
        std::process::Command::new(exe)
            .arg("-NoProfile")
            .arg("-ExecutionPolicy")
            .arg("Bypass")
            .arg("-Command")
            .arg(&script)
            .status()
    };

    // Prefer Windows PowerShell (inbox) first, then PowerShell 7 (pwsh) as fallback.
    let status = run_ps("powershell.exe").or_else(|_| run_ps("powershell")).or_else(|_| run_ps("pwsh"));

    match status {
        Ok(st) if st.success() => Ok(()),
        Ok(_) => Err(NonRobotSearchError::Cancelled),
        Err(e) => Err(NonRobotSearchError::AutomationFailed(format!(
            "failed to spawn PowerShell dialog (powershell.exe/powershell/pwsh): {}",
            e
        ))),
    }
}

#[cfg(all(feature = "non_robot_search", target_os = "linux"))]
fn linux_gui_ok_cancel(title: &str, message: &str) -> Result<(), NonRobotSearchError> {
    // Best-effort desktop prompts. Prefer zenity (GNOME), then kdialog (KDE), then xmessage.
    let try_status = |cmd: &str, args: &[&str]| -> Option<std::process::ExitStatus> {
        std::process::Command::new(cmd).args(args).status().ok()
    };

    if let Some(status) = try_status(
        "zenity",
        &[
            "--question",
            "--title",
            title,
            "--text",
            message,
            "--ok-label",
            "OK",
            "--cancel-label",
            "Cancel",
        ],
    ) {
        return if status.success() {
            Ok(())
        } else {
            Err(NonRobotSearchError::Cancelled)
        };
    }

    if let Some(status) = try_status("kdialog", &["--title", title, "--yesno", message]) {
        return if status.success() {
            Ok(())
        } else {
            Err(NonRobotSearchError::Cancelled)
        };
    }

    if let Some(status) = try_status(
        "xmessage",
        &["-center", "-buttons", "OK:0,Cancel:1", message],
    ) {
        return if status.success() {
            Ok(())
        } else {
            Err(NonRobotSearchError::Cancelled)
        };
    }

    Err(NonRobotSearchError::AutomationFailed(
        "no supported linux desktop dialog found (zenity/kdialog/xmessage)".to_string(),
    ))
}

#[cfg(feature = "non_robot_search")]
fn get_prelaunch_overlay_script(target_url: &str) -> String {
        // NOTE: This overlay is a UX notice (non-blocking). It improves operator clarity,
        // especially when consent is auto-allowed or when tools are invoked via curl.
        let url_json = serde_json::to_string(target_url).unwrap_or_else(|_| "\"\"".to_string());
        format!(
                r#"(() => {{
    try {{
        const id = '__shadowcrawl_prelaunch_overlay__';
        if (document.getElementById(id)) return;

        const targetUrl = {url_json};

        const overlay = document.createElement('div');
        overlay.id = id;
        overlay.style.position = 'fixed';
        overlay.style.left = '0';
        overlay.style.top = '0';
        overlay.style.right = '0';
        overlay.style.bottom = '0';
        overlay.style.zIndex = '2147483647';
        overlay.style.display = 'flex';
        overlay.style.alignItems = 'center';
        overlay.style.justifyContent = 'center';
        overlay.style.background = 'rgba(0, 0, 0, 0.55)';
        overlay.style.backdropFilter = 'blur(6px)';
        overlay.style.webkitBackdropFilter = 'blur(6px)';

        const card = document.createElement('div');
        card.style.width = 'min(720px, calc(100vw - 48px))';
        card.style.background = 'rgba(20, 20, 24, 0.96)';
        card.style.border = '1px solid rgba(255, 255, 255, 0.12)';
        card.style.borderRadius = '16px';
        card.style.boxShadow = '0 20px 60px rgba(0,0,0,0.55)';
        card.style.color = '#fff';
        card.style.fontFamily = 'system-ui, -apple-system, Segoe UI, Roboto, Helvetica, Arial, sans-serif';
        card.style.padding = '20px 20px 16px 20px';

        const h = document.createElement('div');
        h.textContent = 'ShadowCrawl — HITL Rendering';
        h.style.fontSize = '20px';
        h.style.fontWeight = '800';
        h.style.letterSpacing = '0.2px';

        const sub = document.createElement('div');
        sub.textContent = 'A visible browser will open and navigate to your target.';
        sub.style.marginTop = '6px';
        sub.style.opacity = '0.92';

        const url = document.createElement('div');
        url.textContent = targetUrl ? ('Target: ' + targetUrl) : 'Target: (unknown)';
        url.style.marginTop = '12px';
        url.style.padding = '10px 12px';
        url.style.borderRadius = '12px';
        url.style.background = 'rgba(255, 255, 255, 0.06)';
        url.style.border = '1px solid rgba(255, 255, 255, 0.10)';
        url.style.fontFamily = 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace';
        url.style.fontSize = '12px';
        url.style.wordBreak = 'break-all';

        const ul = document.createElement('ul');
        ul.style.marginTop = '14px';
        ul.style.paddingLeft = '18px';
        ul.style.lineHeight = '1.55';
        ul.style.opacity = '0.95';
        ul.innerHTML = [
            '<li>If a site shows a CAPTCHA/login, complete it manually in the browser.</li>',
            '<li>When ready, click <b>FINISH &amp; RETURN</b> to extract immediately.</li>',
            '<li>Emergency stop: hold <b>Esc</b> for ~3 seconds.</li>'
        ].join('');

        const actions = document.createElement('div');
        actions.style.display = 'flex';
        actions.style.justifyContent = 'flex-end';
        actions.style.gap = '10px';
        actions.style.marginTop = '16px';

        const btn = document.createElement('button');
        btn.textContent = 'Continue';
        btn.style.cursor = 'pointer';
        btn.style.padding = '10px 14px';
        btn.style.borderRadius = '12px';
        btn.style.border = '1px solid rgba(255,255,255,0.16)';
        btn.style.background = 'rgba(255,255,255,0.10)';
        btn.style.color = '#fff';
        btn.style.fontWeight = '700';
        btn.onclick = () => overlay.remove();

        actions.appendChild(btn);

        card.appendChild(h);
        card.appendChild(sub);
        card.appendChild(url);
        card.appendChild(ul);
        card.appendChild(actions);
        overlay.appendChild(card);
        document.body.appendChild(overlay);

        // Auto-hide after 10s to avoid blocking the operator.
        setTimeout(() => {{
            const el = document.getElementById(id);
            if (el) el.remove();
        }}, 10000);
    }} catch (e) {{
        // ignore
    }}
}})()"#
        )
}

#[cfg(all(feature = "non_robot_search", target_os = "macos"))]
fn macos_osascript_ok_cancel(title: &str, message: &str) -> Result<(), NonRobotSearchError> {
    // RFD on macOS panics when called from non-main threads in some environments (e.g. servers).
    // AppleScript dialog is a reliable fallback that can be spawned from any thread.
    let esc_as = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");

    // Use System Events + activate to bring the dialog to the foreground (so users actually see it).
    // NOTE: braces must be escaped for Rust format!.
    let script = format!(
        r#"tell application "System Events"
activate
display dialog "{}" with title "{}" buttons {{"Cancel", "OK"}} default button "OK" with icon caution
end tell"#,
        esc_as(message),
        esc_as(title)
    );

    let output = std::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output();

    match output {
        Ok(out) if out.status.success() => Ok(()),
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            // osascript typically reports cancel as "User canceled.".
            if stderr.to_lowercase().contains("user canceled") || stderr.to_lowercase().contains("user cancelled") {
                Err(NonRobotSearchError::Cancelled)
            } else {
                Err(NonRobotSearchError::AutomationFailed(format!(
                    "macOS consent dialog failed (osascript): {}",
                    stderr.trim()
                )))
            }
        }
        Err(e) => Err(NonRobotSearchError::AutomationFailed(format!(
            "failed to spawn macOS dialog (osascript): {}",
            e
        ))),
    }
}

#[cfg(feature = "non_robot_search")]
async fn request_human_help(
    session: &BrowserSession,
    input_controller: &dyn InputController,
) -> Result<(), NonRobotSearchError> {
    warn!("non_robot_search: verification gate detected; requesting human help");
    play_tone(Tone::Urgent);

    // Big overlay inside the page.
    let _ = session.page.evaluate(
        r#"(() => {
            const id = '__shadowcrawl_hitl_overlay__';
            if (document.getElementById(id)) return;
            const div = document.createElement('div');
            div.id = id;
            div.style.position = 'fixed';
            div.style.left = '0';
            div.style.top = '0';
            div.style.right = '0';
            div.style.zIndex = '2147483647';
            div.style.padding = '24px';
            div.style.fontSize = '28px';
            div.style.fontWeight = '800';
            div.style.background = 'rgba(0,0,0,0.88)';
            div.style.color = 'white';
            div.style.textAlign = 'center';
            div.style.borderBottom = '4px solid #ff4444';
            div.textContent = '🚨 SHADOWCRAWL NEEDS HELP: Please complete the verification step in this window.';
            document.documentElement.appendChild(div);
        })()"#,
    ).await;

    // Temporarily unlock inputs for HITL.
    input_controller.unlock().ok();
    Ok(())
}

#[cfg(feature = "non_robot_search")]
async fn wait_for_human_resolution(
    session: &BrowserSession,
    timeout: Duration,
    abort_rx: &mut watch::Receiver<bool>,
) -> Result<(), NonRobotSearchError> {
    let start = Instant::now();
    loop {
        abort_if_needed(abort_rx)?;
        if session.is_closed() {
            return Err(NonRobotSearchError::BrowserClosed);
        }

        // 🚀 Check if user clicked manual return button
        if check_manual_return_triggered(&session.page).await {
            info!("non_robot_search: 🚀 Manual return button clicked during human resolution wait");
            play_tone(Tone::Success);
            // Remove overlay
            let _ = session.page.evaluate(
                r#"(() => { const el = document.getElementById('__shadowcrawl_hitl_overlay__'); if (el) el.remove(); })()"#,
            ).await;
            return Ok(());
        }

        // Consider it resolved if verification markers are gone and the page is no longer an interstitial.
        let challenged = detect_challenge_dom(&session.page).await.unwrap_or(false)
            || detect_interstitial_like(&session.page)
                .await
                .unwrap_or(false);
        if !challenged {
            play_tone(Tone::Success);
            // Remove overlay.
            let _ = session.page.evaluate(
                r#"(() => { const el = document.getElementById('__shadowcrawl_hitl_overlay__'); if (el) el.remove(); })()"#,
            ).await;
            return Ok(());
        }

        if start.elapsed() >= timeout {
            return Err(NonRobotSearchError::HumanUnavailable);
        }

        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

#[cfg(feature = "non_robot_search")]
async fn human_like_idle(abort_rx: &mut watch::Receiver<bool>) -> Result<(), NonRobotSearchError> {
    abort_if_needed(abort_rx)?;
    let idle_ms = {
        let mut rng = rand::rng();
        Uniform::new(800u64, 2000).unwrap().sample(&mut rng)
    };
    tokio::time::sleep(Duration::from_millis(idle_ms)).await;
    Ok(())
}

#[cfg(feature = "non_robot_search")]
async fn human_like_scroll(
    page: &chromiumoxide::Page,
    abort_rx: &mut watch::Receiver<bool>,
) -> Result<(), NonRobotSearchError> {
    let actions: Vec<(u16, u64)> = {
        let mut rng = rand::rng();
        let passes = Uniform::new(2usize, 5).unwrap().sample(&mut rng);
        let scroll_dist = Uniform::new(180u16, 620).unwrap();
        let pause_dist = Uniform::new(250u64, 1200).unwrap();
        (0..passes)
            .map(|_| (scroll_dist.sample(&mut rng), pause_dist.sample(&mut rng)))
            .collect()
    };

    for (px, pause_ms) in actions {
        abort_if_needed(abort_rx)?;
        let _ = page
            .evaluate(format!(
                "window.scrollBy({{top: {}, behavior: 'smooth'}});",
                px
            ))
            .await;
        tokio::time::sleep(Duration::from_millis(pause_ms)).await;
    }
    Ok(())
}

#[cfg(feature = "non_robot_search")]
async fn maybe_human_like_mouse(
    page: &chromiumoxide::Page,
    abort_rx: &mut watch::Receiver<bool>,
) -> Result<(), NonRobotSearchError> {
    // chromiumoxide doesn't expose full OS cursor moves; we do lightweight DOM reads to mimic "attention".
    let points: Vec<(i32, i32)> = {
        let mut rng = rand::rng();
        let moves = Uniform::new(3usize, 7).unwrap().sample(&mut rng);
        let x_dist = Uniform::new(80i32, 900).unwrap();
        let y_dist = Uniform::new(80i32, 700).unwrap();
        (0..moves)
            .map(|_| (x_dist.sample(&mut rng), y_dist.sample(&mut rng)))
            .collect()
    };

    for (x, y) in points {
        abort_if_needed(abort_rx)?;
        let _ = page
            .evaluate(format!("document.elementFromPoint({}, {})?.tagName", x, y))
            .await;
        tokio::time::sleep(Duration::from_millis(80 + (x as u64 % 120))).await;
    }
    Ok(())
}

#[cfg(feature = "non_robot_search")]
async fn detect_challenge_dom(page: &chromiumoxide::Page) -> anyhow::Result<bool> {
    // DOM-based detection (more reliable than HTML alone for dynamic challenges).
    let js = r#"(() => {
        const title = (document.title || '').toLowerCase();
        const iframes = Array.from(document.querySelectorAll('iframe'));
        const srcs = iframes.map(i => (i.getAttribute('src') || '') + ' ' + (i.getAttribute('title') || '')).join(' ').toLowerCase();
        const body = (document.body?.innerText || '').toLowerCase();
        const hasIframe = srcs.includes('challenge')
            || srcs.includes('verify')
            || srcs.includes('verification');
        const hasText = body.includes('checking your browser')
            || body.includes('verification')
            || body.includes('please wait');
        const hasTitle = title.includes('challenge')
            || title.includes('verification')
            || title.includes('just a moment')
            || title.includes('access denied')
            || title.includes('denied');
        return Boolean(hasIframe || hasText || hasTitle);
    })()"#;

    let val = page.evaluate(js).await?;
    Ok(val.into_value::<bool>().unwrap_or(false))
}

#[cfg(feature = "non_robot_search")]
async fn detect_interstitial_like(page: &chromiumoxide::Page) -> anyhow::Result<bool> {
    // Generic heuristics for "holding" pages: very low content, common title patterns, or explicit wait language.
    // This is used to avoid extracting prematurely when the visible page isn't the target content yet.
    let js = r#"(() => {
        const title = (document.title || '').toLowerCase();
        const bodyText = (document.body?.innerText || '').trim();
        const body = bodyText.toLowerCase();
        const bodyLen = bodyText.length;

        const titleLooksHolding = title.includes('just a moment')
            || title.includes('access denied')
            || title.includes('denied')
            || title.includes('verification')
            || title.includes('please wait');

        const textLooksHolding = body.includes('please wait')
            || body.includes('checking your browser')
            || body.includes('press & hold')
            || body.includes('press and hold');

        const tooShort = bodyLen > 0 && bodyLen < 220;
        return Boolean(titleLooksHolding || textLooksHolding || tooShort);
    })()"#;

    let val = page.evaluate(js).await?;
    Ok(val.into_value::<bool>().unwrap_or(false))
}

#[cfg(feature = "non_robot_search")]
fn abort_if_needed(abort_rx: &mut watch::Receiver<bool>) -> Result<(), NonRobotSearchError> {
    if *abort_rx.borrow() {
        return Err(NonRobotSearchError::EmergencyAbort);
    }
    Ok(())
}

#[cfg(feature = "non_robot_search")]
struct BrowserSession {
    browser: Browser,
    page: chromiumoxide::Page,
    handler_task: tokio::task::JoinHandle<()>,
    closed: Arc<AtomicBool>,
    proxy: Option<String>,
    profile_dir: Option<std::path::PathBuf>,
    profile_name: Option<String>,
    created_profile_dir: bool,
    debugging_port: u16,
}

#[cfg(feature = "non_robot_search")]
impl Drop for BrowserSession {
    fn drop(&mut self) {
        // Force-kill browser process on drop to prevent zombie processes
        info!("non_robot_search: BrowserSession drop - force-killing browser on port {}", self.debugging_port);
        force_kill_all_debug_browsers(self.debugging_port);
        
        // Clean up temp profile if created
        if self.created_profile_dir {
            if let Some(dir) = self.profile_dir.take() {
                let _ = std::fs::remove_dir_all(dir);
            }
        }
    }
}

#[cfg(feature = "non_robot_search")]
impl BrowserSession {
    async fn launch(proxy: Option<&str>, user_profile_path: Option<&str>) -> anyhow::Result<Self> {
        let (profile_dir, profile_name, created_profile_dir) =
            resolve_profile_dir(user_profile_path)?;

        // Fixed port for consistency.
        let debugging_port: u16 = 9222;

        // Best-effort cleanup to avoid Chrome profile lock issues:
        // - Only kill our prior automation-launched browser (identified by remote-debugging-port).
        // - Only remove SingletonLock if it's stale and no active process appears to be using the profile.
        if let Some(dir) = profile_dir.as_ref() {
            kill_debug_browser_zombies(debugging_port, dir);
            remove_stale_singleton_lock(dir);
        } else {
            kill_debug_browser_zombies(debugging_port, std::path::Path::new(""));
        }

        let chrome_exe = find_chrome_executable().ok_or_else(|| {
            anyhow!("Browser executable not found (tried Brave, Chrome, Chromium)")
        })?;

        // Minimal flags - keep it as close to a normal user browser as possible.
        let mut args = vec![
            format!("--remote-debugging-port={}", debugging_port),
            "--disable-infobars".to_string(),
            "--no-first-run".to_string(),
            "--no-default-browser-check".to_string(),
        ];

        if let Some(proxy_url) = proxy {
            args.push(format!("--proxy-server={}", proxy_url));
        }

        if let Some(dir) = profile_dir.as_ref() {
            args.push(format!("--user-data-dir={}", dir.display()));
        }

        if let Some(name) = profile_name.as_ref() {
            args.push(format!("--profile-directory={}", name));
        }

        info!("non_robot_search: launching Brave with user profile (human-centric mode)");
        let _chrome_process = std::process::Command::new(&chrome_exe)
            .args(&args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| anyhow!("Failed to spawn Chrome: {}", e))?;

        // Give Chrome time to start and open the debugging port (increased wait)
        info!("non_robot_search: waiting for Chrome to start...");
        tokio::time::sleep(Duration::from_millis(6000)).await;

        // Connect to Chrome via CDP websocket - discover the endpoint via JSON API
        let json_url = format!("http://127.0.0.1:{}/json/version", debugging_port);
        let mut last_error = None;
        let mut browser_opt = None;

        for attempt in 1..=5 {
            // First, try to get the WebSocket debugger URL from Chrome
            let ws_url_result: anyhow::Result<String> = async {
                let response = reqwest::get(&json_url)
                    .await
                    .map_err(|e| anyhow!("HTTP request failed: {}", e))?;
                let json: serde_json::Value = response
                    .json()
                    .await
                    .map_err(|e| anyhow!("JSON parse failed: {}", e))?;
                json["webSocketDebuggerUrl"]
                    .as_str()
                    .ok_or_else(|| anyhow!("No webSocketDebuggerUrl in response"))
                    .map(|s| s.to_string())
            }
            .await;

            match ws_url_result {
                Ok(ws_url) => {
                    info!("non_robot_search: discovered CDP endpoint: {}", ws_url);
                    match Browser::connect(ws_url).await {
                        Ok((b, h)) => {
                            browser_opt = Some((b, h));
                            break;
                        }
                        Err(e) => {
                            last_error = Some(anyhow!("Browser connect failed: {}", e));
                        }
                    }
                }
                Err(e) => {
                    last_error = Some(e);
                }
            }

            if attempt < 5 {
                info!(
                    "non_robot_search: CDP connection attempt {} failed, retrying...",
                    attempt
                );
                tokio::time::sleep(Duration::from_millis(2000)).await;
            }
        }

        let (browser, handler) = browser_opt.ok_or_else(|| {
            anyhow!(
                "Failed to connect to Chrome after 5 attempts. Last error: {:?}",
                last_error
            )
        })?;

        let closed = Arc::new(AtomicBool::new(false));
        let handler_task = spawn_handler_task(handler, Arc::clone(&closed));

        // Create our single working tab with a recognizable title so we can close any extra tabs
        // (Brave often opens a default New Tab on startup, which users see as a second tab).
        let page = browser
            .new_page("data:text/html,<title>ShadowCrawl</title>")
            .await?;

        // Best-effort: close every other page tab, keeping only our ShadowCrawl tab.
        let _ = close_extra_tabs_via_json(debugging_port, "ShadowCrawl").await;

        // 🚨 INJECT CDP STEALTH SCRIPT BEFORE ANY NAVIGATION
        info!("non_robot_search: injecting CDP stealth script to bypass navigator.webdriver detection");
        let stealth_script = get_stealth_script();
        page.execute(
            chromiumoxide::cdp::browser_protocol::page::AddScriptToEvaluateOnNewDocumentParams::new(
                stealth_script,
            ),
        )
        .await
        .map_err(|e| anyhow!("Failed to inject stealth script: {}", e))?;

        // 🚀 INJECT MANUAL RETURN BUTTON
        info!("non_robot_search: injecting manual return button for user control");
        let button_script = get_manual_return_button_script();
        page.execute(
            chromiumoxide::cdp::browser_protocol::page::AddScriptToEvaluateOnNewDocumentParams::new(
                button_script,
            ),
        )
        .await
        .map_err(|e| anyhow!("Failed to inject manual return button: {}", e))?;

        Ok(Self {
            browser,
            page,
            handler_task,
            closed,
            proxy: proxy.map(|s| s.to_string()),
            profile_dir,
            profile_name,
            created_profile_dir,
            debugging_port,
        })
    }

    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    async fn close(&mut self) {
        info!("non_robot_search: closing browser session (port {})", self.debugging_port);
        
        // Close tabs first (more "human" shutdown), reducing Brave's "Restore tabs?" prompt.
        let _ = close_all_tabs_via_json(self.debugging_port).await;

        // Then close the CDP connection/browser gracefully.
        let _ = self.browser.close().await;
        let _ = self.browser.wait().await;
        self.handler_task.abort();
        
        // Wait briefly for graceful shutdown
        tokio::time::sleep(Duration::from_millis(500)).await;
        
        // Force-kill any remaining browser processes
        force_kill_all_debug_browsers(self.debugging_port);

        if self.created_profile_dir {
            if let Some(dir) = self.profile_dir.take() {
                let _ = std::fs::remove_dir_all(dir);
            }
        }
    }

    async fn relaunch(&mut self) -> anyhow::Result<()> {
        // Best effort: close existing browser and abort handler.
        let _ = close_all_tabs_via_json(self.debugging_port).await;
        let _ = self.browser.close().await;
        let _ = self.browser.wait().await;
        self.handler_task.abort();

        self.closed.store(false, Ordering::SeqCst);

        // Fixed port for consistency.
        let debugging_port: u16 = 9222;

        // Same best-effort cleanup as initial launch.
        if let Some(dir) = self.profile_dir.as_ref() {
            kill_debug_browser_zombies(debugging_port, dir);
            remove_stale_singleton_lock(dir);
        } else {
            kill_debug_browser_zombies(debugging_port, std::path::Path::new(""));
        }

        let chrome_exe = find_chrome_executable().ok_or_else(|| {
            anyhow!("Browser executable not found (tried Brave, Chrome, Chromium)")
        })?;

        // Minimal flags - keep it as close to a normal user browser as possible.
        let mut args = vec![
            format!("--remote-debugging-port={}", debugging_port),
            "--disable-infobars".to_string(),
            "--no-first-run".to_string(),
            "--no-default-browser-check".to_string(),
        ];

        if let Some(proxy_url) = self.proxy.as_ref() {
            args.push(format!("--proxy-server={}", proxy_url));
        }

        if let Some(dir) = self.profile_dir.as_ref() {
            args.push(format!("--user-data-dir={}", dir.display()));
        }

        if let Some(name) = self.profile_name.as_ref() {
            args.push(format!("--profile-directory={}", name));
        }

        info!("non_robot_search: relaunching Brave with user profile (human-centric mode)");
        let _chrome_process = std::process::Command::new(&chrome_exe)
            .args(&args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| anyhow!("Failed to spawn Chrome: {}", e))?;

        // Give Chrome time to start (increased wait)
        info!("non_robot_search: waiting for Chrome to restart...");
        tokio::time::sleep(Duration::from_millis(6000)).await;

        // Connect to Chrome via CDP websocket - discover the endpoint via JSON API
        let json_url = format!("http://127.0.0.1:{}/json/version", debugging_port);
        let mut last_error = None;
        let mut browser_opt = None;

        for attempt in 1..=5 {
            // First, try to get the WebSocket debugger URL from Chrome
            let ws_url_result: anyhow::Result<String> = async {
                let response = reqwest::get(&json_url)
                    .await
                    .map_err(|e| anyhow!("HTTP request failed: {}", e))?;
                let json: serde_json::Value = response
                    .json()
                    .await
                    .map_err(|e| anyhow!("JSON parse failed: {}", e))?;
                json["webSocketDebuggerUrl"]
                    .as_str()
                    .ok_or_else(|| anyhow!("No webSocketDebuggerUrl in response"))
                    .map(|s| s.to_string())
            }
            .await;

            match ws_url_result {
                Ok(ws_url) => {
                    info!(
                        "non_robot_search: discovered CDP endpoint on relaunch: {}",
                        ws_url
                    );
                    match Browser::connect(ws_url).await {
                        Ok((b, h)) => {
                            browser_opt = Some((b, h));
                            break;
                        }
                        Err(e) => {
                            last_error = Some(anyhow!("Browser reconnect failed: {}", e));
                        }
                    }
                }
                Err(e) => {
                    last_error = Some(e);
                }
            }

            if attempt < 5 {
                info!(
                    "non_robot_search: CDP reconnection attempt {} failed, retrying...",
                    attempt
                );
                tokio::time::sleep(Duration::from_millis(2000)).await;
            }
        }

        let (browser, handler) = browser_opt.ok_or_else(|| {
            anyhow!(
                "Failed to reconnect to Chrome after 5 attempts. Last error: {:?}",
                last_error
            )
        })?;

        let handler_task = spawn_handler_task(handler, Arc::clone(&self.closed));
        let page = browser
            .new_page("data:text/html,<title>ShadowCrawl</title>")
            .await?;

        // Best-effort: close every other page tab.
        let _ = close_extra_tabs_via_json(debugging_port, "ShadowCrawl").await;

        // 🚨 INJECT CDP STEALTH SCRIPT BEFORE ANY NAVIGATION (RELAUNCH)
        info!("non_robot_search: injecting CDP stealth script on relaunch");
        let stealth_script = get_stealth_script();
        page.execute(
            chromiumoxide::cdp::browser_protocol::page::AddScriptToEvaluateOnNewDocumentParams::new(
                stealth_script,
            ),
        )
        .await
        .map_err(|e| anyhow!("Failed to inject stealth script on relaunch: {}", e))?;

        // 🚀 INJECT MANUAL RETURN BUTTON (RELAUNCH)
        info!("non_robot_search: injecting manual return button on relaunch");
        let button_script = get_manual_return_button_script();
        page.execute(
            chromiumoxide::cdp::browser_protocol::page::AddScriptToEvaluateOnNewDocumentParams::new(
                button_script,
            ),
        )
        .await
        .map_err(|e| anyhow!("Failed to inject manual return button on relaunch: {}", e))?;

        self.browser = browser;
        self.page = page;
        self.handler_task = handler_task;
        self.debugging_port = debugging_port;
        Ok(())
    }
}

#[cfg(feature = "non_robot_search")]
async fn close_extra_tabs_via_json(debugging_port: u16, keep_title: &str) -> anyhow::Result<()> {
    // Use the Chrome/Brave /json endpoint because it reliably enumerates targets without requiring
    // chromiumoxide-internal APIs.
    let list_url = format!("http://127.0.0.1:{}/json/list", debugging_port);
    let close_base = format!("http://127.0.0.1:{}/json/close/", debugging_port);

    let resp = reqwest::get(&list_url).await?;
    let targets: serde_json::Value = resp.json().await?;
    let arr = targets.as_array().cloned().unwrap_or_default();

    // Close all "page" targets except the one with our keep_title.
    for t in arr {
        let ttype = t.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if ttype != "page" {
            continue;
        }
        let title = t.get("title").and_then(|v| v.as_str()).unwrap_or("");
        if title == keep_title {
            continue;
        }
        let id = t.get("id").and_then(|v| v.as_str()).unwrap_or("");
        if id.is_empty() {
            continue;
        }
        let _ = reqwest::get(format!("{}{}", close_base, id)).await;
    }

    Ok(())
}

#[cfg(feature = "non_robot_search")]
async fn close_all_tabs_via_json(debugging_port: u16) -> anyhow::Result<()> {
    let list_url = format!("http://127.0.0.1:{}/json/list", debugging_port);
    let close_base = format!("http://127.0.0.1:{}/json/close/", debugging_port);

    let resp = reqwest::get(&list_url).await?;
    let targets: serde_json::Value = resp.json().await?;
    let arr = targets.as_array().cloned().unwrap_or_default();

    for t in arr {
        let ttype = t.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if ttype != "page" {
            continue;
        }
        let id = t.get("id").and_then(|v| v.as_str()).unwrap_or("");
        if id.is_empty() {
            continue;
        }
        let _ = reqwest::get(format!("{}{}", close_base, id)).await;
    }

    Ok(())
}

#[cfg(feature = "non_robot_search")]
fn force_kill_all_debug_browsers(debugging_port: u16) {
    // Aggressively kill ALL browser processes using this debugging port.
    // Uses sysinfo for cross-platform support (Windows/macOS/Linux).
    use sysinfo::System;
    let marker = format!("--remote-debugging-port={}", debugging_port);

    let mut sys = System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

    let mut killed = 0u32;
    for (_pid, proc_) in sys.processes() {
        let cmd_line = proc_
            .cmd()
            .iter()
            .map(|s| s.to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join(" ");
        if cmd_line.contains(&marker) {
            proc_.kill();
            killed += 1;
        }
    }

    if killed > 0 {
        info!(
            "non_robot_search: force-killed {} browser process(es) with CDP port {}",
            killed, debugging_port
        );
    }
}

#[cfg(feature = "non_robot_search")]
fn kill_debug_browser_zombies(debugging_port: u16, user_data_dir: &std::path::Path) {
    // Do not kill normal user browsers. Only target processes launched with our CDP debugging port.
    // If user_data_dir is non-empty, additionally require it to match.
    // Uses sysinfo for cross-platform support (Windows/macOS/Linux).
    use sysinfo::System;
    let marker = format!("--remote-debugging-port={}", debugging_port);
    let user_dir_marker = if user_data_dir.as_os_str().is_empty() {
        None
    } else {
        Some(format!("--user-data-dir={}", user_data_dir.display()))
    };

    let mut sys = System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

    let mut killed = 0u32;
    for (_pid, proc_) in sys.processes() {
        let cmd_line = proc_
            .cmd()
            .iter()
            .map(|s| s.to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join(" ");
        if !cmd_line.contains(&marker) {
            continue;
        }
        if let Some(ref udm) = user_dir_marker {
            if !cmd_line.contains(udm) {
                continue;
            }
        }
        proc_.kill();
        killed += 1;
    }

    if killed > 0 {
        info!(
            "non_robot_search: killed {} zombie browser process(es) with CDP marker {}",
            killed, marker
        );
    }
}

#[cfg(feature = "non_robot_search")]
fn remove_stale_singleton_lock(user_data_dir: &std::path::Path) {
    // Chrome/Brave lock files usually live in the user-data-dir root.
    // Only remove if:
    // - file exists
    // - it's older than a short grace window
    // - no active process appears to be using this user-data-dir
    let lock_path = user_data_dir.join("SingletonLock");
    let meta = match std::fs::metadata(&lock_path) {
        Ok(m) => m,
        Err(_) => return,
    };

    let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
    let age_ok = SystemTime::now()
        .duration_since(modified)
        .ok()
        .map(|d| d >= Duration::from_secs(120))
        .unwrap_or(false);

    if !age_ok {
        return;
    }

    // Cross-platform process check using sysinfo.
    use sysinfo::System;
    let udm = format!("--user-data-dir={}", user_data_dir.display());
    let mut sys = System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    for (_pid, proc_) in sys.processes() {
        let cmd_line = proc_
            .cmd()
            .iter()
            .map(|s| s.to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join(" ");
        if cmd_line.contains(&udm) {
            // Someone is using it; do not remove.
            return;
        }
    }

    match std::fs::remove_file(&lock_path) {
        Ok(_) => info!(
            "non_robot_search: removed stale SingletonLock at {}",
            lock_path.display()
        ),
        Err(e) => warn!(
            "non_robot_search: failed to remove stale SingletonLock at {}: {}",
            lock_path.display(),
            e
        ),
    }
}

#[cfg(feature = "non_robot_search")]
fn spawn_handler_task(
    mut handler: chromiumoxide::Handler,
    closed: Arc<AtomicBool>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(event) = handler.next().await {
            match event {
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!("chromiumoxide handler event error: {}", e);
                }
            }
        }
        closed.store(true, Ordering::SeqCst);
    })
}

#[cfg(feature = "non_robot_search")]
fn resolve_profile_dir(
    user_profile_path: Option<&str>,
) -> anyhow::Result<(Option<std::path::PathBuf>, Option<String>, bool)> {
    // Priority:
    // 1) Explicit tool argument user_profile_path
    // 2) Env var SHADOWCRAWL_RENDER_PROFILE_DIR
    // 3) Auto-created temp directory (per-run)

    let explicit = user_profile_path
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(expand_tilde);

    let env_dir = std::env::var("SHADOWCRAWL_RENDER_PROFILE_DIR")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(|s| expand_tilde(&s));

    if let Some(raw) = explicit.or(env_dir) {
        let path = std::path::PathBuf::from(&raw);

        // For Brave Browser: Use the real user profile directly for human-centric experience
        // Brave typically stores profiles at: ~/Library/Application Support/BraveSoftware/Brave-Browser/
        // Chrome typically stores at: ~/Library/Application Support/Google/Chrome/

        // If the path looks like a specific profile directory (e.g., .../Default),
        // map to --user-data-dir=parent and --profile-directory=basename.
        if path.is_dir() {
            let maybe_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string());
            let has_prefs = path.join("Preferences").is_file();
            if has_prefs {
                if let (Some(parent), Some(name)) = (path.parent(), maybe_name) {
                    return Ok((Some(parent.to_path_buf()), Some(name), false));
                }
            }
        }

        std::fs::create_dir_all(&path)?;
        return Ok((Some(path), None, false));
    }

    // Default: create a per-run temp directory so relaunch can preserve session state.
    let mut rng = rand::rng();
    let suffix: u64 = Uniform::new(1u64, u64::MAX).unwrap().sample(&mut rng);
    let dir = std::env::temp_dir().join(format!(
        "shadowcrawl-render-profile-{}-{}",
        std::process::id(),
        suffix
    ));
    std::fs::create_dir_all(&dir)?;
    Ok((Some(dir), None, true))
}

#[cfg(feature = "non_robot_search")]
fn expand_tilde(raw: &str) -> String {
    if let Some(rest) = raw.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest).to_string_lossy().to_string();
        }
    }
    raw.to_string()
}

#[cfg(feature = "non_robot_search")]
fn find_chrome_executable() -> Option<String> {
    if let Ok(p) = std::env::var("CHROME_EXECUTABLE") {
        if Path::new(&p).exists() {
            return Some(p);
        }
    }

    #[cfg(target_os = "macos")]
    {
        // Prioritize Brave Browser for better human-centric experience
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
            "/usr/bin/chromium",
            "/usr/bin/chromium-browser",
            "/usr/bin/google-chrome",
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

#[cfg(feature = "non_robot_search")]
fn get_manual_return_button_script() -> String {
    r#"
// ====== SHADOWCRAWL MANUAL RETURN BUTTON ======
// Gives user explicit control over when to finish scraping

(function() {
    // Prevent multiple injections
    if (window.__shadowcrawl_button_injected) return;
    window.__shadowcrawl_button_injected = true;
    
    // Signal flag for Rust to detect
    window.__shadowcrawl_manual_finish = false;
    
    // Create floating button
    const btn = document.createElement('button');
    btn.id = 'shadowcrawl-finish-btn';
    btn.innerHTML = '🚀 SHADOWCRAWL: FINISH & RETURN';
    btn.style.cssText = `
        position: fixed;
        top: auto;
        bottom: 14px;
        right: 10px;
        z-index: 2147483648;
        padding: 15px 20px;
        background: linear-gradient(135deg, #ff4757 0%, #ff6348 100%);
        color: white;
        border: none;
        border-radius: 8px;
        cursor: pointer;
        font-weight: bold;
        font-size: 14px;
        font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
        box-shadow: 0 4px 15px rgba(255, 71, 87, 0.4);
        transition: all 0.3s ease;
        text-shadow: 0 1px 2px rgba(0,0,0,0.2);
    `;
    
    // Hover effect
    btn.addEventListener('mouseenter', () => {
        btn.style.transform = 'scale(1.05)';
        btn.style.boxShadow = '0 6px 20px rgba(255, 71, 87, 0.6)';
    });
    btn.addEventListener('mouseleave', () => {
        btn.style.transform = 'scale(1)';
        btn.style.boxShadow = '0 4px 15px rgba(255, 71, 87, 0.4)';
    });
    
    // Click handler
    btn.addEventListener('click', () => {
        // Set signal flag
        window.__shadowcrawl_manual_finish = true;
        
        // Visual feedback
        btn.innerHTML = '✅ CAPTURING DATA...';
        btn.style.background = 'linear-gradient(135deg, #2ecc71 0%, #27ae60 100%)';
        btn.style.cursor = 'not-allowed';
        btn.disabled = true;
        
        // Notify user
        console.log('🚀 ShadowCrawl: Manual return triggered, extracting data now...');
    });
    
    // Inject into page
    function injectButton() {
        if (document.body) {
            document.body.appendChild(btn);
            console.log('🚀 ShadowCrawl: Manual return button ready (bottom-right corner)');
        } else {
            // Retry if body not ready
            setTimeout(injectButton, 100);
        }
    }
    
    // Wait for DOM ready
    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', injectButton);
    } else {
        injectButton();
    }
})();
"#
    .to_string()
}

#[cfg(feature = "non_robot_search")]
async fn check_manual_return_triggered(page: &chromiumoxide::Page) -> bool {
    // Check if user clicked the manual return button
    let check_script = "window.__shadowcrawl_manual_finish === true";
    
    match page.evaluate(check_script).await {
        Ok(result) => {
            result.into_value::<bool>().unwrap_or(false)
        }
        Err(_) => false,
    }
}

#[cfg(feature = "non_robot_search")]
fn get_stealth_script() -> String {
    r#"
// ====== CHROME 2026 STEALTH ENGINE ======
// Modern anti-detection - relies on JavaScript injection (flags are detected in 2026)

// 1. Chrome Runtime (CDP detection bypass)
if (!window.chrome) {
    window.chrome = {};
}
if (!window.chrome.runtime) {
    window.chrome.runtime = {
        connect: function() { return { onDisconnect: { addListener: function() {} }, postMessage: function() {} }; },
        sendMessage: function() {},
        onMessage: { addListener: function() {}, removeListener: function() {} },
    };
}
window.chrome.csi = function() { return { startE: Date.now(), onloadT: Date.now() + 100, pageT: Date.now() + 50, tran: 15 }; };
window.chrome.loadTimes = function() { return { requestTime: Date.now() / 1000, startLoadTime: Date.now() / 1000, finishDocumentLoadTime: (Date.now() + 500) / 1000, finishLoadTime: (Date.now() + 600) / 1000 }; };
window.chrome.app = { isInstalled: false, InstallState: { DISABLED: 'disabled', INSTALLED: 'installed', NOT_INSTALLED: 'not_installed' }, RunningState: { CANNOT_RUN: 'cannot_run', READY_TO_RUN: 'ready_to_run', RUNNING: 'running' } };

// 2. Navigator Overrides (CRITICAL - compensates for removed flag)
Object.defineProperty(navigator, 'webdriver', { get: () => false, configurable: true });
Object.defineProperty(navigator, 'languages', { get: () => ['en-US', 'en'], configurable: true });
Object.defineProperty(navigator, 'plugins', { get: () => [1, 2, 3, 4, 5], configurable: true });
Object.defineProperty(navigator, 'platform', { get: () => 'MacIntel', configurable: true });
Object.defineProperty(navigator, 'vendor', { get: () => 'Google Inc.', configurable: true });

// 3. Permissions Query (notification permission bypass)
const originalQuery = window.navigator.permissions && window.navigator.permissions.query;
if (originalQuery) {
    window.navigator.permissions.query = (parameters) => (
        parameters.name === 'notifications'
            ? Promise.resolve({ state: Notification.permission })
            : originalQuery(parameters)
    );
}

// 4. Canvas Fingerprint Noise Injection
const originalGetContext = HTMLCanvasElement.prototype.getContext;
HTMLCanvasElement.prototype.getContext = function(type, ...args) {
    const context = originalGetContext.apply(this, [type, ...args]);
    if (type === '2d' || type === 'webgl' || type === 'webgl2') {
        if (context) {
            const originalToDataURL = this.toDataURL;
            this.toDataURL = function(...args) {
                const data = originalToDataURL.apply(this, args);
                return data.replace(/.$/, String.fromCharCode(Math.random() * 10 | 0));
            };
        }
    }
    return context;
};

// 5. WebGL Vendor/Renderer Spoofing (SwiftShader masking)
const getParameter = WebGLRenderingContext.prototype.getParameter;
WebGLRenderingContext.prototype.getParameter = function(parameter) {
    if (parameter === 37445) return 'Intel Inc.';
    if (parameter === 37446) return 'Intel Iris OpenGL Engine';
    return getParameter.apply(this, arguments);
};

if (typeof WebGL2RenderingContext !== 'undefined') {
    const getParameter2 = WebGL2RenderingContext.prototype.getParameter;
    WebGL2RenderingContext.prototype.getParameter = function(parameter) {
        if (parameter === 37445) return 'Intel Inc.';
        if (parameter === 37446) return 'Intel Iris OpenGL Engine';
        return getParameter2.apply(this, arguments);
    };
}

// 6. Playwright/Puppeteer/Selenium Markers Cleanup
delete window.__playwright;
delete window.__puppeteer;
delete window.__selenium;
delete window.__webdriver_script_fn;
delete window.callPhantom;
delete window._phantom;
delete window.phantom;
delete window.__nightmare;
delete document.__selenium_unwrapped;
delete document.__webdriver_evaluate;
delete document.__driver_evaluate;
delete document.__webdriver_script_function;
delete document.__webdriver_script_func;
delete document.__fxdriver_evaluate;
delete document.__driver_unwrapped;
delete document.__fxdriver_unwrapped;
delete document.__selenium_evaluate;

// 7. User-Agent Data (Client Hints for Chrome 90+)
if (navigator.userAgentData) {
    Object.defineProperty(navigator, 'userAgentData', {
        get: () => ({
            brands: [
                { brand: 'Google Chrome', version: '131' },
                { brand: 'Chromium', version: '131' },
                { brand: 'Not_A Brand', version: '24' }
            ],
            mobile: false,
            platform: 'macOS'
        }),
        configurable: true
    });
}

// 8. Additional modern detection bypasses
Object.defineProperty(navigator, 'deviceMemory', { get: () => 8, configurable: true });
Object.defineProperty(navigator, 'hardwareConcurrency', { get: () => 8, configurable: true });
Object.defineProperty(navigator, 'maxTouchPoints', { get: () => 0, configurable: true });

// 9. Battery API spoof (often checked)
if (navigator.getBattery) {
    navigator.getBattery = async () => ({
        charging: true,
        chargingTime: 0,
        dischargingTime: Infinity,
        level: 1,
        addEventListener: function() {},
        removeEventListener: function() {},
        dispatchEvent: function() { return true; }
    });
}

"#
    .to_string()
}

#[cfg(feature = "non_robot_search")]
struct KillSwitch {
    stop: Arc<AtomicBool>,
}

#[cfg(feature = "non_robot_search")]
impl KillSwitch {
    fn start(abort_tx: watch::Sender<bool>) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = Arc::clone(&stop);
        std::thread::spawn(move || {
            let pressed_for_cb = Arc::new(AtomicBool::new(false));

            // rdev global listener; requires OS permissions on macOS (Accessibility).
            let callback = move |event: rdev::Event| {
                if stop_thread.load(Ordering::SeqCst) {
                    return;
                }
                if let rdev::EventType::KeyPress(rdev::Key::Escape) = event.event_type {
                    pressed_for_cb.store(true, Ordering::SeqCst);
                    let pressed_for_timer = Arc::clone(&pressed_for_cb);
                    let abort_tx = abort_tx.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(Duration::from_secs(3));
                        if pressed_for_timer.load(Ordering::SeqCst) {
                            let _ = abort_tx.send(true);
                        }
                    });
                }
                if let rdev::EventType::KeyRelease(rdev::Key::Escape) = event.event_type {
                    pressed_for_cb.store(false, Ordering::SeqCst);
                }
            };

            // Best effort; if listener fails, no kill switch.
            let _ = rdev::listen(callback);
        });

        Self { stop }
    }

    fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

#[cfg(feature = "non_robot_search")]
trait InputController: Send + Sync {
    fn lock(&self) -> anyhow::Result<()>;
    fn unlock(&self) -> anyhow::Result<()>;
}

#[cfg(feature = "non_robot_search")]
struct NoopInputController;

#[cfg(feature = "non_robot_search")]
impl InputController for NoopInputController {
    fn lock(&self) -> anyhow::Result<()> {
        // Placeholder: OS-level blocking is platform-specific. Keep behavior explicit in logs.
        info!("non_robot_search: input lock requested (noop implementation)");
        Ok(())
    }

    fn unlock(&self) -> anyhow::Result<()> {
        info!("non_robot_search: input unlock requested (noop implementation)");
        Ok(())
    }
}

#[cfg(feature = "non_robot_search")]
struct InputLockGuard<'a> {
    controller: &'a dyn InputController,
}

#[cfg(feature = "non_robot_search")]
impl<'a> InputLockGuard<'a> {
    fn new(controller: &'a dyn InputController) -> Self {
        Self { controller }
    }
}

#[cfg(feature = "non_robot_search")]
impl Drop for InputLockGuard<'_> {
    fn drop(&mut self) {
        let _ = self.controller.unlock();
    }
}

#[cfg(feature = "non_robot_search")]
enum Tone {
    Attention,
    Urgent,
    Success,
}

#[cfg(feature = "non_robot_search")]
fn play_tone(tone: Tone) {
    // Best-effort audio. If audio output is unavailable, silently ignore.
    let (freq, dur_ms) = match tone {
        Tone::Attention => (440.0_f32, 250u64),
        Tone::Urgent => (880.0_f32, 700u64),
        Tone::Success => (523.0_f32, 250u64),
    };

    std::thread::spawn(move || {
        let Ok(mut stream) = OutputStreamBuilder::open_default_stream() else {
            return;
        };
        stream.log_on_drop(false);
        let sink = Sink::connect_new(stream.mixer());
        let src = rodio::source::SineWave::new(freq)
            .take_duration(Duration::from_millis(dur_ms))
            .amplify(0.20);
        sink.append(src);
        sink.sleep_until_end();
    });
}
