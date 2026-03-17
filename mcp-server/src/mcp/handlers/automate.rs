//! Phase 18/19 — Playwright Killer: `scout_browser_automate` + `scout_browser_close`.
//!
//! Phase 18: navigate, click, type, evaluate, wait_for_selector, snapshot
//! Phase 19: scroll, press_key, screenshot  +  --headless=new persistent session

use crate::cdp::state;
use crate::mcp::{McpCallResponse, McpContent};
use crate::scraping::browser_manager;
use crate::types::ErrorResponse;
use anyhow::{anyhow, Result};
use axum::http::StatusCode;
use axum::response::Json;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, info};

use crate::AppState;

// ── Snapshot JS (what the LLM "sees" after each step) ────────────────────────

/// JavaScript evaluated for a `snapshot` step.
/// Returns a JSON string describing the page's interactive elements + body text.
const SNAPSHOT_SCRIPT: &str = r#"
JSON.stringify({
  title: document.title,
  url:   location.href,
  headings: [...document.querySelectorAll('h1,h2,h3,h4')]
              .slice(0, 20)
              .map(e => e.textContent.trim())
              .filter(Boolean),
  interactable: {
    buttons: [...document.querySelectorAll('button,[type="submit"],[role="button"]')]
               .slice(0, 20)
               .map(e => (e.textContent?.trim() || e.value || e.name || '').slice(0, 60))
               .filter(Boolean),
    inputs: [...document.querySelectorAll('input:not([type="hidden"]),textarea,select')]
              .slice(0, 20)
              .map(e => ({
                tag: e.tagName.toLowerCase(),
                type: e.type || null,
                id:   e.id   || null,
                name: e.name || null,
                placeholder: e.placeholder || null
              })),
    links: [...document.querySelectorAll('a[href]')]
             .slice(0, 20)
             .map(e => ({ text: e.textContent.trim().slice(0, 80), href: e.href }))
             .filter(e => e.text)
  },
  bodyText: (document.body ? document.body.innerText : '').slice(0, 3000)
})
"#;

// ── Step execution ────────────────────────────────────────────────────────────

async fn execute_step(page: &chromiumoxide::Page, step: &Value, idx: usize) -> Value {
    let action = step.get("action").and_then(|v| v.as_str()).unwrap_or("");
    let target = step.get("target").and_then(|v| v.as_str()).unwrap_or("");
    let value = step.get("value").and_then(|v| v.as_str()).unwrap_or("");

    let result = match action {
        "navigate" => run_navigate(page, target).await,
        "click" => run_click(page, target).await,
        "type" => run_type(page, target, value).await,
        "evaluate" => run_evaluate(page, value).await,
        "wait_for_selector" => {
            let timeout_ms = step
                .get("timeout_ms")
                .and_then(|v| v.as_u64())
                .unwrap_or(10_000);
            run_wait_for_selector(page, target, timeout_ms).await
        }
        "snapshot" => run_snapshot(page).await,
        "scroll" => {
            let direction = step.get("direction").and_then(|v| v.as_str()).unwrap_or("down");
            let pixels = step.get("pixels").and_then(|v| v.as_i64()).unwrap_or(500);
            run_scroll(page, direction, pixels).await
        }
        "press_key" => {
            let key = step.get("key").and_then(|v| v.as_str()).unwrap_or("");
            run_press_key(page, key).await
        }
        "screenshot" => run_screenshot(page).await,
        other => Err(anyhow!(
            "Unknown action '{}'. Valid actions: navigate, click, type, evaluate, wait_for_selector, snapshot, scroll, press_key, screenshot",
            other
        )),
    };

    match result {
        Ok(r) => json!({ "step": idx, "action": action, "status": "ok", "result": r }),
        Err(e) => json!({ "step": idx, "action": action, "status": "error", "error": e.to_string() }),
    }
}

// ── Individual action implementations ────────────────────────────────────────

async fn run_navigate(page: &chromiumoxide::Page, url: &str) -> Result<Value> {
    if url.is_empty() {
        return Err(anyhow!("navigate: 'target' (URL) is required"));
    }
    info!("🌐 navigate → {}", url);
    page.goto(url)
        .await
        .map_err(|e| anyhow!("navigate failed: {}", e))?;
    // Wait for the page to stabilise (network idle heuristic).
    browser_manager::wait_until_stable(page, 1000, 8000)
        .await
        .ok();
    Ok(json!({ "navigated_to": url }))
}

async fn run_click(page: &chromiumoxide::Page, selector: &str) -> Result<Value> {
    if selector.is_empty() {
        return Err(anyhow!("click: 'target' (CSS selector) is required"));
    }
    debug!("🖱️ click → {}", selector);
    let elem = page
        .find_element(selector)
        .await
        .map_err(|e| anyhow!("click: selector '{}' not found: {}", selector, e))?;
    elem.click()
        .await
        .map_err(|e| anyhow!("click: dispatch failed: {}", e))?;
    Ok(json!({ "clicked": selector }))
}

