//! Phase 18/19/21 — Playwright Killer: `scout_browser_automate` + `scout_browser_close`.
//!
//! Phase 18: navigate, click, type, evaluate, wait_for_selector, snapshot
//! Phase 19: scroll, press_key, screenshot  +  --headless=new persistent session
//! Phase 21: assert (fail-fast DOM assertions), mock_api (fetch+XHR network mocking)

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
    let timeout_ms = step
        .get("timeout_ms")
        .and_then(|v| v.as_u64())
        .unwrap_or(10_000);

    let result = match action {
        "run_flow" => run_flow(page, step).await,
        "navigate" => run_navigate(page, target).await,
        "click" => run_click(page, target, timeout_ms).await,
        "type" => run_type(page, target, value, timeout_ms).await,
        "evaluate" => run_evaluate(page, value).await,
        "wait_for_selector" => {
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
        "select_option" => run_select_option(page, target, value, timeout_ms).await,
        "drag_drop" => run_drag_drop(page, target, value, timeout_ms).await,
        // ── Phase 21 ───────────────────────────────────────────────────────────
        "assert" => {
            let condition = step.get("condition").and_then(|v| v.as_str()).unwrap_or("contains_text");
            run_assert(page, target, value, condition, timeout_ms).await
        }
        "mock_api" => {
            let url_pattern = step.get("url_pattern").and_then(|v| v.as_str()).unwrap_or("");
            let response_json = step.get("response_json").and_then(|v| v.as_str()).unwrap_or("{}");
            let status_code = step.get("status_code").and_then(|v| v.as_u64()).unwrap_or(200) as u16;
            run_mock_api(page, url_pattern, response_json, status_code).await
        }
        "console_tap" => run_console_tap(page).await,
        "console_dump" => run_console_dump(page).await,
        "storage_clear" => run_storage_clear(page, target).await,
        "storage_state_export" => run_storage_state_export(page).await,
        "storage_state_import" => run_storage_state_import(page, value).await,
        other => Err(anyhow!(
            "Unknown action '{}'. Valid actions: run_flow, navigate, click, type, evaluate, wait_for_selector, snapshot, scroll, press_key, screenshot, select_option, drag_drop, assert, mock_api, console_tap, console_dump, storage_clear, storage_state_export, storage_state_import",
            other
        )),
    };

    match result {
        Ok(r) => json!({ "step": idx, "action": action, "status": "ok", "result": r }),
        Err(e) => {
            // assert failures set halt=true to stop the sequence immediately.
            let halt = action == "assert";
            json!({ "step": idx, "action": action, "status": "error", "error": e.to_string(), "halt": halt })
        }
    }
}

async fn run_flow(page: &chromiumoxide::Page, step: &Value) -> Result<Value> {
    let nested_steps: Vec<Value> = if let Some(arr) = step.get("steps").and_then(|v| v.as_array()) {
        arr.clone()
    } else if let Some(raw) = step.get("value").and_then(|v| v.as_str()) {
        serde_json::from_str::<Vec<Value>>(raw)
            .map_err(|e| anyhow!("run_flow: invalid JSON array in value: {}", e))?
    } else {
        return Err(anyhow!(
            "run_flow: provide nested steps via 'steps' array or JSON array in 'value'"
        ));
    };

    if nested_steps.is_empty() {
        return Err(anyhow!("run_flow: nested steps cannot be empty"));
    }

    let mut results = Vec::with_capacity(nested_steps.len());
    for (idx, nested) in nested_steps.iter().enumerate() {
        let sub_result = Box::pin(execute_step(page, nested, idx)).await;
        let is_error = sub_result.get("status").and_then(|v| v.as_str()) == Some("error");
        let should_halt = sub_result
            .get("halt")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        results.push(sub_result);
        if is_error && should_halt {
            break;
        }
    }

    Ok(json!({
        "flow_steps": nested_steps.len(),
        "results": results
    }))
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

async fn run_click(page: &chromiumoxide::Page, selector: &str, timeout_ms: u64) -> Result<Value> {
    if selector.is_empty() {
        return Err(anyhow!("click: 'target' (CSS selector) is required"));
    }
    run_wait_for_selector(page, selector, timeout_ms).await?;
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
    timeout_ms: u64,
) -> Result<Value> {
    if selector.is_empty() {
        return Err(anyhow!("type: 'target' (CSS selector) is required"));
    }
    if text.is_empty() {
        return Err(anyhow!("type: 'value' (text to type) is required"));
    }
    run_wait_for_selector(page, selector, timeout_ms).await?;
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

async fn run_select_option(
    page: &chromiumoxide::Page,
    selector: &str,
    option_value: &str,
    timeout_ms: u64,
) -> Result<Value> {
    if selector.is_empty() {
        return Err(anyhow!("select_option: 'target' (select CSS selector) is required"));
    }
    if option_value.is_empty() {
        return Err(anyhow!("select_option: 'value' (option value/text) is required"));
    }
    run_wait_for_selector(page, selector, timeout_ms).await?;

    let sel_js = serde_json::to_string(selector).unwrap_or_default();
    let val_js = serde_json::to_string(option_value).unwrap_or_default();
    let script = format!(
        r#"(function() {{
  var el = document.querySelector({selector});
  if (!el) return {{ ok: false, reason: 'element not found' }};
  if (el.tagName.toLowerCase() !== 'select') return {{ ok: false, reason: 'target is not <select>' }};
  var wanted = {value};
  var matched = false;
  for (var i = 0; i < el.options.length; i++) {{
    var opt = el.options[i];
    if (opt.value === wanted || (opt.textContent || '').trim() === wanted) {{
      el.selectedIndex = i;
      matched = true;
      break;
    }}
  }}
  if (!matched) return {{ ok: false, reason: 'option not found' }};
  el.dispatchEvent(new Event('input', {{ bubbles: true }}));
  el.dispatchEvent(new Event('change', {{ bubbles: true }}));
  return {{ ok: true, selected: el.value }};
}})()"#,
        selector = sel_js,
        value = val_js
    );

    let remote = page
        .evaluate(script)
        .await
        .map_err(|e| anyhow!("select_option: evaluate failed: {}", e))?;
    let result = remote.into_value::<Value>().unwrap_or(Value::Null);
    if result.get("ok").and_then(|v| v.as_bool()) == Some(true) {
        Ok(json!({ "selected_on": selector, "value": option_value }))
    } else {
        Err(anyhow!("select_option failed: {}", result))
    }
}

async fn run_drag_drop(
    page: &chromiumoxide::Page,
    source_selector: &str,
    target_selector: &str,
    timeout_ms: u64,
) -> Result<Value> {
    if source_selector.is_empty() {
        return Err(anyhow!("drag_drop: 'target' (source CSS selector) is required"));
    }
    if target_selector.is_empty() {
        return Err(anyhow!("drag_drop: 'value' (destination CSS selector) is required"));
    }
    run_wait_for_selector(page, source_selector, timeout_ms).await?;
    run_wait_for_selector(page, target_selector, timeout_ms).await?;

    let src_js = serde_json::to_string(source_selector).unwrap_or_default();
    let dst_js = serde_json::to_string(target_selector).unwrap_or_default();
    let script = format!(
        r#"(function() {{
  var src = document.querySelector({src});
  var dst = document.querySelector({dst});
  if (!src || !dst) return {{ ok: false, reason: 'source/target not found' }};
  var dt = new DataTransfer();
  src.dispatchEvent(new DragEvent('dragstart', {{ bubbles: true, cancelable: true, dataTransfer: dt }}));
  dst.dispatchEvent(new DragEvent('dragenter', {{ bubbles: true, cancelable: true, dataTransfer: dt }}));
  dst.dispatchEvent(new DragEvent('dragover', {{ bubbles: true, cancelable: true, dataTransfer: dt }}));
  dst.dispatchEvent(new DragEvent('drop', {{ bubbles: true, cancelable: true, dataTransfer: dt }}));
  src.dispatchEvent(new DragEvent('dragend', {{ bubbles: true, cancelable: true, dataTransfer: dt }}));
  return {{ ok: true }};
}})()"#,
        src = src_js,
        dst = dst_js
    );

    let remote = page
        .evaluate(script)
        .await
        .map_err(|e| anyhow!("drag_drop: evaluate failed: {}", e))?;
    let result = remote.into_value::<Value>().unwrap_or(Value::Null);
    if result.get("ok").and_then(|v| v.as_bool()) == Some(true) {
        Ok(json!({ "dragged": source_selector, "dropped_on": target_selector }))
    } else {
        Err(anyhow!("drag_drop failed: {}", result))
    }
}