async fn run_type(
    page: &chromiumoxide::Page,
    selector: &str,
    text: &str,
) -> Result<Value> {
    if selector.is_empty() {
        return Err(anyhow!("type: 'target' (CSS selector) is required"));
    }
    if text.is_empty() {
        return Err(anyhow!("type: 'value' (text to type) is required"));
    }
    debug!("⌨️ type '{}' → {}", text, selector);
    let elem = page
        .find_element(selector)
        .await
        .map_err(|e| anyhow!("type: selector '{}' not found: {}", selector, e))?;
    // Click first to ensure focus, then type.
    elem.click()
        .await
        .map_err(|e| anyhow!("type: click-to-focus failed: {}", e))?;
    elem.type_str(text)
        .await
        .map_err(|e| anyhow!("type: dispatch failed: {}", e))?;
    Ok(json!({ "typed": text, "into": selector }))
}

async fn run_evaluate(page: &chromiumoxide::Page, script: &str) -> Result<Value> {
    if script.is_empty() {
        return Err(anyhow!("evaluate: 'value' (JS expression) is required"));
    }
    debug!("📜 evaluate: {}", script.chars().take(80).collect::<String>());
    let remote = page
        .evaluate(script)
        .await
        .map_err(|e| anyhow!("evaluate failed: {}", e))?;
    let val = remote
        .into_value::<Value>()
        .unwrap_or(Value::Null);
    Ok(val)
}