// ── Phase 21: assert ─────────────────────────────────────────────────────────

/// Convert a glob pattern (using `*` and `?`) to a JS-safe regex source string.
/// All regex-special characters except `*` / `?` are escaped.
fn glob_to_js_regex(pattern: &str) -> String {
    let mut out = String::with_capacity(pattern.len() * 2);
    for ch in pattern.chars() {
        match ch {
            '*' => out.push_str(".*"),
            '?' => out.push('.'),
            '.' | '+' | '^' | '$' | '{' | '}' | '(' | ')' | '|' | '[' | ']' | '\\' => {
                out.push('\\');
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out
}

/// Evaluate a DOM assertion in the page. Returns an error when the condition is false,
/// which will trigger fail-fast halt in the execution loop.
async fn run_assert(
    page: &chromiumoxide::Page,
    selector: &str,
    expected_value: &str,
    condition: &str,
    timeout_ms: u64,
) -> Result<Value> {
    if selector.is_empty() {
        return Err(anyhow!("assert: 'target' (CSS selector) is required"));
    }
    let sel_js = serde_json::to_string(selector).unwrap_or_default();
    let val_js = serde_json::to_string(expected_value).unwrap_or_default();

    let script = match condition {
        "contains_text" => format!(
            "(function(){{var el=document.querySelector({sel});if(!el)return{{ok:false,reason:'element not found'}};var t=(el.textContent||el.value||'').trim();return{{ok:t.includes({val}),actual:t.slice(0,200)}};}})()",
            sel = sel_js,
            val = val_js
        ),
        "is_visible" => format!(
            "(function(){{var el=document.querySelector({sel});if(!el)return{{ok:false,reason:'element not found'}};var r=el.getBoundingClientRect();var s=window.getComputedStyle(el);var ok=s.display!='none'&&s.visibility!='hidden'&&s.opacity!='0'&&r.width>0&&r.height>0;return{{ok:ok,display:s.display,visibility:s.visibility,w:r.width,h:r.height}};}})()",
            sel = sel_js
        ),
        "is_hidden" => format!(
            "(function(){{var el=document.querySelector({sel});if(!el)return{{ok:true,reason:'element not found (counts as hidden)'}};var r=el.getBoundingClientRect();var s=window.getComputedStyle(el);var visible=s.display!='none'&&s.visibility!='hidden'&&s.opacity!='0'&&r.width>0&&r.height>0;return{{ok:!visible}};}})()",
            sel = sel_js
        ),
        other => {
            return Err(anyhow!(
                "assert: unknown condition '{}'. Valid: contains_text, is_visible, is_hidden",
                other
            ))
        }
    };

        debug!(
                "🔍 assert '{}' {} '{}' ({}ms timeout)",
                selector, condition, expected_value, timeout_ms
        );

        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        let mut last_result: Value;
        loop {
                let remote = page
                        .evaluate(script.clone())
                        .await
                        .map_err(|e| anyhow!("assert: evaluate failed: {}", e))?;
                let result = remote.into_value::<Value>().unwrap_or(Value::Null);
                let ok = result.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
                if ok {
                        return Ok(json!({
                                "asserted": condition,
                                "selector": selector,
                                "passed": true,
                                "timeout_ms": timeout_ms
                        }));
                }

                last_result = result;
                if Instant::now() >= deadline {
                        return Err(anyhow!(
                                "Assertion failed after {}ms: '{}' {} '{}'. Last details: {}",
                                timeout_ms,
                                selector,
                                condition,
                                expected_value,
                                last_result
                        ));
                }
                tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

// ── Runtime console + storage actions (Playwright-like diagnostics/setup) ───

async fn run_console_tap(page: &chromiumoxide::Page) -> Result<Value> {
        let script = r#"
(function() {
    if (window.__cortexConsoleTapInstalled) {
        return { status: 'already_installed' };
    }
    window.__cortexConsoleTapInstalled = true;
    window.__cortexConsoleEvents = window.__cortexConsoleEvents || [];
    var maxEvents = 200;

    function pushEvent(level, args) {
        try {
            var text = Array.prototype.slice.call(args)
                .map(function(v) {
                    if (typeof v === 'string') return v;
                    try { return JSON.stringify(v); } catch (_) { return String(v); }
                })
                .join(' ')
                .slice(0, 1000);
            window.__cortexConsoleEvents.push({
                ts: new Date().toISOString(),
                level: level,
                text: text
            });
            if (window.__cortexConsoleEvents.length > maxEvents) {
                window.__cortexConsoleEvents.splice(0, window.__cortexConsoleEvents.length - maxEvents);
            }
        } catch (_) {}
    }

    ['log', 'info', 'warn', 'error', 'debug'].forEach(function(level) {
        var orig = console[level] ? console[level].bind(console) : null;
        console[level] = function() {
            pushEvent(level, arguments);
            if (orig) return orig.apply(console, arguments);
        };
    });

    window.addEventListener('error', function(e) {
        pushEvent('error', [e.message || 'window.error']);
    });
    window.addEventListener('unhandledrejection', function(e) {
        var reason = e && e.reason ? e.reason : 'unhandledrejection';
        pushEvent('error', [reason]);
    });

    return { status: 'installed' };
})()
"#;

        page.evaluate(script)
                .await
                .map_err(|e| anyhow!("console_tap: evaluate failed: {}", e))?;
        Ok(json!({ "console_tap": "installed" }))
}

async fn run_console_dump(page: &chromiumoxide::Page) -> Result<Value> {
        let script = r#"
(function() {
    var events = window.__cortexConsoleEvents || [];
    var errorCount = events.filter(function(e) { return e.level === 'error'; }).length;
    var warnCount = events.filter(function(e) { return e.level === 'warn'; }).length;
    return { total: events.length, errors: errorCount, warnings: warnCount, events: events };
})()
"#;

        let remote = page
                .evaluate(script)
                .await
                .map_err(|e| anyhow!("console_dump: evaluate failed: {}", e))?;
        Ok(remote.into_value::<Value>().unwrap_or(Value::Null))
}

async fn run_storage_clear(page: &chromiumoxide::Page, scope: &str) -> Result<Value> {
        let scope = if scope.is_empty() { "all" } else { scope };
        let script = match scope {
                "local" => "(function(){ localStorage.clear(); return { cleared: 'local' }; })()",
                "session" => "(function(){ sessionStorage.clear(); return { cleared: 'session' }; })()",
                "cookies" => r#"(function(){
                        document.cookie.split(';').forEach(function(c){
                            var name = c.split('=')[0].trim();
                            if (name) {
                                document.cookie = name + '=; expires=Thu, 01 Jan 1970 00:00:00 GMT; path=/';
                            }
                        });
                        return { cleared: 'cookies' };
                    })()"#,
                "all" => r#"(function(){
                        localStorage.clear();
                        sessionStorage.clear();
                        document.cookie.split(';').forEach(function(c){
                            var name = c.split('=')[0].trim();
                            if (name) {
                                document.cookie = name + '=; expires=Thu, 01 Jan 1970 00:00:00 GMT; path=/';
                            }
                        });
                        return { cleared: 'all' };
                    })()"#,
                other => {
                        return Err(anyhow!(
                                "storage_clear: unknown scope '{}'. Valid: all, local, session, cookies",
                                other
                        ))
                }
        };

        let remote = page
                .evaluate(script)
                .await
                .map_err(|e| anyhow!("storage_clear: evaluate failed: {}", e))?;
        Ok(remote.into_value::<Value>().unwrap_or(Value::Null))
}

async fn run_storage_state_export(page: &chromiumoxide::Page) -> Result<Value> {
        let script = r#"
(function() {
    var local = {};
    var session = {};
    for (var i = 0; i < localStorage.length; i++) {
        var k = localStorage.key(i);
        local[k] = localStorage.getItem(k);
    }
    for (var j = 0; j < sessionStorage.length; j++) {
        var s = sessionStorage.key(j);
        session[s] = sessionStorage.getItem(s);
    }
    return {
        url: location.href,
        localStorage: local,
        sessionStorage: session,
        cookies: document.cookie || ''
    };
})()
"#;

        let remote = page
                .evaluate(script)
                .await
                .map_err(|e| anyhow!("storage_state_export: evaluate failed: {}", e))?;
        Ok(remote.into_value::<Value>().unwrap_or(Value::Null))
}

async fn run_storage_state_import(page: &chromiumoxide::Page, raw_state: &str) -> Result<Value> {
        if raw_state.is_empty() {
                return Err(anyhow!("storage_state_import: 'value' JSON is required"));
        }
        let state_value: Value = serde_json::from_str(raw_state)
                .map_err(|e| anyhow!("storage_state_import: invalid JSON in 'value': {}", e))?;
        let state_json = serde_json::to_string(&state_value)
                .map_err(|e| anyhow!("storage_state_import: serialize failed: {}", e))?;
        let state_js = serde_json::to_string(&state_json).unwrap_or_else(|_| "\"{}\"".to_string());
        let script = format!(
                r#"(function() {{
    var state = JSON.parse({state_js});
    var local = state.localStorage || {{}};
    var session = state.sessionStorage || {{}};
    Object.keys(local).forEach(function(k) {{ localStorage.setItem(k, String(local[k])); }});
    Object.keys(session).forEach(function(k) {{ sessionStorage.setItem(k, String(session[k])); }});
    var appliedCookies = 0;
    if (typeof state.cookies === 'string' && state.cookies.length > 0) {{
        state.cookies.split(';').forEach(function(part) {{
            var kv = part.trim();
            if (!kv) return;
            document.cookie = kv + '; path=/';
            appliedCookies += 1;
        }});
    }}
    return {{ applied_local: Object.keys(local).length, applied_session: Object.keys(session).length, applied_cookies: appliedCookies }};
}})()"#,
                state_js = state_js
        );

        let remote = page
                .evaluate(script)
                .await
                .map_err(|e| anyhow!("storage_state_import: evaluate failed: {}", e))?;
        Ok(remote.into_value::<Value>().unwrap_or(Value::Null))
}