async fn run_wait_for_selector(
    page: &chromiumoxide::Page,
    selector: &str,
    timeout_ms: u64,
) -> Result<Value> {
    if selector.is_empty() {
        return Err(anyhow!("wait_for_selector: 'target' (CSS selector) is required"));
    }
    debug!("⏳ wait_for_selector '{}' ({}ms timeout)", selector, timeout_ms);
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        if page.find_element(selector).await.is_ok() {
            return Ok(json!({ "found": selector }));
        }
        if Instant::now() >= deadline {
            return Err(anyhow!(
                "wait_for_selector: '{}' not found within {}ms",
                selector,
                timeout_ms
            ));
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

async fn run_snapshot(page: &chromiumoxide::Page) -> Result<Value> {
    debug!("📸 snapshot");
    let remote = page
        .evaluate(SNAPSHOT_SCRIPT)
        .await
        .map_err(|e| anyhow!("snapshot evaluate failed: {}", e))?;
    // The script returns a JSON string — parse it into a Value so the outer
    // response is clean JSON (not a double-encoded string).
    let raw = remote
        .into_value::<Value>()
        .unwrap_or(Value::Null);
    if let Value::String(s) = &raw {
        return serde_json::from_str(s).map_err(|e| anyhow!("snapshot parse failed: {}", e));
    }
    Ok(raw)
}

// ── Phase 19: scroll ─────────────────────────────────────────────────────────

async fn run_scroll(page: &chromiumoxide::Page, direction: &str, pixels: i64) -> Result<Value> {
    debug!("🖱 scroll direction={} pixels={}", direction, pixels);
    let script = match direction {
        "bottom" => "window.scrollTo(0, document.body.scrollHeight)".to_string(),
        "top"    => "window.scrollTo(0, 0)".to_string(),
        "up"     => format!("window.scrollBy({{top: -{}, behavior: 'smooth'}})", pixels),
        _        => format!("window.scrollBy({{top: {}, behavior: 'smooth'}})", pixels),
    };
    page.evaluate(script)
        .await
        .map_err(|e| anyhow!("scroll evaluate failed: {}", e))?;
    // Brief pause to let smooth-scroll settle before the next step.
    tokio::time::sleep(Duration::from_millis(300)).await;
    Ok(json!({ "scrolled": direction, "pixels": pixels }))
}

// ── Phase 19: press_key ───────────────────────────────────────────────────────

/// Map a human-readable key name to its Windows virtual key code.
/// Required for reliable JS `keydown` / `keyup` event handling.
fn virtual_key_code(key: &str) -> i64 {
    match key {
        "Backspace" => 8,
        "Tab" => 9,
        "Enter" | "Return" => 13,
        "Shift" => 16,
        "Control" | "Ctrl" => 17,
        "Alt" => 18,
        "Pause" => 19,
        "CapsLock" => 20,
        "Escape" | "Esc" => 27,
        "Space" | " " => 32,
        "PageUp" => 33,
        "PageDown" => 34,
        "End" => 35,
        "Home" => 36,
        "ArrowLeft" => 37,
        "ArrowUp" => 38,
        "ArrowRight" => 39,
        "ArrowDown" => 40,
        "Delete" => 46,
        "F1" => 112, "F2" => 113, "F3" => 114, "F4" => 115,
        "F5" => 116, "F6" => 117, "F7" => 118, "F8" => 119,
        "F9" => 120, "F10" => 121, "F11" => 122, "F12" => 123,
        // Single printable ASCII character
        s if s.len() == 1 => s.chars().next().map(|c| c as i64).unwrap_or(0),
        _ => 0,
    }
}

async fn run_press_key(page: &chromiumoxide::Page, key: &str) -> Result<Value> {
    use chromiumoxide::cdp::browser_protocol::input::{
        DispatchKeyEventParams, DispatchKeyEventType,
    };
    if key.is_empty() {
        return Err(anyhow!("press_key: 'key' parameter is required"));
    }
    debug!("⌨ press_key: {}", key);
    let vk = virtual_key_code(key);
    // Normalise "Esc" / "Return" aliases to canonical W3C names.
    let canonical = match key { "Esc" => "Escape", "Return" => "Enter", other => other };

    let key_down = DispatchKeyEventParams::builder()
        .r#type(DispatchKeyEventType::KeyDown)
        .key(canonical)
        .windows_virtual_key_code(vk)
        .build()
        .map_err(|e| anyhow!("press_key: build keydown params failed: {}", e))?;
    let key_up = DispatchKeyEventParams::builder()
        .r#type(DispatchKeyEventType::KeyUp)
        .key(canonical)
        .windows_virtual_key_code(vk)
        .build()
        .map_err(|e| anyhow!("press_key: build keyup params failed: {}", e))?;

    page.execute(key_down)
        .await
        .map_err(|e| anyhow!("press_key: keyDown dispatch failed: {}", e))?;
    tokio::time::sleep(Duration::from_millis(30)).await;
    page.execute(key_up)
        .await
        .map_err(|e| anyhow!("press_key: keyUp dispatch failed: {}", e))?;

    Ok(json!({ "pressed": canonical }))
}

// ── Phase 19: screenshot ──────────────────────────────────────────────────────

async fn run_screenshot(page: &chromiumoxide::Page) -> Result<Value> {
    use chromiumoxide::cdp::browser_protocol::page::{
        CaptureScreenshotFormat, CaptureScreenshotParams,
    };
    debug!("📷 screenshot");
    let params = CaptureScreenshotParams::builder()
        .format(CaptureScreenshotFormat::Png)
        .build();
    let result = page
        .execute(params)
        .await
        .map_err(|e| anyhow!("screenshot: capture failed: {}", e))?;
    // CDP returns the image already base64-encoded.
    let b64 = &result.result.data;
    Ok(json!({
        "format": "png",
        "encoding": "base64",
        "data": b64
    }))
}

// ── scout_browser_automate handler ───────────────────────────────────────────

pub async fn handle(
    _state: Arc<AppState>,
    arguments: &Value,
) -> Result<Json<McpCallResponse>, (StatusCode, Json<ErrorResponse>)> {
    let steps = match arguments.get("steps").and_then(|v| v.as_array()) {
        Some(s) if !s.is_empty() => s.clone(),
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "'steps' must be a non-empty array of action objects".to_string(),
                }),
            ));
        }
    };

    // Lock the global session and ensure a live browser is ready.
    let mut guard = state::session_lock().lock().await;
    if let Err(e) = state::ensure_session(&mut guard).await {
        return Ok(Json(McpCallResponse {
            content: vec![McpContent {
                content_type: "text".to_string(),
                text: format!("Failed to launch browser session: {}", e),
            }],
            is_error: true,
        }));
    }

    let page = match guard.as_ref() {
        Some(s) => s.page.clone(), // Arc clone — keep ref-count alive through steps
        None => {
            return Ok(Json(McpCallResponse {
                content: vec![McpContent {
                    content_type: "text".to_string(),
                    text: "Session unexpectedly absent after ensure_session".to_string(),
                }],
                is_error: true,
            }));
        }
    };

    // Execute steps sequentially, collecting results.
    let mut results: Vec<Value> = Vec::with_capacity(steps.len());
    let mut had_error = false;

    for (idx, step) in steps.iter().enumerate() {
        let step_result = execute_step(&page, step, idx).await;
        if step_result.get("status").and_then(|v| v.as_str()) == Some("error") {
            had_error = true;
        }
        results.push(step_result);
    }

    let text = serde_json::to_string_pretty(&results)
        .unwrap_or_else(|e| format!(r#"[{{"error":"serialization failed: {}"}}]"#, e));

    Ok(Json(McpCallResponse {
        content: vec![McpContent {
            content_type: "text".to_string(),
            text,
        }],
        is_error: had_error,
    }))
}

// ── scout_browser_close handler ────────────────────────────────────────────────

pub async fn handle_close(
    _state: Arc<AppState>,
    _arguments: &Value,
) -> Result<Json<McpCallResponse>, (StatusCode, Json<ErrorResponse>)> {
    match state::close_session().await {
        Ok(()) => Ok(Json(McpCallResponse {
            content: vec![McpContent {
                content_type: "text".to_string(),
                text: r#"{"status":"closed","message":"Browser session terminated."}"#.to_string(),
            }],
            is_error: false,
        })),
        Err(e) => Ok(Json(McpCallResponse {
            content: vec![McpContent {
                content_type: "text".to_string(),
                text: format!("close_session error: {}", e),
            }],
            is_error: true,
        })),
    }
}