// ── Phase 21: mock_api ────────────────────────────────────────────────────────

/// Inject a `fetch` + `XMLHttpRequest` interceptor for URLs matching `url_pattern` (glob).
/// Uses `Page.addScriptToEvaluateOnNewDocument` so the mock persists across navigations,
/// then evaluates the same script on the live page immediately.
async fn run_mock_api(
    page: &chromiumoxide::Page,
    url_pattern: &str,
    response_json: &str,
    status_code: u16,
) -> Result<Value> {
    use chromiumoxide::cdp::browser_protocol::page::AddScriptToEvaluateOnNewDocumentParams;

    if url_pattern.is_empty() {
        return Err(anyhow!("mock_api: 'url_pattern' is required"));
    }

    // Pre-convert glob → regex in Rust to avoid complex JS regex-escaping inside format!.
    let js_regex = glob_to_js_regex(url_pattern);
    let regex_js = serde_json::to_string(&js_regex).unwrap_or_default();
    let body_js = serde_json::to_string(response_json).unwrap_or_default();
    let status = status_code as u32;

    let script = format!(
        r#"
(function() {{
  var _re = new RegExp({regex});
  var _body = {body};
  var _status = {status};
  function _match(url) {{ return _re.test(url); }}

  /* ── fetch override ─────────────────────────── */
  var _origFetch = window.fetch ? window.fetch.bind(window) : null;
  window.fetch = function(resource, init) {{
    var url = typeof resource === 'string' ? resource
              : (resource && resource.url ? resource.url : String(resource));
    if (_match(url)) {{
      return Promise.resolve(new Response(_body, {{
        status: _status,
        headers: {{ 'Content-Type': 'application/json' }}
      }}));
    }}
    return _origFetch ? _origFetch(resource, init) : Promise.reject(new Error('fetch unavailable'));
  }};

  /* ── XMLHttpRequest override ─────────────────── */
  var _OrigXHR = XMLHttpRequest;
  function _MockXHR() {{
    var _xhr = new _OrigXHR();
    var _self = this;
    var _url = '', _mocked = false;
    this.readyState = 0; this.status = 0; this.statusText = '';
    this.responseText = ''; this.response = '';
    this.onreadystatechange = null; this.onload = null; this.onerror = null;
    this.timeout = 0; this.withCredentials = false;

    this.open = function(m, u) {{
      _url = u; _mocked = _match(u);
      if (!_mocked) _xhr.open.apply(_xhr, arguments);
    }};
    this.send = function(body) {{
      if (_mocked) {{
        setTimeout(function() {{
          _self.readyState = 4; _self.status = _status; _self.statusText = 'OK';
          _self.responseText = _body; _self.response = _body;
          if (_self.onreadystatechange) _self.onreadystatechange();
          if (_self.onload) _self.onload({{ target: _self }});
        }}, 0);
      }} else {{
        _xhr.onreadystatechange = function() {{
          _self.readyState = _xhr.readyState; _self.status = _xhr.status;
          _self.statusText = _xhr.statusText; _self.responseText = _xhr.responseText;
          _self.response = _xhr.response;
          if (_self.onreadystatechange) _self.onreadystatechange();
        }};
        _xhr.onload  = function(e) {{ if (_self.onload)  _self.onload(e); }};
        _xhr.onerror = function(e) {{ if (_self.onerror) _self.onerror(e); }};
        _xhr.send(body);
      }}
    }};
    this.setRequestHeader = function() {{ if (!_mocked) _xhr.setRequestHeader.apply(_xhr, arguments); }};
    this.getResponseHeader = function(n) {{ return _mocked ? (n === 'Content-Type' ? 'application/json' : null) : _xhr.getResponseHeader(n); }};
    this.abort = function() {{ if (!_mocked) _xhr.abort(); }};
    this.addEventListener = function(ev, cb) {{
      if (_mocked && ev === 'load') setTimeout(cb, 0);
      else if (!_mocked) _xhr.addEventListener(ev, cb);
    }};
  }}
  XMLHttpRequest = _MockXHR;
}})();
"#,
        regex = regex_js,
        body = body_js,
        status = status
    );

    debug!("🔀 mock_api pattern='{}' status={}", url_pattern, status_code);

    // Persist mock across future page loads.
    page.execute(AddScriptToEvaluateOnNewDocumentParams::new(script.clone()))
        .await
        .map_err(|e| anyhow!("mock_api: addScriptToEvaluateOnNewDocument failed: {}", e))?;

    // Also activate on the currently-loaded page immediately.
    page.evaluate(script)
        .await
        .map_err(|e| anyhow!("mock_api: immediate inject on current page failed: {}", e))?;

    Ok(json!({
        "mocked": url_pattern,
        "status_code": status_code,
        "scope": "current page + all future navigations"
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
        let is_error = step_result.get("status").and_then(|v| v.as_str()) == Some("error");
        let should_halt = step_result.get("halt").and_then(|v| v.as_bool()).unwrap_or(false);
        if is_error {
            had_error = true;
        }
        results.push(step_result);
        if should_halt {
            // Fail-fast: assertion failed — do not execute any further steps.
            break;
        }
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

// ── Phase 20: Agent Auth Portal ───────────────────────────────────────────────

/// Launch the agent profile in **visible** mode so a human can complete an
/// OAuth / CAPTCHA / 2FA flow, then close the browser and return.
///
/// Steps:
/// 1. Close any live headless session to release the SingletonLock.
/// 2. Launch a fully visible browser window on the SAME agent profile.
/// 3. Navigate to `url`.
/// 4. Block for up to `timeout_secs` seconds (default 120) — this is the
///    window during which the user completes the login.
/// 5. Close the visible browser; profile cookies are now persisted.
/// 6. Future `scout_browser_automate` calls will reuse those cookies silently.
pub async fn handle_profile_auth(
    _state: Arc<AppState>,
    arguments: &Value,
) -> Result<Json<McpCallResponse>, (StatusCode, Json<ErrorResponse>)> {
    use chromiumoxide::browser::BrowserConfig;
    use chromiumoxide::handler::viewport::Viewport;
    use futures::StreamExt;

    let url = arguments
        .get("url")
        .and_then(|v| v.as_str())
        .unwrap_or("about:blank");

    let instruction = arguments
        .get("instruction")
        .and_then(|v| v.as_str())
        .unwrap_or("Please complete the login in this window, then close it when done.");

    let timeout_secs = arguments
        .get("timeout_secs")
        .and_then(|v| v.as_u64())
        .unwrap_or(120)
        .clamp(10, 600); // clamp: 10s–10min

    // Step 1: release the headless SingletonLock on the profile.
    if let Err(e) = state::close_session().await {
        return Ok(Json(McpCallResponse {
            content: vec![McpContent {
                content_type: "text".to_string(),
                text: format!("agent_profile_auth: could not close headless session: {}", e),
            }],
            is_error: true,
        }));
    }

    // Step 2: resolve browser and build a VISIBLE config.
    let exe = match browser_manager::find_chrome_executable() {
        Some(e) => e,
        None => {
            return Ok(Json(McpCallResponse {
                content: vec![McpContent {
                    content_type: "text".to_string(),
                    text: "agent_profile_auth: no browser found (install Brave, Chrome, or Chromium)"
                        .to_string(),
                }],
                is_error: true,
            }));
        }
    };

    let (profile_dir, _) = state::agent_profile_dir();
    let ua = browser_manager::random_user_agent();

    // Visible browser: omit --headless=new so the OS shows the window.
    let config = match BrowserConfig::builder()
        .chrome_executable(&exe)
        .viewport(Viewport {
            width: 1280,
            height: 900,
            device_scale_factor: Some(1.0),
            emulating_mobile: false,
            is_landscape: true,
            has_touch: false,
        })
        .window_size(1280, 900)
        .arg("--no-sandbox")
        .arg("--disable-setuid-sandbox")
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg("--disable-blink-features=AutomationControlled")
        .arg(format!("--user-data-dir={}", profile_dir.display()))
        .arg(format!("--user-agent={}", ua))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return Ok(Json(McpCallResponse {
                content: vec![McpContent {
                    content_type: "text".to_string(),
                    text: format!("agent_profile_auth: config build failed: {}", e),
                }],
                is_error: true,
            }));
        }
    };

    info!(
        "🔓 Opening agent profile in VISIBLE mode for human auth ({}s window): {}",
        timeout_secs, url
    );
    info!("📋 Instruction: {}", instruction);

    let (mut browser, mut handler) =
        match browser_manager::launch_browser_serialized(config, "agent_profile_auth").await {
            Ok(pair) => pair,
            Err(e) => {
                return Ok(Json(McpCallResponse {
                    content: vec![McpContent {
                        content_type: "text".to_string(),
                        text: format!("agent_profile_auth: browser launch failed: {}", e),
                    }],
                    is_error: true,
                }));
            }
        };

    let handler_task = tokio::spawn(async move {
        while let Some(event) = handler.next().await {
            if let Err(e) = event {
                browser_manager::log_cdp_handler_error("auth-portal handler", &e.to_string());
            }
        }
    });

    // Step 3: navigate to the target URL.
    let nav_result = browser.new_page(url).await;
    if let Err(e) = nav_result {
        let _ = browser.close().await;
        handler_task.abort();
        return Ok(Json(McpCallResponse {
            content: vec![McpContent {
                content_type: "text".to_string(),
                text: format!("agent_profile_auth: navigation failed: {}", e),
            }],
            is_error: true,
        }));
    }

    // Step 4: wait for the user to finish (or for the timeout).
    // We poll the browser liveness every 2 seconds so we return promptly
    // if the user closes the window manually before the timeout expires.
    let page = nav_result.unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_secs);
    let mut user_closed = false;
    loop {
        if tokio::time::Instant::now() >= deadline {
            break;
        }
        // Probe: a closed window makes the page unreachable.
        if page.evaluate("1").await.is_err() {
            user_closed = true;
            break;
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    // Step 5: close the visible browser to flush cookies to disk.
    let _ = tokio::time::timeout(Duration::from_secs(5), browser.close()).await;
    handler_task.abort();
    let _ = handler_task.await;

    let reason = if user_closed {
        "user closed window"
    } else {
        "timeout reached"
    };
    info!("✅ Auth portal closed ({}). Agent profile is now authenticated.", reason);

    let text = serde_json::to_string_pretty(&json!({
        "status": "ok",
        "reason": reason,
        "profile": profile_dir.to_string_lossy(),
        "message": "Session saved to agent profile. Future scout_browser_automate calls will reuse these cookies silently."
    }))
    .unwrap_or_default();

    Ok(Json(McpCallResponse {
        content: vec![McpContent {
            content_type: "text".to_string(),
            text,
        }],
        is_error: false,
    }))
}
