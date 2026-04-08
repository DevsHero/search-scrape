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
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::{LazyLock, Mutex};
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

static STORAGE_FIXTURES: LazyLock<Mutex<HashMap<String, Value>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Default)]
struct TraceState {
    enabled: bool,
    run_id: Option<String>,
    started_at: Option<String>,
    events: Vec<Value>,
}

static TRACE_STATE: LazyLock<Mutex<TraceState>> =
    LazyLock::new(|| Mutex::new(TraceState::default()));

fn build_visible_auth_config(
    exe: &str,
    profile_dir: &Path,
) -> Result<chromiumoxide::browser::BrowserConfig> {
    use chromiumoxide::browser::BrowserConfig;
    use chromiumoxide::handler::viewport::Viewport;

    let ua = browser_manager::random_user_agent();

    BrowserConfig::builder()
        .chrome_executable(exe)
        .with_head()
        .no_sandbox()
        .viewport(Viewport {
            width: 1280,
            height: 900,
            device_scale_factor: Some(1.0),
            emulating_mobile: false,
            is_landscape: true,
            has_touch: false,
        })
        .window_size(1280, 900)
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg("--disable-blink-features=AutomationControlled")
        .arg(format!("--user-data-dir={}", profile_dir.display()))
        .arg(format!("--user-agent={}", ua))
        .build()
        .map_err(|e| anyhow!("Failed to build visible auth browser config: {}", e))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct MockRouteDefinition {
    pattern: String,
    method: Option<String>,
    response_body: String,
    status_code: u16,
    response_headers: Value,
    delay_ms: u64,
    once: bool,
    remove_headers: Vec<String>,
}

static MOCK_ROUTES: LazyLock<Mutex<Vec<MockRouteDefinition>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct DialogPolicy {
    accept: bool,
    prompt_text: Option<String>,
}

static DIALOG_POLICY: LazyLock<Mutex<Option<DialogPolicy>>> =
    LazyLock::new(|| Mutex::new(None));

fn trace_record(event: Value) {
    if let Ok(mut st) = TRACE_STATE.lock() {
        if st.enabled {
            st.events.push(event);
        }
    }
}

// ── Step execution ────────────────────────────────────────────────────────────

async fn execute_step(
    browser: &chromiumoxide::Browser,
    page: &mut chromiumoxide::Page,
    step: &Value,
    idx: usize,
) -> Value {
    let action = step.get("action").and_then(|v| v.as_str()).unwrap_or("");
    let target = step.get("target").and_then(|v| v.as_str()).unwrap_or("");
    let value = step.get("value").and_then(|v| v.as_str()).unwrap_or("");
    let timeout_ms = step
        .get("timeout_ms")
        .and_then(|v| v.as_u64())
        .unwrap_or(10_000);

    let result = match action {
        "run_flow" => run_flow(browser, page, step).await,
        "trace_start" | "start_tracing" => run_trace_start(step).await,
        "trace_stop" | "stop_tracing" => run_trace_stop().await,
        "trace_export" => run_trace_export(step).await,
        "navigate" => run_navigate(page, target).await,
        "navigate_back" => run_navigate_back(page).await,
        "click" => run_click(page, step, timeout_ms).await,
        "click_locator" => run_click_locator(page, step, timeout_ms).await,
        "hover" => run_hover(page, step, timeout_ms).await,
        "type" => run_type(page, step, timeout_ms).await,
        "type_locator" => run_type_locator(page, step, timeout_ms).await,
        "evaluate" | "run_code" => run_evaluate(page, step).await,
        "wait_for_selector" => {
            run_wait_for_selector(page, target, timeout_ms).await
        }
        "wait_for_locator" => run_wait_for_locator(page, step, timeout_ms).await,
        "wait_for" => run_wait_for(page, step).await,
        "snapshot" => run_snapshot(page, step).await,
        "scroll" => {
            let direction = step.get("direction").and_then(|v| v.as_str()).unwrap_or("down");
            let pixels = step.get("pixels").and_then(|v| v.as_i64()).unwrap_or(500);
            run_scroll(page, direction, pixels).await
        }
        "press_key" => {
            let key = step.get("key").and_then(|v| v.as_str()).unwrap_or("");
            run_press_key(page, key).await
        }
        "resize" => run_resize(page, step).await,
        "screenshot" | "take_screenshot" => run_screenshot(page, step).await,
        "select_option" => run_select_option(page, step, timeout_ms).await,
        "drag_drop" => run_drag_drop(page, target, value, timeout_ms).await,
        "file_upload" => run_file_upload(page, step, timeout_ms).await,
        "fill_form" => run_fill_form(page, step, timeout_ms).await,
        "handle_dialog" => run_handle_dialog(page, step).await,
        "tabs" => run_tabs(browser, page, step).await,
        "pdf_save" => run_pdf_save(page, step).await,
        "mouse_click_xy" => run_mouse_click_xy(page, step).await,
        "mouse_down" => run_mouse_down(page, step).await,
        "mouse_drag_xy" => run_mouse_drag_xy(page, step).await,
        "mouse_move_xy" => run_mouse_move_xy(page, step).await,
        "mouse_up" => run_mouse_up(page, step).await,
        "mouse_wheel" => run_mouse_wheel(page, step).await,
        // ── Phase 21 ───────────────────────────────────────────────────────────
        "assert" => {
            let condition = step.get("condition").and_then(|v| v.as_str()).unwrap_or("contains_text");
            run_assert(page, target, value, condition, timeout_ms).await
        }
        "assert_locator" => {
            let condition = step
                .get("condition")
                .and_then(|v| v.as_str())
                .unwrap_or("contains_text");
            run_assert_locator(page, step, condition, timeout_ms).await
        }
        "mock_api" | "route" => run_mock_api(page, step).await,
        "route_list" => run_route_list().await,
        "unroute" => run_unroute(page, step).await,
        "network_state_set" => run_network_state_set(page, step).await,
        "network_tap" => run_network_tap(page).await,
        "network_dump" | "network_requests" => run_network_dump(page, step).await,
        "console_tap" => run_console_tap(page).await,
        "console_dump" | "console_messages" => run_console_dump(page, step).await,
        "storage_clear" => run_storage_clear(page, target).await,
        "storage_state_export" | "storage_state" => run_storage_state_export(page, step).await,
        "storage_state_import" | "set_storage_state" => run_storage_state_import(page, step).await,
        "storage_checkpoint" => run_storage_checkpoint(page, target).await,
        "storage_rollback" => run_storage_rollback(page, target).await,
        "cookie_clear" => run_cookie_clear(page).await,
        "cookie_delete" => run_cookie_delete(page, step).await,
        "cookie_get" => run_cookie_get(page, step).await,
        "cookie_list" => run_cookie_list(page, step).await,
        "cookie_set" => run_cookie_set(page, step).await,
        "localstorage_clear" => run_local_storage_clear(page).await,
        "localstorage_delete" => run_local_storage_delete(page, step).await,
        "localstorage_get" => run_local_storage_get(page, step).await,
        "localstorage_list" => run_local_storage_list(page).await,
        "localstorage_set" => run_local_storage_set(page, step).await,
        "sessionstorage_clear" => run_session_storage_clear(page).await,
        "sessionstorage_delete" => run_session_storage_delete(page, step).await,
        "sessionstorage_get" => run_session_storage_get(page, step).await,
        "sessionstorage_list" => run_session_storage_list(page).await,
        "sessionstorage_set" => run_session_storage_set(page, step).await,
        "generate_locator" => run_generate_locator(page, step, timeout_ms).await,
        "verify_element_visible" => run_verify_element_visible(page, step, timeout_ms).await,
        "verify_list_visible" => run_verify_list_visible(page, step, timeout_ms).await,
        "verify_text_visible" => run_verify_text_visible(page, step, timeout_ms).await,
        "verify_value" => run_verify_value(page, step, timeout_ms).await,
        other => Err(anyhow!(
            "Unknown action '{}'. See scout_browser_automate schema for supported actions.",
            other
        )),
    };

    match result {
        Ok(r) => {
            let event = json!({
                "ts": Utc::now().to_rfc3339(),
                "step": idx,
                "action": action,
                "status": "ok",
                "result": r
            });
            trace_record(event.clone());
            event
        }
        Err(e) => {
            // assert failures set halt=true to stop the sequence immediately.
            let halt = action == "assert";
            let event = json!({
                "ts": Utc::now().to_rfc3339(),
                "step": idx,
                "action": action,
                "status": "error",
                "error": e.to_string(),
                "halt": halt
            });
            trace_record(event.clone());
            event
        }
    }
}

async fn run_flow(
    browser: &chromiumoxide::Browser,
    page: &mut chromiumoxide::Page,
    step: &Value,
) -> Result<Value> {
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
        let sub_result = Box::pin(execute_step(browser, page, nested, idx)).await;
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

async fn run_trace_start(step: &Value) -> Result<Value> {
        let run_id = step
                .get("target")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
        let mut st = TRACE_STATE
                .lock()
                .map_err(|_| anyhow!("trace_start: trace state lock poisoned"))?;
        st.enabled = true;
        st.events.clear();
        st.run_id = run_id.clone();
        st.started_at = Some(Utc::now().to_rfc3339());
        Ok(json!({
                "trace": "started",
                "run_id": run_id
        }))
}

async fn run_trace_stop() -> Result<Value> {
        let mut st = TRACE_STATE
                .lock()
                .map_err(|_| anyhow!("trace_stop: trace state lock poisoned"))?;
        st.enabled = false;
        Ok(json!({
                "trace": "stopped",
                "run_id": st.run_id,
                "events": st.events.len(),
                "started_at": st.started_at,
                "stopped_at": Utc::now().to_rfc3339()
        }))
}

async fn run_trace_export(step: &Value) -> Result<Value> {
        let output_path = step.get("target").and_then(|v| v.as_str()).unwrap_or("");
        let st = TRACE_STATE
                .lock()
                .map_err(|_| anyhow!("trace_export: trace state lock poisoned"))?;
        let payload = json!({
                "run_id": st.run_id,
                "started_at": st.started_at,
                "exported_at": Utc::now().to_rfc3339(),
                "events": st.events
        });

        if output_path.is_empty() {
                return Ok(payload);
        }

        let path = PathBuf::from(output_path);
        let data = serde_json::to_vec_pretty(&payload)
                .map_err(|e| anyhow!("trace_export: serialize failed: {}", e))?;
        std::fs::write(&path, data)
                .map_err(|e| anyhow!("trace_export: write failed for '{}': {}", path.display(), e))?;

        Ok(json!({
                "trace": "exported",
                "path": path.display().to_string(),
                "events": st.events.len()
        }))
}

fn locator_field<'a>(step: &'a Value, key: &str) -> Option<&'a str> {
        step.get(key).and_then(|v| v.as_str()).filter(|s| !s.is_empty())
}

fn build_locator_js(step: &Value) -> (String, String, Option<String>, bool, Option<String>) {
    let strategy = locator_field(step, "locator")
        .or_else(|| locator_field(step, "locator_strategy"))
                .unwrap_or("css")
                .to_string();
        let locator_value = locator_field(step, "target").unwrap_or("").to_string();
    let locator_name = locator_field(step, "name")
        .or_else(|| locator_field(step, "locator_name"))
        .map(|s| s.to_string());
        let exact = step
        .get("exact")
        .or_else(|| step.get("locator_exact"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
    let scope = locator_field(step, "scope")
        .or_else(|| locator_field(step, "locator_scope"))
        .map(|s| s.to_string());
        (strategy, locator_value, locator_name, exact, scope)
}

fn locator_resolve_script(
        strategy: &str,
        value: &str,
        name: Option<&str>,
        exact: bool,
        scope: Option<&str>,
) -> String {
        let strategy_js = serde_json::to_string(strategy).unwrap_or_else(|_| "\"css\"".to_string());
        let value_js = serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string());
        let name_js = serde_json::to_string(name.unwrap_or(""))
                .unwrap_or_else(|_| "\"\"".to_string());
        let scope_js = serde_json::to_string(scope.unwrap_or(""))
                .unwrap_or_else(|_| "\"\"".to_string());
        format!(
                r#"(function() {{
    var strategy = {strategy};
    var value = {value};
    var roleName = {name};
    var exact = {exact};
    var scopeSel = {scope};
    var root = scopeSel ? document.querySelector(scopeSel) : document;
    if (!root) return {{ ok:false, reason:'locator scope not found' }};
    function norm(s) {{ return (s || '').replace(/\s+/g, ' ').trim(); }}
    function matchText(actual, wanted) {{
        if (!wanted) return false;
        var a = norm(actual).toLowerCase();
        var w = norm(wanted).toLowerCase();
        return exact ? a === w : a.indexOf(w) !== -1;
    }}
    function visible(el) {{
        if (!el) return false;
        var st = window.getComputedStyle(el);
        var r = el.getBoundingClientRect();
        return st.display !== 'none' && st.visibility !== 'hidden' && st.opacity !== '0' && r.width > 0 && r.height > 0;
    }}
    function firstVisible(list) {{
        for (var i = 0; i < list.length; i++) if (visible(list[i])) return list[i];
        return list[0] || null;
    }}

    var el = null;
    if (strategy === 'css') {{
        el = root.querySelector(value);
    }} else if (strategy === 'text') {{
        var candidates = root.querySelectorAll('button,a,[role],label,div,span,p,h1,h2,h3,h4,h5,h6,li');
        var out = [];
        for (var i = 0; i < candidates.length; i++) if (matchText(candidates[i].innerText || candidates[i].textContent, value)) out.push(candidates[i]);
        el = firstVisible(out);
    }} else if (strategy === 'role') {{
        var roleSel = '[role="' + value + '"]';
        var roleCands = Array.prototype.slice.call(root.querySelectorAll(roleSel));
        if (value === 'button') roleCands = roleCands.concat(Array.prototype.slice.call(root.querySelectorAll('button,input[type="button"],input[type="submit"]')));
        if (value === 'textbox') roleCands = roleCands.concat(Array.prototype.slice.call(root.querySelectorAll('input[type="text"],textarea,input:not([type])')));
        if (roleName) roleCands = roleCands.filter(function(n) {{ return matchText(n.innerText || n.textContent || n.getAttribute('aria-label'), roleName); }});
        el = firstVisible(roleCands);
    }} else if (strategy === 'label') {{
        var labels = root.querySelectorAll('label');
        for (var j = 0; j < labels.length; j++) {{
            var lb = labels[j];
            if (!matchText(lb.innerText || lb.textContent, value)) continue;
            var forId = lb.getAttribute('for');
            if (forId) {{
                el = document.getElementById(forId);
            }} else {{
                el = lb.querySelector('input,textarea,select');
            }}
            if (el) break;
        }}
    }} else if (strategy === 'placeholder') {{
        var inputs = root.querySelectorAll('input[placeholder],textarea[placeholder]');
        for (var k = 0; k < inputs.length; k++) {{
            if (matchText(inputs[k].getAttribute('placeholder'), value)) {{ el = inputs[k]; break; }}
        }}
    }} else if (strategy === 'testid') {{
        var tid = value.replace(/"/g, '\\"');
        el = root.querySelector('[data-testid="' + tid + '"]');
    }} else {{
        return {{ ok:false, reason:'unknown locator strategy: ' + strategy }};
    }}

    if (!el) return {{ ok:false, reason:'locator did not match any element' }};
    return {{ ok:true, strategy:strategy, tag:(el.tagName || '').toLowerCase() }};
}})()"#,
                strategy = strategy_js,
                value = value_js,
                name = name_js,
                exact = if exact { "true" } else { "false" },
                scope = scope_js
        )
}

// ── Individual action implementations ────────────────────────────────────────

fn step_bool(step: &Value, key: &str) -> Option<bool> {
    step.get(key).and_then(|v| v.as_bool())
}

fn step_f64(step: &Value, key: &str) -> Option<f64> {
    step.get(key).and_then(|v| v.as_f64())
}

fn step_i64(step: &Value, key: &str) -> Option<i64> {
    step.get(key).and_then(|v| v.as_i64())
}

fn step_u64(step: &Value, key: &str) -> Option<u64> {
    step.get(key).and_then(|v| v.as_u64())
}

fn step_string_vec(step: &Value, key: &str) -> Vec<String> {
    match step.get(key) {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| item.as_str().map(|s| s.to_string()))
            .collect(),
        Some(Value::String(single)) if !single.is_empty() => vec![single.clone()],
        _ => Vec::new(),
    }
}

fn mouse_modifiers_from_step(step: &Value) -> i64 {
    let Some(items) = step.get("modifiers").and_then(|v| v.as_array()) else {
        return 0;
    };
    let mut value = 0;
    for item in items.iter().filter_map(|v| v.as_str()) {
        match item {
            "Alt" => value |= 1,
            "Control" | "Ctrl" => value |= 2,
            "Meta" | "Command" | "ControlOrMeta" => value |= 4,
            "Shift" => value |= 8,
            _ => {}
        }
    }
    value
}

fn parse_mouse_button(raw: &str) -> Result<(chromiumoxide::cdp::browser_protocol::input::MouseButton, i64)> {
    use chromiumoxide::cdp::browser_protocol::input::MouseButton;

    match raw {
        "right" => Ok((MouseButton::Right, 2)),
        "middle" => Ok((MouseButton::Middle, 4)),
        "back" => Ok((MouseButton::Back, 8)),
        "forward" => Ok((MouseButton::Forward, 16)),
        "left" | "" => Ok((MouseButton::Left, 1)),
        other => Err(anyhow!("Unsupported mouse button '{}'", other)),
    }
}

async fn selector_rect(page: &chromiumoxide::Page, selector: &str) -> Result<(f64, f64, f64, f64)> {
    let selector_js = serde_json::to_string(selector).unwrap_or_else(|_| "\"\"".to_string());
    let script = format!(
        r#"(function() {{
  var el = document.querySelector({selector});
  if (!el) return {{ ok:false, reason:'element not found' }};
  el.scrollIntoView({{ block:'center', inline:'center' }});
  var r = el.getBoundingClientRect();
  return {{ ok:true, x:r.left, y:r.top, width:r.width, height:r.height }};
}})()"#,
        selector = selector_js
    );
    let remote = page
        .evaluate(script)
        .await
        .map_err(|e| anyhow!("selector_rect evaluate failed: {}", e))?;
    let result = remote.into_value::<Value>().unwrap_or(Value::Null);
    if result.get("ok").and_then(|v| v.as_bool()) != Some(true) {
        return Err(anyhow!("selector_rect failed for '{}': {}", selector, result));
    }
    Ok((
        result.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0),
        result.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0),
        result.get("width").and_then(|v| v.as_f64()).unwrap_or(0.0),
        result.get("height").and_then(|v| v.as_f64()).unwrap_or(0.0),
    ))
}

async fn selector_center(page: &chromiumoxide::Page, selector: &str) -> Result<(f64, f64)> {
    let (x, y, width, height) = selector_rect(page, selector).await?;
    Ok((x + width / 2.0, y + height / 2.0))
}

async fn locator_center(page: &chromiumoxide::Page, step: &Value) -> Result<(f64, f64)> {
    let (strategy, value, name, exact, scope) = build_locator_js(step);
    if value.is_empty() {
        return Err(anyhow!("locator_center: 'target' is required"));
    }
    let strategy_js = serde_json::to_string(&strategy).unwrap_or_else(|_| "\"css\"".to_string());
    let value_js = serde_json::to_string(&value).unwrap_or_else(|_| "\"\"".to_string());
    let name_js = serde_json::to_string(name.as_deref().unwrap_or(""))
        .unwrap_or_else(|_| "\"\"".to_string());
    let scope_js = serde_json::to_string(scope.as_deref().unwrap_or(""))
        .unwrap_or_else(|_| "\"\"".to_string());
    let script = format!(
        r#"(function() {{
  var strategy = {strategy};
  var value = {value};
  var roleName = {name};
  var exact = {exact};
  var scopeSel = {scope};
  var root = scopeSel ? document.querySelector(scopeSel) : document;
  if (!root) return {{ ok:false, reason:'locator scope not found' }};
  function norm(s) {{ return (s || '').replace(/\s+/g, ' ').trim(); }}
  function matchText(actual, wanted) {{
    if (!wanted) return false;
    var a = norm(actual).toLowerCase();
    var w = norm(wanted).toLowerCase();
    return exact ? a === w : a.indexOf(w) !== -1;
  }}
  function visible(el) {{
    if (!el) return false;
    var st = window.getComputedStyle(el);
    var r = el.getBoundingClientRect();
    return st.display !== 'none' && st.visibility !== 'hidden' && st.opacity !== '0' && r.width > 0 && r.height > 0;
  }}
  function firstVisible(list) {{
    for (var i = 0; i < list.length; i++) if (visible(list[i])) return list[i];
    return list[0] || null;
  }}
  var el = null;
  if (strategy === 'css') el = root.querySelector(value);
  else if (strategy === 'text') {{
    var candidates = root.querySelectorAll('button,a,[role],label,div,span,p,h1,h2,h3,h4,h5,h6,li');
    var out = [];
    for (var i = 0; i < candidates.length; i++) if (matchText(candidates[i].innerText || candidates[i].textContent, value)) out.push(candidates[i]);
    el = firstVisible(out);
  }} else if (strategy === 'role') {{
    var roleSel = '[role="' + value + '"]';
    var roleCands = Array.prototype.slice.call(root.querySelectorAll(roleSel));
    if (value === 'button') roleCands = roleCands.concat(Array.prototype.slice.call(root.querySelectorAll('button,input[type="button"],input[type="submit"]')));
    if (value === 'textbox') roleCands = roleCands.concat(Array.prototype.slice.call(root.querySelectorAll('input[type="text"],textarea,input:not([type])')));
    if (roleName) roleCands = roleCands.filter(function(n) {{ return matchText(n.innerText || n.textContent || n.getAttribute('aria-label'), roleName); }});
    el = firstVisible(roleCands);
  }} else if (strategy === 'label') {{
    var labels = root.querySelectorAll('label');
    for (var j = 0; j < labels.length; j++) {{
      var lb = labels[j];
      if (!matchText(lb.innerText || lb.textContent, value)) continue;
      var forId = lb.getAttribute('for');
      if (forId) el = document.getElementById(forId);
      else el = lb.querySelector('input,textarea,select');
      if (el) break;
    }}
  }} else if (strategy === 'placeholder') {{
    var inputs = root.querySelectorAll('input[placeholder],textarea[placeholder]');
    for (var k = 0; k < inputs.length; k++) if (matchText(inputs[k].getAttribute('placeholder'), value)) {{ el = inputs[k]; break; }}
  }} else if (strategy === 'testid') {{
    var tid = value.replace(/"/g, '\\"');
    el = root.querySelector('[data-testid="' + tid + '"]');
  }}
  if (!el) return {{ ok:false, reason:'locator did not match any element' }};
  el.scrollIntoView({{ block:'center', inline:'center' }});
  var r = el.getBoundingClientRect();
  return {{ ok:true, x:r.left + r.width / 2, y:r.top + r.height / 2 }};
}})()"#,
        strategy = strategy_js,
        value = value_js,
        name = name_js,
        exact = if exact { "true" } else { "false" },
        scope = scope_js,
    );
    let remote = page
        .evaluate(script)
        .await
        .map_err(|e| anyhow!("locator_center evaluate failed: {}", e))?;
    let result = remote.into_value::<Value>().unwrap_or(Value::Null);
    if result.get("ok").and_then(|v| v.as_bool()) != Some(true) {
        return Err(anyhow!("locator_center failed: {}", result));
    }
    Ok((
        result.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0),
        result.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0),
    ))
}

async fn dispatch_mouse_click(
    page: &chromiumoxide::Page,
    x: f64,
    y: f64,
    button_name: &str,
    click_count: i64,
    delay_ms: u64,
    modifiers: i64,
) -> Result<()> {
    use chromiumoxide::cdp::browser_protocol::input::{
        DispatchMouseEventParams, DispatchMouseEventType,
    };

    let (button, buttons) = parse_mouse_button(button_name)?;
    page.execute(
        DispatchMouseEventParams::builder()
            .r#type(DispatchMouseEventType::MouseMoved)
            .x(x)
            .y(y)
            .modifiers(modifiers)
            .build()
            .map_err(|e| anyhow!("mouse click move build failed: {}", e))?,
    )
    .await
    .map_err(|e| anyhow!("mouse click move failed: {}", e))?;

    let down = DispatchMouseEventParams::builder()
        .r#type(DispatchMouseEventType::MousePressed)
        .x(x)
        .y(y)
        .button(button.clone())
        .buttons(buttons)
        .click_count(click_count)
        .modifiers(modifiers)
        .build()
        .map_err(|e| anyhow!("mouse click down build failed: {}", e))?;
    page.execute(down)
        .await
        .map_err(|e| anyhow!("mouse click down failed: {}", e))?;
    if delay_ms > 0 {
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
    }
    let up = DispatchMouseEventParams::builder()
        .r#type(DispatchMouseEventType::MouseReleased)
        .x(x)
        .y(y)
        .button(button)
        .buttons(buttons)
        .click_count(click_count)
        .modifiers(modifiers)
        .build()
        .map_err(|e| anyhow!("mouse click up build failed: {}", e))?;
    page.execute(up)
        .await
        .map_err(|e| anyhow!("mouse click up failed: {}", e))?;
    Ok(())
}

async fn maybe_write_json_file(path: &str, value: &Value) -> Result<String> {
    let data = serde_json::to_vec_pretty(value)
        .map_err(|e| anyhow!("serialize json output failed: {}", e))?;
    std::fs::write(path, data)
        .map_err(|e| anyhow!("write json output '{}' failed: {}", path, e))?;
    Ok(path.to_string())
}

async fn maybe_write_base64_file(path: &str, encoded: &str) -> Result<String> {
    let bytes = BASE64_STANDARD
        .decode(encoded)
        .map_err(|e| anyhow!("base64 decode failed for '{}': {}", path, e))?;
    std::fs::write(path, bytes)
        .map_err(|e| anyhow!("write binary output '{}' failed: {}", path, e))?;
    Ok(path.to_string())
}

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

async fn run_navigate_back(page: &chromiumoxide::Page) -> Result<Value> {
    page.evaluate("history.back()")
        .await
        .map_err(|e| anyhow!("navigate_back failed: {}", e))?;
    browser_manager::wait_until_stable(page, 800, 5000).await.ok();
    let remote = page
        .evaluate("location.href")
        .await
        .map_err(|e| anyhow!("navigate_back location read failed: {}", e))?;
    let current = remote.into_value::<String>().unwrap_or_default();
    Ok(json!({ "navigated_back": true, "url": current }))
}

async fn run_click(page: &chromiumoxide::Page, step: &Value, timeout_ms: u64) -> Result<Value> {
    let selector = step.get("target").and_then(|v| v.as_str()).unwrap_or("");
    if selector.is_empty() {
        return Err(anyhow!("click: 'target' (CSS selector) is required"));
    }
    run_wait_for_selector(page, selector, timeout_ms).await?;
    let button_name = step.get("button").and_then(|v| v.as_str()).unwrap_or("left");
    let double_click = step_bool(step, "doubleClick")
        .or_else(|| step_bool(step, "double_click"))
        .unwrap_or(false);
    let click_count = step_i64(step, "clickCount")
        .or_else(|| step_i64(step, "click_count"))
        .unwrap_or(if double_click { 2 } else { 1 });
    let delay_ms = step_u64(step, "delay")
        .or_else(|| step_u64(step, "delay_ms"))
        .unwrap_or(0);
    let modifiers = mouse_modifiers_from_step(step);
    let (x, y) = selector_center(page, selector).await?;
    debug!("🖱️ click → {} @ ({}, {})", selector, x, y);
    dispatch_mouse_click(page, x, y, button_name, click_count, delay_ms, modifiers).await?;
    Ok(json!({
        "clicked": selector,
        "button": button_name,
        "click_count": click_count,
        "modifiers": modifiers
    }))
}

async fn run_hover(page: &chromiumoxide::Page, step: &Value, timeout_ms: u64) -> Result<Value> {
    let selector = step.get("target").and_then(|v| v.as_str()).unwrap_or("");
    let (x, y) = if !selector.is_empty() {
        run_wait_for_selector(page, selector, timeout_ms).await?;
        selector_center(page, selector).await?
    } else {
        run_wait_for_locator(page, step, timeout_ms).await?;
        locator_center(page, step).await?
    };
    use chromiumoxide::cdp::browser_protocol::input::{
        DispatchMouseEventParams, DispatchMouseEventType,
    };
    page.execute(
        DispatchMouseEventParams::builder()
            .r#type(DispatchMouseEventType::MouseMoved)
            .x(x)
            .y(y)
            .modifiers(mouse_modifiers_from_step(step))
            .build()
            .map_err(|e| anyhow!("hover build failed: {}", e))?,
    )
    .await
    .map_err(|e| anyhow!("hover failed: {}", e))?;
    Ok(json!({ "hovered": if !selector.is_empty() { selector } else { step.get("target").and_then(|v| v.as_str()).unwrap_or("") }, "x": x, "y": y }))
}

async fn run_type(page: &chromiumoxide::Page, step: &Value, timeout_ms: u64) -> Result<Value> {
    let selector = step.get("target").and_then(|v| v.as_str()).unwrap_or("");
    let text = step.get("value").and_then(|v| v.as_str()).unwrap_or("");
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
    let slowly = step_bool(step, "slowly").unwrap_or(false);
    if slowly {
        for ch in text.chars() {
            run_press_key(page, &ch.to_string()).await?;
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    } else {
        elem.type_str(text)
            .await
            .map_err(|e| anyhow!("type: dispatch failed: {}", e))?;
    }
    if step_bool(step, "submit").unwrap_or(false) {
        run_press_key(page, "Enter").await?;
    }
    Ok(json!({ "typed": text, "into": selector, "slowly": slowly }))
}

async fn run_click_locator(page: &chromiumoxide::Page, step: &Value, timeout_ms: u64) -> Result<Value> {
        let (strategy, value, name, exact, scope) = build_locator_js(step);
        if value.is_empty() {
                return Err(anyhow!("click_locator: 'target' locator value is required"));
        }
        let script = format!(
                r#"(function() {{
    var found = {resolve};
    if (!found.ok) return found;
    var strategy = {strategy};
    var value = {value};
    var roleName = {name};
    var exact = {exact};
    var scopeSel = {scope};
    var root = scopeSel ? document.querySelector(scopeSel) : document;
    if (!root) return {{ ok:false, reason:'locator scope not found' }};
    function norm(s) {{ return (s || '').replace(/\s+/g, ' ').trim(); }}
    function matchText(actual, wanted) {{
        if (!wanted) return false;
        var a = norm(actual).toLowerCase();
        var w = norm(wanted).toLowerCase();
        return exact ? a === w : a.indexOf(w) !== -1;
    }}
    function visible(el) {{
        if (!el) return false;
        var st = window.getComputedStyle(el);
        var r = el.getBoundingClientRect();
        return st.display !== 'none' && st.visibility !== 'hidden' && st.opacity !== '0' && r.width > 0 && r.height > 0;
    }}
    function firstVisible(list) {{
        for (var i = 0; i < list.length; i++) if (visible(list[i])) return list[i];
        return list[0] || null;
    }}
    function find() {{
        var el = null;
        if (strategy === 'css') el = root.querySelector(value);
        else if (strategy === 'text') {{
            var c = root.querySelectorAll('button,a,[role],label,div,span,p,h1,h2,h3,h4,h5,h6,li');
            var out = [];
            for (var i = 0; i < c.length; i++) if (matchText(c[i].innerText || c[i].textContent, value)) out.push(c[i]);
            el = firstVisible(out);
        }} else if (strategy === 'role') {{
            var roleSel = '[role="' + value + '"]';
            var roleCands = Array.prototype.slice.call(root.querySelectorAll(roleSel));
            if (value === 'button') roleCands = roleCands.concat(Array.prototype.slice.call(root.querySelectorAll('button,input[type="button"],input[type="submit"]')));
            if (value === 'textbox') roleCands = roleCands.concat(Array.prototype.slice.call(root.querySelectorAll('input[type="text"],textarea,input:not([type])')));
            if (roleName) roleCands = roleCands.filter(function(n) {{ return matchText(n.innerText || n.textContent || n.getAttribute('aria-label'), roleName); }});
            el = firstVisible(roleCands);
        }} else if (strategy === 'label') {{
            var labels = root.querySelectorAll('label');
            for (var j = 0; j < labels.length; j++) {{
                var lb = labels[j];
                if (!matchText(lb.innerText || lb.textContent, value)) continue;
                var forId = lb.getAttribute('for');
                if (forId) el = document.getElementById(forId);
                else el = lb.querySelector('input,textarea,select');
                if (el) break;
            }}
        }} else if (strategy === 'placeholder') {{
            var inputs = root.querySelectorAll('input[placeholder],textarea[placeholder]');
            for (var k = 0; k < inputs.length; k++) if (matchText(inputs[k].getAttribute('placeholder'), value)) {{ el = inputs[k]; break; }}
        }} else if (strategy === 'testid') {{
            var tid = value.replace(/"/g, '\\"');
            el = root.querySelector('[data-testid="' + tid + '"]');
        }}
        return el;
    }}
    var el = find();
    if (!el) return {{ ok:false, reason:'locator did not match any element' }};
    el.click();
    return {{ ok:true, strategy:strategy, clicked:value, tag:(el.tagName || '').toLowerCase() }};
}})()"#,
                resolve = locator_resolve_script(&strategy, &value, name.as_deref(), exact, scope.as_deref()),
                strategy = serde_json::to_string(&strategy).unwrap_or_else(|_| "\"css\"".to_string()),
                value = serde_json::to_string(&value).unwrap_or_else(|_| "\"\"".to_string()),
                name = serde_json::to_string(name.as_deref().unwrap_or(""))
                        .unwrap_or_else(|_| "\"\"".to_string()),
                exact = if exact { "true" } else { "false" },
                scope = serde_json::to_string(scope.as_deref().unwrap_or(""))
                        .unwrap_or_else(|_| "\"\"".to_string())
        );

        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        loop {
                let remote = page
                        .evaluate(script.clone())
                        .await
                        .map_err(|e| anyhow!("click_locator: evaluate failed: {}", e))?;
                let result = remote.into_value::<Value>().unwrap_or(Value::Null);
                if result.get("ok").and_then(|v| v.as_bool()) == Some(true) {
                        return Ok(result);
                }
                if Instant::now() >= deadline {
                return Err(anyhow!("click_locator timed out after {}ms: {}", timeout_ms, result));
                }
                tokio::time::sleep(Duration::from_millis(200)).await;
        }
}

async fn run_type_locator(page: &chromiumoxide::Page, step: &Value, timeout_ms: u64) -> Result<Value> {
        let (strategy, value, name, exact, scope) = build_locator_js(step);
        let text = step.get("value").and_then(|v| v.as_str()).unwrap_or("");
        if value.is_empty() {
                return Err(anyhow!("type_locator: 'target' locator value is required"));
        }
        if text.is_empty() {
                return Err(anyhow!("type_locator: 'value' text is required"));
        }
        let text_js = serde_json::to_string(text).unwrap_or_else(|_| "\"\"".to_string());
        let script = format!(
                r#"(function() {{
    var strategy = {strategy};
    var value = {value};
    var roleName = {name};
    var exact = {exact};
    var scopeSel = {scope};
    var txt = {text};
    var root = scopeSel ? document.querySelector(scopeSel) : document;
    if (!root) return {{ ok:false, reason:'locator scope not found' }};
    function norm(s) {{ return (s || '').replace(/\s+/g, ' ').trim(); }}
    function matchText(actual, wanted) {{
        if (!wanted) return false;
        var a = norm(actual).toLowerCase();
        var w = norm(wanted).toLowerCase();
        return exact ? a === w : a.indexOf(w) !== -1;
    }}
    function firstVisible(list) {{
        for (var i = 0; i < list.length; i++) {{
            var el = list[i];
            var st = window.getComputedStyle(el);
            var r = el.getBoundingClientRect();
            if (st.display !== 'none' && st.visibility !== 'hidden' && st.opacity !== '0' && r.width > 0 && r.height > 0) return el;
        }}
        return list[0] || null;
    }}
    var el = null;
    if (strategy === 'css') el = root.querySelector(value);
    else if (strategy === 'label') {{
        var labels = root.querySelectorAll('label');
        for (var j = 0; j < labels.length; j++) {{
            var lb = labels[j];
            if (!matchText(lb.innerText || lb.textContent, value)) continue;
            var forId = lb.getAttribute('for');
            if (forId) el = document.getElementById(forId);
            else el = lb.querySelector('input,textarea,select');
            if (el) break;
        }}
    }} else if (strategy === 'placeholder') {{
        var inputs = root.querySelectorAll('input[placeholder],textarea[placeholder]');
        for (var k = 0; k < inputs.length; k++) if (matchText(inputs[k].getAttribute('placeholder'), value)) {{ el = inputs[k]; break; }}
    }} else if (strategy === 'testid') {{
        var tid = value.replace(/"/g, '\\"');
        el = root.querySelector('[data-testid="' + tid + '"]');
    }} else if (strategy === 'text') {{
        var c = root.querySelectorAll('input,textarea,[contenteditable="true"]');
        var out = [];
        for (var i = 0; i < c.length; i++) {{
            var n = c[i];
            if (matchText(n.getAttribute('aria-label') || n.placeholder || n.name || n.id, value)) out.push(n);
        }}
        el = firstVisible(out);
    }} else if (strategy === 'role') {{
        var roleSel = '[role="' + value + '"]';
        var roleCands = Array.prototype.slice.call(root.querySelectorAll(roleSel));
        if (value === 'textbox') roleCands = roleCands.concat(Array.prototype.slice.call(root.querySelectorAll('input[type="text"],textarea,input:not([type])')));
        if (roleName) roleCands = roleCands.filter(function(n) {{ return matchText(n.innerText || n.textContent || n.getAttribute('aria-label'), roleName); }});
        el = firstVisible(roleCands);
    }}
    if (!el) return {{ ok:false, reason:'locator did not match any input-like element' }};
    el.focus();
    if ('value' in el) el.value = txt;
    else el.textContent = txt;
    el.dispatchEvent(new Event('input', {{ bubbles:true }}));
    el.dispatchEvent(new Event('change', {{ bubbles:true }}));
    return {{ ok:true, strategy:strategy, typed:txt, tag:(el.tagName || '').toLowerCase() }};
}})()"#,
                strategy = serde_json::to_string(&strategy).unwrap_or_else(|_| "\"css\"".to_string()),
                value = serde_json::to_string(&value).unwrap_or_else(|_| "\"\"".to_string()),
                name = serde_json::to_string(name.as_deref().unwrap_or(""))
                        .unwrap_or_else(|_| "\"\"".to_string()),
                exact = if exact { "true" } else { "false" },
                scope = serde_json::to_string(scope.as_deref().unwrap_or(""))
                        .unwrap_or_else(|_| "\"\"".to_string()),
                text = text_js
        );

        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        loop {
                let remote = page
                        .evaluate(script.clone())
                        .await
                        .map_err(|e| anyhow!("type_locator: evaluate failed: {}", e))?;
                let result = remote.into_value::<Value>().unwrap_or(Value::Null);
                if result.get("ok").and_then(|v| v.as_bool()) == Some(true) {
                        return Ok(result);
                }
                if Instant::now() >= deadline {
                return Err(anyhow!("type_locator timed out after {}ms: {}", timeout_ms, result));
                }
                tokio::time::sleep(Duration::from_millis(200)).await;
        }
}

async fn run_wait_for_locator(page: &chromiumoxide::Page, step: &Value, timeout_ms: u64) -> Result<Value> {
        let (strategy, value, name, exact, scope) = build_locator_js(step);
        if value.is_empty() {
                return Err(anyhow!("wait_for_locator: 'target' locator value is required"));
        }
        let script = locator_resolve_script(&strategy, &value, name.as_deref(), exact, scope.as_deref());
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        loop {
                let remote = page
                        .evaluate(script.clone())
                        .await
                        .map_err(|e| anyhow!("wait_for_locator: evaluate failed: {}", e))?;
                let result = remote.into_value::<Value>().unwrap_or(Value::Null);
                if result.get("ok").and_then(|v| v.as_bool()) == Some(true) {
                        return Ok(json!({ "found": value, "strategy": strategy }));
                }
                if Instant::now() >= deadline {
                return Err(anyhow!("wait_for_locator timed out after {}ms: {}", timeout_ms, result));
                }
                tokio::time::sleep(Duration::from_millis(200)).await;
        }
}

async fn run_assert_locator(
        page: &chromiumoxide::Page,
        step: &Value,
        condition: &str,
        timeout_ms: u64,
) -> Result<Value> {
        let (strategy, value, name, exact, scope) = build_locator_js(step);
        if value.is_empty() {
                return Err(anyhow!("assert_locator: 'target' locator value is required"));
        }
        let expected_value = step.get("value").and_then(|v| v.as_str()).unwrap_or("");
        let resolve = locator_resolve_script(&strategy, &value, name.as_deref(), exact, scope.as_deref());
        let expected_js = serde_json::to_string(expected_value).unwrap_or_else(|_| "\"\"".to_string());
        let condition_js = serde_json::to_string(condition).unwrap_or_else(|_| "\"contains_text\"".to_string());

        let script = format!(
                r#"(function() {{
    var cond = {condition};
    var expected = {expected};
    var resolved = {resolve};
    if (!resolved.ok) {{
        return cond === 'is_hidden' ? {{ ok:true, reason:'not found counts as hidden' }} : resolved;
    }}
    var strategy = {strategy};
    var value = {value};
    var roleName = {name};
    var exact = {exact};
    var scopeSel = {scope};
    var root = scopeSel ? document.querySelector(scopeSel) : document;
    function norm(s) {{ return (s || '').replace(/\s+/g, ' ').trim(); }}
    function matchText(actual, wanted) {{
        if (!wanted) return false;
        var a = norm(actual).toLowerCase();
        var w = norm(wanted).toLowerCase();
        return exact ? a === w : a.indexOf(w) !== -1;
    }}
    function firstVisible(list) {{
        for (var i = 0; i < list.length; i++) {{
            var el = list[i];
            var st = window.getComputedStyle(el);
            var r = el.getBoundingClientRect();
            if (st.display !== 'none' && st.visibility !== 'hidden' && st.opacity !== '0' && r.width > 0 && r.height > 0) return el;
        }}
        return list[0] || null;
    }}
    function find() {{
        var el = null;
        if (strategy === 'css') el = root.querySelector(value);
        else if (strategy === 'text') {{
            var c = root.querySelectorAll('button,a,[role],label,div,span,p,h1,h2,h3,h4,h5,h6,li');
            var out = [];
            for (var i = 0; i < c.length; i++) if (matchText(c[i].innerText || c[i].textContent, value)) out.push(c[i]);
            el = firstVisible(out);
        }} else if (strategy === 'role') {{
            var roleSel = '[role="' + value + '"]';
            var roleCands = Array.prototype.slice.call(root.querySelectorAll(roleSel));
            if (value === 'button') roleCands = roleCands.concat(Array.prototype.slice.call(root.querySelectorAll('button,input[type="button"],input[type="submit"]')));
            if (value === 'textbox') roleCands = roleCands.concat(Array.prototype.slice.call(root.querySelectorAll('input[type="text"],textarea,input:not([type])')));
            if (roleName) roleCands = roleCands.filter(function(n) {{ return matchText(n.innerText || n.textContent || n.getAttribute('aria-label'), roleName); }});
            el = firstVisible(roleCands);
        }} else if (strategy === 'label') {{
            var labels = root.querySelectorAll('label');
            for (var j = 0; j < labels.length; j++) {{
                var lb = labels[j];
                if (!matchText(lb.innerText || lb.textContent, value)) continue;
                var forId = lb.getAttribute('for');
                if (forId) el = document.getElementById(forId);
                else el = lb.querySelector('input,textarea,select');
                if (el) break;
            }}
        }} else if (strategy === 'placeholder') {{
            var inputs = root.querySelectorAll('input[placeholder],textarea[placeholder]');
            for (var k = 0; k < inputs.length; k++) if (matchText(inputs[k].getAttribute('placeholder'), value)) {{ el = inputs[k]; break; }}
        }} else if (strategy === 'testid') {{
            var tid = value.replace(/"/g, '\\"');
            el = root.querySelector('[data-testid="' + tid + '"]');
        }}
        return el;
    }}
    var el = find();
    if (!el) return cond === 'is_hidden' ? {{ ok:true }} : {{ ok:false, reason:'locator element not found' }};
    var st = window.getComputedStyle(el);
    var r = el.getBoundingClientRect();
    var visible = st.display !== 'none' && st.visibility !== 'hidden' && st.opacity !== '0' && r.width > 0 && r.height > 0;
    if (cond === 'is_visible') return {{ ok:visible }};
    if (cond === 'is_hidden') return {{ ok:!visible }};
    var text = norm(el.innerText || el.textContent || el.value || '');
    var exp = norm(expected);
    return {{ ok: text.toLowerCase().indexOf(exp.toLowerCase()) !== -1, actual:text.slice(0,240) }};
}})()"#,
                condition = condition_js,
                expected = expected_js,
                resolve = resolve,
                strategy = serde_json::to_string(&strategy).unwrap_or_else(|_| "\"css\"".to_string()),
                value = serde_json::to_string(&value).unwrap_or_else(|_| "\"\"".to_string()),
                name = serde_json::to_string(name.as_deref().unwrap_or(""))
                        .unwrap_or_else(|_| "\"\"".to_string()),
                exact = if exact { "true" } else { "false" },
                scope = serde_json::to_string(scope.as_deref().unwrap_or(""))
                        .unwrap_or_else(|_| "\"\"".to_string())
        );

        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        loop {
                let remote = page
                        .evaluate(script.clone())
                        .await
                        .map_err(|e| anyhow!("assert_locator: evaluate failed: {}", e))?;
                let result = remote.into_value::<Value>().unwrap_or(Value::Null);
                if result.get("ok").and_then(|v| v.as_bool()) == Some(true) {
                        return Ok(json!({
                                "asserted": condition,
                                "locator_strategy": strategy,
                                "target": value,
                                "passed": true,
                                "timeout_ms": timeout_ms
                        }));
                }
                if Instant::now() >= deadline {
                        return Err(anyhow!(
                                "assert_locator failed after {}ms (condition={}): {}",
                                timeout_ms,
                                condition,
                                result
                        ));
                }
                tokio::time::sleep(Duration::from_millis(200)).await;
        }
}

async fn run_storage_checkpoint(page: &chromiumoxide::Page, key: &str) -> Result<Value> {
        if key.is_empty() {
                return Err(anyhow!("storage_checkpoint: 'target' key is required"));
        }
    let state = run_storage_state_export(page, &json!({})).await?;
        let mut fixtures = STORAGE_FIXTURES
                .lock()
                .map_err(|_| anyhow!("storage_checkpoint: fixture state lock poisoned"))?;
        fixtures.insert(key.to_string(), state.clone());
        Ok(json!({ "checkpointed": key, "keys": fixtures.len() }))
}

async fn run_storage_rollback(page: &chromiumoxide::Page, key: &str) -> Result<Value> {
        if key.is_empty() {
                return Err(anyhow!("storage_rollback: 'target' key is required"));
        }
    let state = {
        let fixtures = STORAGE_FIXTURES
            .lock()
            .map_err(|_| anyhow!("storage_rollback: fixture state lock poisoned"))?;
        fixtures
            .get(key)
            .cloned()
            .ok_or_else(|| anyhow!("storage_rollback: checkpoint '{}' not found", key))?
    };
        let raw = serde_json::to_string(&state)
                .map_err(|e| anyhow!("storage_rollback: serialize failed: {}", e))?;
        let result = run_storage_state_import(page, &json!({ "value": raw })).await?;
        Ok(json!({ "rolled_back": key, "result": result }))
}

async fn run_network_tap(page: &chromiumoxide::Page) -> Result<Value> {
        use chromiumoxide::cdp::browser_protocol::page::AddScriptToEvaluateOnNewDocumentParams;

        let script = r#"
(function() {
    if (window.__cortexNetworkTapInstalled) {
        return { status: 'already_installed' };
    }
    window.__cortexNetworkTapInstalled = true;
    window.__cortexNetworkEvents = window.__cortexNetworkEvents || [];
    var maxEvents = 400;
    function pushEvent(ev) {
        try {
            window.__cortexNetworkEvents.push(ev);
            if (window.__cortexNetworkEvents.length > maxEvents) {
                window.__cortexNetworkEvents.splice(0, window.__cortexNetworkEvents.length - maxEvents);
            }
        } catch (_) {}
    }

    var origFetch = window.fetch ? window.fetch.bind(window) : null;
    if (origFetch) {
        window.fetch = async function(resource, init) {
            var url = typeof resource === 'string' ? resource : (resource && resource.url ? resource.url : String(resource));
            var method = (init && init.method) || 'GET';
            var start = Date.now();
            try {
                var res = await origFetch(resource, init);
                pushEvent({ ts: new Date().toISOString(), transport:'fetch', method: method, url: url, status: res.status, ok: res.ok, duration_ms: Date.now() - start });
                return res;
            } catch (err) {
                pushEvent({ ts: new Date().toISOString(), transport:'fetch', method: method, url: url, error: String(err), duration_ms: Date.now() - start });
                throw err;
            }
        };
    }

    var OrigXHR = XMLHttpRequest;
    XMLHttpRequest = function() {
        var xhr = new OrigXHR();
        var method = 'GET';
        var url = '';
        var start = 0;
        var open = xhr.open;
        xhr.open = function(m, u) {
            method = m || 'GET';
            url = u || '';
            return open.apply(xhr, arguments);
        };
        var send = xhr.send;
        xhr.send = function() {
            start = Date.now();
            xhr.addEventListener('loadend', function() {
                pushEvent({ ts: new Date().toISOString(), transport:'xhr', method: method, url: url, status: xhr.status, ok: xhr.status >= 200 && xhr.status < 400, duration_ms: Date.now() - start });
            });
            return send.apply(xhr, arguments);
        };
        return xhr;
    };

    return { status: 'installed' };
})();
"#;

        page.execute(AddScriptToEvaluateOnNewDocumentParams::new(script.to_string()))
            .await
            .map_err(|e| anyhow!("network_tap: addScript failed: {}", e))?;
        page.evaluate(script)
                .await
                .map_err(|e| anyhow!("network_tap: evaluate failed: {}", e))?;
        Ok(json!({ "network_tap": "installed" }))
}

async fn run_network_dump(page: &chromiumoxide::Page, step: &Value) -> Result<Value> {
        let include_static = step_bool(step, "includeStatic")
                .or_else(|| step_bool(step, "include_static"))
                .unwrap_or(false);
        let script = r#"
(function() {
    var events = window.__cortexNetworkEvents || [];
    if (__INCLUDE_STATIC__) {
        var resources = performance.getEntriesByType('resource').map(function(entry) {
            return {
                ts: new Date().toISOString(),
                transport: 'resource',
                method: 'GET',
                url: entry.name,
                status: 200,
                ok: true,
                duration_ms: Math.round(entry.duration || 0)
            };
        });
        events = events.concat(resources);
    }
    var failed = events.filter(function(e) { return !!e.error || (typeof e.status === 'number' && e.status >= 400); }).length;
    return { total: events.length, failed: failed, events: events };
})();
"#
        .replace("__INCLUDE_STATIC__", if include_static { "true" } else { "false" });
        let remote = page
                .evaluate(script)
                .await
                .map_err(|e| anyhow!("network_dump: evaluate failed: {}", e))?;
        let value = remote.into_value::<Value>().unwrap_or(Value::Null);
        if let Some(path) = step.get("filename").and_then(|v| v.as_str()) {
                let _ = maybe_write_json_file(path, &value).await?;
        }
        Ok(value)
}

async fn run_evaluate(page: &chromiumoxide::Page, step: &Value) -> Result<Value> {
    let script = step.get("value").and_then(|v| v.as_str()).unwrap_or("");
    if script.is_empty() {
        return Err(anyhow!("evaluate: 'value' (JS expression) is required"));
    }
    let selector = step.get("target").and_then(|v| v.as_str()).unwrap_or("");
    debug!("📜 evaluate: {}", script.chars().take(80).collect::<String>());
    let runtime_script = if selector.is_empty() {
        script.to_string()
    } else {
        let selector_js = serde_json::to_string(selector).unwrap_or_else(|_| "\"\"".to_string());
        if script.contains("=>") || script.trim_start().starts_with("function") || script.trim_start().starts_with("async") {
            format!(
                r#"(async function() {{
  var element = document.querySelector({selector});
  if (!element) throw new Error('evaluate: selector not found');
  return await ({script})(element);
}})()"#,
                selector = selector_js,
                script = script,
            )
        } else {
            format!(
                r#"(function() {{
  var element = document.querySelector({selector});
  if (!element) throw new Error('evaluate: selector not found');
  return (function(element) {{ {script} }})(element);
}})()"#,
                selector = selector_js,
                script = script,
            )
        }
    };
    let remote = page
        .evaluate(runtime_script)
        .await
        .map_err(|e| anyhow!("evaluate failed: {}", e))?;
    let val = remote
        .into_value::<Value>()
        .unwrap_or(Value::Null);
    Ok(val)
}

async fn run_wait_for(page: &chromiumoxide::Page, step: &Value) -> Result<Value> {
    let time_secs = step_f64(step, "time");
    let text = step.get("text").and_then(|v| v.as_str()).unwrap_or("");
    let text_gone = step
        .get("textGone")
        .or_else(|| step.get("text_gone"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if let Some(secs) = time_secs {
        tokio::time::sleep(Duration::from_millis((secs * 1000.0) as u64)).await;
        return Ok(json!({ "waited_seconds": secs }));
    }

    if text.is_empty() && text_gone.is_empty() {
        return Err(anyhow!("wait_for: provide 'time', 'text', or 'textGone'"));
    }

    let deadline = Instant::now() + Duration::from_millis(step_u64(step, "timeout_ms").unwrap_or(10_000));
    loop {
        let body = page
            .evaluate("document.body ? document.body.innerText : ''")
            .await
            .map_err(|e| anyhow!("wait_for evaluate failed: {}", e))?
            .into_value::<String>()
            .unwrap_or_default();

        let text_ok = text.is_empty() || body.contains(text);
        let text_gone_ok = text_gone.is_empty() || !body.contains(text_gone);
        if text_ok && text_gone_ok {
            return Ok(json!({
                "wait_for": true,
                "text": if text.is_empty() { Value::Null } else { Value::String(text.to_string()) },
                "text_gone": if text_gone.is_empty() { Value::Null } else { Value::String(text_gone.to_string()) }
            }));
        }
        if Instant::now() >= deadline {
            return Err(anyhow!("wait_for timed out"));
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
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

async fn run_snapshot(page: &chromiumoxide::Page, step: &Value) -> Result<Value> {
    debug!("📸 snapshot");
    let selector = step.get("target").and_then(|v| v.as_str()).unwrap_or("");
    let script = if selector.is_empty() {
        SNAPSHOT_SCRIPT.to_string()
    } else {
        let selector_js = serde_json::to_string(selector).unwrap_or_else(|_| "\"\"".to_string());
        format!(
            r#"JSON.stringify((function() {{
  var root = document.querySelector({selector});
  if (!root) return {{ error:'snapshot selector not found', selector:{selector} }};
  return {{
    title: document.title,
    url: location.href,
    selector: {selector},
    text: (root.innerText || root.textContent || '').slice(0, 3000),
    html: (root.outerHTML || '').slice(0, 6000)
  }};
}})())"#,
            selector = selector_js,
        )
    };
    let remote = page
        .evaluate(script)
        .await
        .map_err(|e| anyhow!("snapshot evaluate failed: {}", e))?;
    // The script returns a JSON string — parse it into a Value so the outer
    // response is clean JSON (not a double-encoded string).
    let raw = remote
        .into_value::<Value>()
        .unwrap_or(Value::Null);
    if let Value::String(s) = &raw {
        let parsed: Value = serde_json::from_str(s).map_err(|e| anyhow!("snapshot parse failed: {}", e))?;
        if let Some(path) = step.get("filename").and_then(|v| v.as_str()) {
            let _ = maybe_write_json_file(path, &parsed).await?;
        }
        return Ok(parsed);
    }
    if let Some(path) = step.get("filename").and_then(|v| v.as_str()) {
        let _ = maybe_write_json_file(path, &raw).await?;
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

async fn run_resize(page: &chromiumoxide::Page, step: &Value) -> Result<Value> {
    use chromiumoxide::cdp::browser_protocol::emulation::SetDeviceMetricsOverrideParams;

    let width = step_i64(step, "width").unwrap_or(1280);
    let height = step_i64(step, "height").unwrap_or(800);
    let cmd = SetDeviceMetricsOverrideParams::builder()
        .width(width)
        .height(height)
        .device_scale_factor(1.0)
        .mobile(false)
        .screen_width(width)
        .screen_height(height)
        .build()
        .map_err(|e| anyhow!("resize build failed: {}", e))?;
    page.execute(cmd)
        .await
        .map_err(|e| anyhow!("resize failed: {}", e))?;
    Ok(json!({ "width": width, "height": height }))
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

async fn run_screenshot(page: &chromiumoxide::Page, step: &Value) -> Result<Value> {
    use chromiumoxide::cdp::browser_protocol::page::{
        CaptureScreenshotFormat, CaptureScreenshotParams, Viewport,
    };
    debug!("📷 screenshot");
    let image_type = step
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("png");
    let format = match image_type {
        "jpeg" | "jpg" => CaptureScreenshotFormat::Jpeg,
        "webp" => CaptureScreenshotFormat::Webp,
        _ => CaptureScreenshotFormat::Png,
    };
    let full_page = step_bool(step, "fullPage")
        .or_else(|| step_bool(step, "full_page"))
        .unwrap_or(false);
    let selector = step.get("target").and_then(|v| v.as_str()).unwrap_or("");
    let mut builder = CaptureScreenshotParams::builder()
        .format(format)
        .capture_beyond_viewport(full_page)
        .from_surface(true);
    if !selector.is_empty() {
        let (x, y, width, height) = selector_rect(page, selector).await?;
        builder = builder.clip(
            Viewport::builder()
                .x(x)
                .y(y)
                .width(width)
                .height(height)
                .scale(1.0)
                .build()
                .map_err(|e| anyhow!("screenshot clip build failed: {}", e))?,
        );
    }
    let params = builder.build();
    let result = page
        .execute(params)
        .await
        .map_err(|e| anyhow!("screenshot: capture failed: {}", e))?;
    let screenshot_bytes: &[u8] = <chromiumoxide::Binary as AsRef<[u8]>>::as_ref(&result.result.data);
    let b64 = BASE64_STANDARD.encode(screenshot_bytes);
    let mut payload = json!({
        "format": image_type,
        "encoding": "base64",
        "data": b64
    });
    if let Some(path) = step.get("filename").and_then(|v| v.as_str()) {
        let written = maybe_write_base64_file(path, &b64).await?;
        payload["path"] = Value::String(written);
    }
    Ok(payload)
}

async fn run_select_option(
    page: &chromiumoxide::Page,
    step: &Value,
    timeout_ms: u64,
) -> Result<Value> {
    let selector = step.get("target").and_then(|v| v.as_str()).unwrap_or("");
    let option_values = step_string_vec(step, "values");
    let option_value = if !option_values.is_empty() {
        option_values[0].as_str()
    } else {
        step.get("value").and_then(|v| v.as_str()).unwrap_or("")
    };
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

async fn run_file_upload(page: &chromiumoxide::Page, step: &Value, timeout_ms: u64) -> Result<Value> {
    use chromiumoxide::cdp::browser_protocol::dom::SetFileInputFilesParams;

    let selector = step.get("target").and_then(|v| v.as_str()).unwrap_or("");
    let paths = step_string_vec(step, "paths");
    if selector.is_empty() {
        return Err(anyhow!("file_upload: 'target' selector is required"));
    }
    run_wait_for_selector(page, selector, timeout_ms).await?;
    let element = page
        .find_element(selector)
        .await
        .map_err(|e| anyhow!("file_upload: selector '{}' not found: {}", selector, e))?;
    let params = SetFileInputFilesParams::builder()
        .files(paths.clone())
        .object_id(element.remote_object_id.clone())
        .build()
        .map_err(|e| anyhow!("file_upload build failed: {}", e))?;
    page.execute(params)
        .await
        .map_err(|e| anyhow!("file_upload failed: {}", e))?;
    Ok(json!({ "uploaded": paths, "selector": selector }))
}

async fn run_fill_form(page: &chromiumoxide::Page, step: &Value, timeout_ms: u64) -> Result<Value> {
    let fields = step
        .get("fields")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("fill_form: 'fields' array is required"))?;
    let mut results = Vec::new();
    for field in fields {
        let selector = field
            .get("selector")
            .or_else(|| field.get("target"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("fill_form: each field requires selector/target"))?;
        let field_type = field.get("type").and_then(|v| v.as_str()).unwrap_or("textbox");
        run_wait_for_selector(page, selector, timeout_ms).await?;
        let selector_js = serde_json::to_string(selector).unwrap_or_else(|_| "\"\"".to_string());
        let value = field.get("value").cloned().unwrap_or(Value::Null);
        let value_js = serde_json::to_string(&value).unwrap_or_else(|_| "null".to_string());
        let field_type_js = serde_json::to_string(field_type).unwrap_or_else(|_| "\"textbox\"".to_string());
        let script = format!(
            r#"(function() {{
  var el = document.querySelector({selector});
  if (!el) return {{ ok:false, reason:'field not found' }};
  var kind = {field_type};
  var value = {value};
  el.scrollIntoView({{ block:'center', inline:'center' }});
  if (kind === 'checkbox' || kind === 'radio') {{
    el.checked = value === true || value === 'true';
  }} else if (kind === 'combobox' && el.tagName.toLowerCase() === 'select') {{
    for (var i = 0; i < el.options.length; i++) {{
      var opt = el.options[i];
      if (String(opt.value) === String(value) || (opt.textContent || '').trim() === String(value)) {{
        el.selectedIndex = i;
        break;
      }}
    }}
  }} else if (kind === 'slider') {{
    el.value = String(value);
  }} else if ('value' in el) {{
    el.value = String(value == null ? '' : value);
  }} else {{
    el.textContent = String(value == null ? '' : value);
  }}
  el.dispatchEvent(new Event('input', {{ bubbles:true }}));
  el.dispatchEvent(new Event('change', {{ bubbles:true }}));
  return {{ ok:true }};
}})()"#,
            selector = selector_js,
            field_type = field_type_js,
            value = value_js,
        );
        let remote = page
            .evaluate(script)
            .await
            .map_err(|e| anyhow!("fill_form evaluate failed: {}", e))?;
        let result = remote.into_value::<Value>().unwrap_or(Value::Null);
        if result.get("ok").and_then(|v| v.as_bool()) != Some(true) {
            return Err(anyhow!("fill_form failed on '{}': {}", selector, result));
        }
        results.push(json!({ "selector": selector, "type": field_type }));
    }
    Ok(json!({ "filled": results.len(), "results": results }))
}

async fn run_handle_dialog(page: &chromiumoxide::Page, step: &Value) -> Result<Value> {
    use chromiumoxide::cdp::browser_protocol::page::AddScriptToEvaluateOnNewDocumentParams;

    let accept = step_bool(step, "accept").unwrap_or(true);
    let prompt_text = step
        .get("promptText")
        .or_else(|| step.get("prompt_text"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    {
        let mut policy = DIALOG_POLICY
            .lock()
            .map_err(|_| anyhow!("handle_dialog: dialog policy lock poisoned"))?;
        *policy = Some(DialogPolicy {
            accept,
            prompt_text: prompt_text.clone(),
        });
    }

    let script = format!(
        r#"(function() {{
  window.__cortexDialogPolicy = {{ accept: {accept}, promptText: {prompt_text} }};
  if (window.__cortexDialogOverridesInstalled) return {{ installed:true, updated:true }};
  window.__cortexDialogOverridesInstalled = true;
  window.__cortexDialogEvents = window.__cortexDialogEvents || [];
  function record(kind, message) {{
    window.__cortexDialogEvents.push({{ ts:new Date().toISOString(), kind:kind, message:String(message || '') }});
  }}
  window.alert = function(message) {{ record('alert', message); return undefined; }};
  window.confirm = function(message) {{ record('confirm', message); return !!window.__cortexDialogPolicy.accept; }};
  window.prompt = function(message) {{ record('prompt', message); return window.__cortexDialogPolicy.accept ? (window.__cortexDialogPolicy.promptText || '') : null; }};
  return {{ installed:true, accept:window.__cortexDialogPolicy.accept }};
}})()"#,
        accept = if accept { "true" } else { "false" },
        prompt_text = serde_json::to_string(prompt_text.as_deref().unwrap_or("")).unwrap_or_else(|_| "\"\"".to_string()),
    );
    page.execute(AddScriptToEvaluateOnNewDocumentParams::new(script.clone()))
        .await
        .map_err(|e| anyhow!("handle_dialog addScript failed: {}", e))?;
    page.evaluate(script)
        .await
        .map_err(|e| anyhow!("handle_dialog evaluate failed: {}", e))?;
    Ok(json!({ "accept": accept, "prompt_text": prompt_text }))
}

async fn run_tabs(
    browser: &chromiumoxide::Browser,
    current_page: &mut chromiumoxide::Page,
    step: &Value,
) -> Result<Value> {
    let action = step.get("value").and_then(|v| v.as_str()).or_else(|| step.get("tab_action").and_then(|v| v.as_str())).or_else(|| step.get("action_name").and_then(|v| v.as_str())).unwrap_or("");
    let action = if action.is_empty() {
        step.get("target").and_then(|v| v.as_str()).unwrap_or("list")
    } else {
        action
    };
    match action {
        "list" => {
            let pages = browser.pages().await.map_err(|e| anyhow!("tabs list failed: {}", e))?;
            let active_id = current_page.target_id().clone();
            let mut rows = Vec::new();
            for (idx, page) in pages.iter().enumerate() {
                let url = page.url().await.ok().flatten().unwrap_or_default();
                let title = page.get_title().await.ok().flatten().unwrap_or_default();
                rows.push(json!({
                    "index": idx,
                    "url": url,
                    "title": title,
                    "active": page.target_id() == &active_id
                }));
            }
            Ok(json!({ "tabs": rows }))
        }
        "new" => {
            let url = step.get("target").and_then(|v| v.as_str()).filter(|s| s.starts_with("http") || *s == "about:blank").unwrap_or("about:blank");
            let page = browser.new_page(url).await.map_err(|e| anyhow!("tabs new failed: {}", e))?;
            page.activate().await.map_err(|e| anyhow!("tabs new activate failed: {}", e))?;
            *current_page = page.clone();
            Ok(json!({ "created": true, "url": url }))
        }
        "select" => {
            let index = step.get("index").and_then(|v| v.as_u64()).ok_or_else(|| anyhow!("tabs select: 'index' is required"))? as usize;
            let pages = browser.pages().await.map_err(|e| anyhow!("tabs select list failed: {}", e))?;
            let page = pages.get(index).cloned().ok_or_else(|| anyhow!("tabs select: index {} out of range", index))?;
            page.activate().await.map_err(|e| anyhow!("tabs select activate failed: {}", e))?;
            *current_page = page.clone();
            let url = page.url().await.ok().flatten().unwrap_or_default();
            Ok(json!({ "selected": index, "url": url }))
        }
        "close" => {
            let pages = browser.pages().await.map_err(|e| anyhow!("tabs close list failed: {}", e))?;
            let index = step.get("index").and_then(|v| v.as_u64()).map(|v| v as usize).unwrap_or_else(|| {
                pages.iter().position(|page| page.target_id() == current_page.target_id()).unwrap_or(0)
            });
            let page = pages.get(index).cloned().ok_or_else(|| anyhow!("tabs close: index {} out of range", index))?;
            page.close().await.map_err(|e| anyhow!("tabs close failed: {}", e))?;
            tokio::time::sleep(Duration::from_millis(150)).await;
            let mut remaining = browser.pages().await.map_err(|e| anyhow!("tabs post-close list failed: {}", e))?;
            if remaining.is_empty() {
                let page = browser.new_page("about:blank").await.map_err(|e| anyhow!("tabs close reopen failed: {}", e))?;
                *current_page = page.clone();
                return Ok(json!({ "closed": index, "reopened": true }));
            }
            let new_index = index.min(remaining.len() - 1);
            let new_page = remaining.swap_remove(new_index);
            let mut activate_error = None;
            for delay_ms in [0_u64, 150_u64] {
                if delay_ms > 0 {
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                }
                match new_page.activate().await {
                    Ok(_) => {
                        activate_error = None;
                        break;
                    }
                    Err(err) => activate_error = Some(err.to_string()),
                }
            }
            if let Some(err) = activate_error {
                return Err(anyhow!("tabs post-close activate failed: {}", err));
            }
            let url = new_page.url().await.ok().flatten().unwrap_or_default();
            *current_page = new_page;
            Ok(json!({ "closed": index, "active_index": new_index, "url": url }))
        }
        other => Err(anyhow!("tabs: unknown action '{}'. Valid: list, new, select, close", other)),
    }
}

async fn run_mouse_click_xy(page: &chromiumoxide::Page, step: &Value) -> Result<Value> {
    let x = step_f64(step, "x").ok_or_else(|| anyhow!("mouse_click_xy: 'x' is required"))?;
    let y = step_f64(step, "y").ok_or_else(|| anyhow!("mouse_click_xy: 'y' is required"))?;
    let button = step.get("button").and_then(|v| v.as_str()).unwrap_or("left");
    let click_count = step_i64(step, "clickCount").or_else(|| step_i64(step, "click_count")).unwrap_or(1);
    let delay_ms = step_u64(step, "delay").or_else(|| step_u64(step, "delay_ms")).unwrap_or(0);
    dispatch_mouse_click(page, x, y, button, click_count, delay_ms, mouse_modifiers_from_step(step)).await?;
    Ok(json!({ "x": x, "y": y, "button": button, "click_count": click_count }))
}

async fn run_mouse_down(page: &chromiumoxide::Page, step: &Value) -> Result<Value> {
    use chromiumoxide::cdp::browser_protocol::input::{DispatchMouseEventParams, DispatchMouseEventType};
    let button_name = step.get("button").and_then(|v| v.as_str()).unwrap_or("left");
    let (button, buttons) = parse_mouse_button(button_name)?;
    let x = step_f64(step, "x").unwrap_or(0.0);
    let y = step_f64(step, "y").unwrap_or(0.0);
    page.execute(
        DispatchMouseEventParams::builder()
            .r#type(DispatchMouseEventType::MousePressed)
            .x(x)
            .y(y)
            .button(button)
            .buttons(buttons)
            .modifiers(mouse_modifiers_from_step(step))
            .build()
            .map_err(|e| anyhow!("mouse_down build failed: {}", e))?,
    )
    .await
    .map_err(|e| anyhow!("mouse_down failed: {}", e))?;
    Ok(json!({ "button": button_name, "x": x, "y": y }))
}

async fn run_mouse_up(page: &chromiumoxide::Page, step: &Value) -> Result<Value> {
    use chromiumoxide::cdp::browser_protocol::input::{DispatchMouseEventParams, DispatchMouseEventType};
    let button_name = step.get("button").and_then(|v| v.as_str()).unwrap_or("left");
    let (button, buttons) = parse_mouse_button(button_name)?;
    let x = step_f64(step, "x").unwrap_or(0.0);
    let y = step_f64(step, "y").unwrap_or(0.0);
    page.execute(
        DispatchMouseEventParams::builder()
            .r#type(DispatchMouseEventType::MouseReleased)
            .x(x)
            .y(y)
            .button(button)
            .buttons(buttons)
            .modifiers(mouse_modifiers_from_step(step))
            .build()
            .map_err(|e| anyhow!("mouse_up build failed: {}", e))?,
    )
    .await
    .map_err(|e| anyhow!("mouse_up failed: {}", e))?;
    Ok(json!({ "button": button_name, "x": x, "y": y }))
}

async fn run_mouse_move_xy(page: &chromiumoxide::Page, step: &Value) -> Result<Value> {
    use chromiumoxide::cdp::browser_protocol::input::{DispatchMouseEventParams, DispatchMouseEventType};
    let x = step_f64(step, "x").ok_or_else(|| anyhow!("mouse_move_xy: 'x' is required"))?;
    let y = step_f64(step, "y").ok_or_else(|| anyhow!("mouse_move_xy: 'y' is required"))?;
    page.execute(
        DispatchMouseEventParams::builder()
            .r#type(DispatchMouseEventType::MouseMoved)
            .x(x)
            .y(y)
            .modifiers(mouse_modifiers_from_step(step))
            .build()
            .map_err(|e| anyhow!("mouse_move_xy build failed: {}", e))?,
    )
    .await
    .map_err(|e| anyhow!("mouse_move_xy failed: {}", e))?;
    Ok(json!({ "x": x, "y": y }))
}

async fn run_mouse_drag_xy(page: &chromiumoxide::Page, step: &Value) -> Result<Value> {
    let start_x = step_f64(step, "startX").or_else(|| step_f64(step, "start_x")).ok_or_else(|| anyhow!("mouse_drag_xy: 'startX' is required"))?;
    let start_y = step_f64(step, "startY").or_else(|| step_f64(step, "start_y")).ok_or_else(|| anyhow!("mouse_drag_xy: 'startY' is required"))?;
    let end_x = step_f64(step, "endX").or_else(|| step_f64(step, "end_x")).ok_or_else(|| anyhow!("mouse_drag_xy: 'endX' is required"))?;
    let end_y = step_f64(step, "endY").or_else(|| step_f64(step, "end_y")).ok_or_else(|| anyhow!("mouse_drag_xy: 'endY' is required"))?;
    run_mouse_move_xy(page, &json!({ "x": start_x, "y": start_y })).await?;
    run_mouse_down(page, &json!({ "x": start_x, "y": start_y, "button": "left" })).await?;
    run_mouse_move_xy(page, &json!({ "x": end_x, "y": end_y })).await?;
    run_mouse_up(page, &json!({ "x": end_x, "y": end_y, "button": "left" })).await?;
    Ok(json!({ "start_x": start_x, "start_y": start_y, "end_x": end_x, "end_y": end_y }))
}

async fn run_mouse_wheel(page: &chromiumoxide::Page, step: &Value) -> Result<Value> {
    use chromiumoxide::cdp::browser_protocol::input::{DispatchMouseEventParams, DispatchMouseEventType};
    let delta_x = step_f64(step, "deltaX").or_else(|| step_f64(step, "delta_x")).unwrap_or(0.0);
    let delta_y = step_f64(step, "deltaY").or_else(|| step_f64(step, "delta_y")).unwrap_or(0.0);
    page.execute(
        DispatchMouseEventParams::builder()
            .r#type(DispatchMouseEventType::MouseWheel)
            .x(step_f64(step, "x").unwrap_or(0.0))
            .y(step_f64(step, "y").unwrap_or(0.0))
            .delta_x(delta_x)
            .delta_y(delta_y)
            .build()
            .map_err(|e| anyhow!("mouse_wheel build failed: {}", e))?,
    )
    .await
    .map_err(|e| anyhow!("mouse_wheel failed: {}", e))?;
    Ok(json!({ "delta_x": delta_x, "delta_y": delta_y }))
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
    use chromiumoxide::cdp::browser_protocol::page::AddScriptToEvaluateOnNewDocumentParams;

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

    page.execute(AddScriptToEvaluateOnNewDocumentParams::new(script.to_string()))
        .await
        .map_err(|e| anyhow!("console_tap: addScript failed: {}", e))?;
        page.evaluate(script)
                .await
                .map_err(|e| anyhow!("console_tap: evaluate failed: {}", e))?;
        Ok(json!({ "console_tap": "installed" }))
}

async fn run_console_dump(page: &chromiumoxide::Page, step: &Value) -> Result<Value> {
    let level = step.get("level").and_then(|v| v.as_str()).unwrap_or("info");
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
    let mut value = remote.into_value::<Value>().unwrap_or(Value::Null);
    let min_rank = match level {
        "error" => 0,
        "warning" | "warn" => 1,
        "debug" => 3,
        _ => 2,
    };
    if let Some(events) = value.get_mut("events").and_then(|v| v.as_array_mut()) {
        events.retain(|event| {
            let rank = match event.get("level").and_then(|v| v.as_str()).unwrap_or("info") {
                "error" => 0,
                "warn" | "warning" => 1,
                "info" | "log" => 2,
                _ => 3,
            };
            rank <= min_rank
        });
    }
    if let Some(path) = step.get("filename").and_then(|v| v.as_str()) {
        let _ = maybe_write_json_file(path, &value).await?;
    }
    Ok(value)
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

async fn run_storage_state_export(page: &chromiumoxide::Page, step: &Value) -> Result<Value> {
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
    let value = remote.into_value::<Value>().unwrap_or(Value::Null);
    if let Some(path) = step
        .get("target")
        .or_else(|| step.get("filename"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        let _ = maybe_write_json_file(path, &value).await?;
    }
    Ok(value)
}

async fn run_storage_state_import(page: &chromiumoxide::Page, step: &Value) -> Result<Value> {
    let owned_raw;
    let raw_state = if let Some(raw) = step
        .get("value")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        raw
    } else if let Some(path) = step
        .get("target")
        .or_else(|| step.get("filename"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        owned_raw = std::fs::read_to_string(path)
            .map_err(|e| anyhow!("storage_state_import: read '{}' failed: {}", path, e))?;
        owned_raw.as_str()
    } else {
        ""
    };
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
    localStorage.clear();
    sessionStorage.clear();
    document.cookie.split(';').forEach(function(c) {{
        var name = c.split('=')[0].trim();
        if (name) {{
            document.cookie = name + '=; expires=Thu, 01 Jan 1970 00:00:00 GMT; path=/';
        }}
    }});
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

fn current_routes_json() -> Result<String> {
    let routes = MOCK_ROUTES
        .lock()
        .map_err(|_| anyhow!("mock route state lock poisoned"))?;
    serde_json::to_string(&*routes).map_err(|e| anyhow!("serialize mock routes failed: {}", e))
}

fn mock_routes_sync_script(routes_json: &str) -> String {
    format!(
        r#"(function() {{
  window.__cortexRouteDefs = {routes};
  window.__cortexRouteUsage = window.__cortexRouteUsage || {{}};
  function normalizeHeaders(value) {{
    if (!value || typeof value !== 'object') return {{}};
    return value;
  }}
    function buildResponseHeaders(route) {{
        var headers = Object.assign({{}}, normalizeHeaders(route.response_headers));
        var hasContentType = false;
        Object.keys(headers).forEach(function(key) {{
            if (String(key).toLowerCase() === 'content-type') hasContentType = true;
        }});
        if (!hasContentType) headers['Content-Type'] = 'application/json';
        var blocked = {{}};
        (route.remove_headers || []).forEach(function(name) {{
            blocked[String(name || '').toLowerCase()] = true;
        }});
        Object.keys(headers).forEach(function(key) {{
            if (blocked[String(key).toLowerCase()]) delete headers[key];
        }});
        return headers;
    }}
  function globToRegex(pattern) {{
    return '^' + String(pattern || '')
      .replace(/[.+^${{}}()|[\]\\]/g, '\\$&')
      .replace(/\*/g, '.*')
      .replace(/\?/g, '.') + '$';
  }}
  function matchRoute(url, method) {{
    var routes = window.__cortexRouteDefs || [];
    var upperMethod = String(method || 'GET').toUpperCase();
    for (var i = 0; i < routes.length; i++) {{
      var route = routes[i];
      var key = (route.method || '') + '|' + route.pattern;
      if (route.once && window.__cortexRouteUsage[key]) continue;
      if (!(new RegExp(globToRegex(route.pattern))).test(url)) continue;
      if (route.method && String(route.method).toUpperCase() !== upperMethod) continue;
      return {{ route: route, key: key }};
    }}
    return null;
  }}
  function markUsed(key) {{
    window.__cortexRouteUsage[key] = (window.__cortexRouteUsage[key] || 0) + 1;
  }}
    function recordNetworkEvent(kind, method, url, status, ok, error) {{
        window.__cortexNetworkEvents = window.__cortexNetworkEvents || [];
        window.__cortexNetworkEvents.push({{
            ts: new Date().toISOString(),
            transport: kind,
            method: String(method || 'GET').toUpperCase(),
            url: String(url || ''),
            status: status,
            ok: !!ok,
            error: error || null
        }});
    }}
  if (window.__cortexRouteManagerInstalled) return {{ ok:true, routes:(window.__cortexRouteDefs || []).length }};
  window.__cortexRouteManagerInstalled = true;
  var origFetch = window.fetch ? window.fetch.bind(window) : null;
  if (origFetch) {{
    window.fetch = function(resource, init) {{
      var url = typeof resource === 'string' ? resource : (resource && resource.url ? resource.url : String(resource));
      var method = (init && init.method) || (resource && resource.method) || 'GET';
      var matched = matchRoute(url, method);
      if (matched) {{
        markUsed(matched.key);
        return new Promise(function(resolve) {{
          setTimeout(function() {{
                        var headers = buildResponseHeaders(matched.route);
                        recordNetworkEvent('fetch', method, url, matched.route.status_code, matched.route.status_code < 400, null);
            resolve(new Response(matched.route.response_body, {{
              status: matched.route.status_code,
                            headers: headers
            }}));
          }}, matched.route.delay_ms || 0);
        }});
      }}
      return origFetch(resource, init);
    }};
  }}
  var OrigXHR = XMLHttpRequest;
  XMLHttpRequest = function() {{
    var xhr = new OrigXHR();
    var method = 'GET';
    var url = '';
    var matched = null;
    var open = xhr.open;
    xhr.open = function(m, u) {{
      method = m || 'GET';
      url = u || '';
      matched = matchRoute(url, method);
      if (!matched) return open.apply(xhr, arguments);
    }};
    var send = xhr.send;
    xhr.send = function() {{
      if (matched) {{
        markUsed(matched.key);
        setTimeout(function() {{
                    recordNetworkEvent('xhr', method, url, matched.route.status_code, matched.route.status_code < 400, null);
          xhr.readyState = 4;
          xhr.status = matched.route.status_code;
          xhr.statusText = 'OK';
          xhr.responseText = matched.route.response_body;
          xhr.response = matched.route.response_body;
          if (xhr.onreadystatechange) xhr.onreadystatechange();
          if (xhr.onload) xhr.onload({{ target: xhr }});
        }}, matched.route.delay_ms || 0);
        return;
      }}
      return send.apply(xhr, arguments);
    }};
    xhr.getResponseHeader = function(name) {{
      if (!matched) return OrigXHR.prototype.getResponseHeader.call(xhr, name);
            var headers = buildResponseHeaders(matched.route);
            var wanted = String(name || '').toLowerCase();
            var found = null;
            Object.keys(headers).some(function(key) {{
                if (String(key).toLowerCase() === wanted) {{
                    found = headers[key];
                    return true;
                }}
                return false;
            }});
            return found == null ? null : found;
    }};
        xhr.getAllResponseHeaders = function() {{
            if (!matched) return OrigXHR.prototype.getAllResponseHeaders.call(xhr);
            var headers = buildResponseHeaders(matched.route);
            return Object.keys(headers).map(function(key) {{ return key + ': ' + headers[key]; }}).join('\r\n');
        }};
    xhr.setRequestHeader = function() {{ if (!matched) return OrigXHR.prototype.setRequestHeader.apply(xhr, arguments); }};
    return xhr;
  }};
  return {{ ok:true, routes:(window.__cortexRouteDefs || []).length }};
}})()"#,
        routes = routes_json
    )
}

async fn sync_mock_routes(page: &chromiumoxide::Page) -> Result<Value> {
    use chromiumoxide::cdp::browser_protocol::page::AddScriptToEvaluateOnNewDocumentParams;

    let routes_json = current_routes_json()?;
    let script = mock_routes_sync_script(&routes_json);
    page.execute(AddScriptToEvaluateOnNewDocumentParams::new(script.clone()))
        .await
        .map_err(|e| anyhow!("mock_api addScript failed: {}", e))?;
    let remote = page
        .evaluate(script)
        .await
        .map_err(|e| anyhow!("mock_api evaluate failed: {}", e))?;
    Ok(remote.into_value::<Value>().unwrap_or(Value::Null))
}

async fn run_mock_api(page: &chromiumoxide::Page, step: &Value) -> Result<Value> {
    let url_pattern = step
        .get("url_pattern")
        .or_else(|| step.get("pattern"))
        .or_else(|| step.get("target"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if url_pattern.is_empty() {
        return Err(anyhow!("mock_api: 'url_pattern' or 'pattern' is required"));
    }

    let response_body = step
        .get("response_json")
        .or_else(|| step.get("body"))
        .and_then(|v| v.as_str())
        .unwrap_or("{}");
    let status_code = step
        .get("status_code")
        .or_else(|| step.get("status"))
        .and_then(|v| v.as_u64())
        .unwrap_or(200) as u16;
    let method = step
        .get("method")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_ascii_uppercase())
        .filter(|s| !s.is_empty());
    let response_headers = step
        .get("response_headers")
        .cloned()
        .or_else(|| step.get("headers").cloned())
        .unwrap_or_else(|| json!({}));
    let remove_headers = match step.get("removeHeaders").or_else(|| step.get("remove_headers")) {
        Some(Value::String(value)) => value
            .split(',')
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect::<Vec<_>>(),
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| item.as_str().map(|s| s.to_string()))
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    };
    let delay_ms = step.get("delay_ms").and_then(|v| v.as_u64()).unwrap_or(0);
    let once = step.get("once").and_then(|v| v.as_bool()).unwrap_or(false);

    {
        let mut routes = MOCK_ROUTES
            .lock()
            .map_err(|_| anyhow!("mock_api: route state lock poisoned"))?;
        routes.push(MockRouteDefinition {
            pattern: url_pattern.to_string(),
            method: method.clone(),
            response_body: response_body.to_string(),
            status_code,
            response_headers: response_headers.clone(),
            delay_ms,
            once,
            remove_headers: remove_headers.clone(),
        });
    }

    let _ = sync_mock_routes(page).await?;
    Ok(json!({
        "mocked": url_pattern,
        "status_code": status_code,
        "method": method,
        "delay_ms": delay_ms,
        "once": once,
        "remove_headers": remove_headers,
        "scope": "current page + future same-tab navigations"
    }))
}

async fn run_route_list() -> Result<Value> {
    let routes = MOCK_ROUTES
        .lock()
        .map_err(|_| anyhow!("route_list: route state lock poisoned"))?;
    Ok(json!({ "routes": &*routes }))
}

async fn run_unroute(page: &chromiumoxide::Page, step: &Value) -> Result<Value> {
    let pattern = step
        .get("pattern")
        .or_else(|| step.get("target"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let removed = {
        let mut routes = MOCK_ROUTES
            .lock()
            .map_err(|_| anyhow!("unroute: route state lock poisoned"))?;
        let before = routes.len();
        if pattern.is_empty() {
            routes.clear();
        } else {
            routes.retain(|route| route.pattern != pattern);
        }
        before.saturating_sub(routes.len())
    };
    let _ = sync_mock_routes(page).await?;
    Ok(json!({ "removed": removed, "pattern": if pattern.is_empty() { Value::Null } else { Value::String(pattern.to_string()) } }))
}

#[allow(deprecated)]
async fn run_network_state_set(page: &chromiumoxide::Page, step: &Value) -> Result<Value> {
    use chromiumoxide::cdp::browser_protocol::network::EmulateNetworkConditionsParams;

    let state = step
        .get("state")
        .or_else(|| step.get("target"))
        .and_then(|v| v.as_str())
        .unwrap_or("online");
    let offline = state.eq_ignore_ascii_case("offline");
    page.execute(EmulateNetworkConditionsParams::new(offline, 0.0, -1.0, -1.0))
        .await
        .map_err(|e| anyhow!("network_state_set failed: {}", e))?;
    Ok(json!({ "state": if offline { "offline" } else { "online" } }))
}

async fn run_cookie_list(page: &chromiumoxide::Page, step: &Value) -> Result<Value> {
    let domain_filter = step.get("domain").and_then(|v| v.as_str());
    let path_filter = step.get("path").and_then(|v| v.as_str());
    let cookies = page
        .get_cookies()
        .await
        .map_err(|e| anyhow!("cookie_list failed: {}", e))?;
    let mut values = serde_json::to_value(cookies).unwrap_or_else(|_| json!([]));
    if let Some(items) = values.as_array_mut() {
        items.retain(|item| {
            let domain_ok = domain_filter
                .map(|domain| item.get("domain").and_then(|v| v.as_str()).map(|s| s.contains(domain)).unwrap_or(false))
                .unwrap_or(true);
            let path_ok = path_filter
                .map(|path| item.get("path").and_then(|v| v.as_str()).map(|s| s == path).unwrap_or(false))
                .unwrap_or(true);
            domain_ok && path_ok
        });
    }
    Ok(json!({ "cookies": values }))
}

async fn run_cookie_get(page: &chromiumoxide::Page, step: &Value) -> Result<Value> {
    let name = step
        .get("name")
        .or_else(|| step.get("target"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("cookie_get: 'name' is required"))?;
    let cookies = run_cookie_list(page, step).await?;
    let found = cookies
        .get("cookies")
        .and_then(|v| v.as_array())
        .and_then(|items| items.iter().find(|item| item.get("name").and_then(|v| v.as_str()) == Some(name)).cloned())
        .unwrap_or(Value::Null);
    Ok(json!({ "cookie": found }))
}

async fn run_cookie_clear(page: &chromiumoxide::Page) -> Result<Value> {
    run_storage_clear(page, "cookies").await
}

async fn run_cookie_delete(page: &chromiumoxide::Page, step: &Value) -> Result<Value> {
    use chromiumoxide::cdp::browser_protocol::network::DeleteCookiesParams;

    let name = step
        .get("name")
        .or_else(|| step.get("target"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("cookie_delete: 'name' is required"))?;
    let mut builder = DeleteCookiesParams::builder().name(name.to_string());
    if let Some(domain) = step.get("domain").and_then(|v| v.as_str()) {
        builder = builder.domain(domain.to_string());
    }
    if let Some(path) = step.get("path").and_then(|v| v.as_str()) {
        builder = builder.path(path.to_string());
    }
    let params = builder.build().map_err(|e| anyhow!("cookie_delete build failed: {}", e))?;
    page.delete_cookie(params)
        .await
        .map_err(|e| anyhow!("cookie_delete failed: {}", e))?;
    Ok(json!({ "deleted": name }))
}

async fn run_cookie_set(page: &chromiumoxide::Page, step: &Value) -> Result<Value> {
    use chromiumoxide::cdp::browser_protocol::network::{CookieParam, CookieSameSite, TimeSinceEpoch};

    let name = step.get("name").and_then(|v| v.as_str()).ok_or_else(|| anyhow!("cookie_set: 'name' is required"))?;
    let value = step.get("value").and_then(|v| v.as_str()).ok_or_else(|| anyhow!("cookie_set: 'value' is required"))?;
    let mut builder = CookieParam::builder().name(name.to_string()).value(value.to_string());
    if let Some(domain) = step.get("domain").and_then(|v| v.as_str()) {
        builder = builder.domain(domain.to_string());
    }
    if let Some(path) = step.get("path").and_then(|v| v.as_str()) {
        builder = builder.path(path.to_string());
    }
    if let Some(secure) = step_bool(step, "secure") {
        builder = builder.secure(secure);
    }
    if let Some(http_only) = step_bool(step, "httpOnly").or_else(|| step_bool(step, "http_only")) {
        builder = builder.http_only(http_only);
    }
    if let Some(same_site) = step.get("sameSite").or_else(|| step.get("same_site")).and_then(|v| v.as_str()) {
        let parsed = same_site.parse::<CookieSameSite>().map_err(|_| anyhow!("cookie_set: invalid sameSite '{}'", same_site))?;
        builder = builder.same_site(parsed);
    }
    if let Some(expires) = step_f64(step, "expires") {
        builder = builder.expires(TimeSinceEpoch::new(expires));
    }
    let cookie = builder.build().map_err(|e| anyhow!("cookie_set build failed: {}", e))?;
    page.set_cookie(cookie)
        .await
        .map_err(|e| anyhow!("cookie_set failed: {}", e))?;
    Ok(json!({ "set": name }))
}

async fn run_local_storage_list(page: &chromiumoxide::Page) -> Result<Value> {
    let remote = page
        .evaluate("(function(){ var out={}; for (var i=0;i<localStorage.length;i++){ var k=localStorage.key(i); out[k]=localStorage.getItem(k);} return out; })()")
        .await
        .map_err(|e| anyhow!("localstorage_list failed: {}", e))?;
    Ok(json!({ "items": remote.into_value::<Value>().unwrap_or_else(|_| json!({})) }))
}

async fn run_local_storage_get(page: &chromiumoxide::Page, step: &Value) -> Result<Value> {
    let key = step
        .get("key")
        .or_else(|| step.get("target"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("localstorage_get: 'key' is required"))?;
    let script = format!("localStorage.getItem({})", serde_json::to_string(key).unwrap_or_else(|_| "\"\"".to_string()));
    let remote = page.evaluate(script).await.map_err(|e| anyhow!("localstorage_get failed: {}", e))?;
    Ok(json!({ "key": key, "value": remote.into_value::<Value>().unwrap_or(Value::Null) }))
}

async fn run_local_storage_set(page: &chromiumoxide::Page, step: &Value) -> Result<Value> {
    let key = step
        .get("key")
        .or_else(|| step.get("target"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("localstorage_set: 'key' is required"))?;
    let value = step.get("value").and_then(|v| v.as_str()).ok_or_else(|| anyhow!("localstorage_set: 'value' is required"))?;
    let script = format!("localStorage.setItem({}, {});", serde_json::to_string(key).unwrap_or_default(), serde_json::to_string(value).unwrap_or_default());
    page.evaluate(script).await.map_err(|e| anyhow!("localstorage_set failed: {}", e))?;
    Ok(json!({ "set": key }))
}

async fn run_local_storage_delete(page: &chromiumoxide::Page, step: &Value) -> Result<Value> {
    let key = step
        .get("key")
        .or_else(|| step.get("target"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("localstorage_delete: 'key' is required"))?;
    let script = format!("localStorage.removeItem({});", serde_json::to_string(key).unwrap_or_default());
    page.evaluate(script).await.map_err(|e| anyhow!("localstorage_delete failed: {}", e))?;
    Ok(json!({ "deleted": key }))
}

async fn run_local_storage_clear(page: &chromiumoxide::Page) -> Result<Value> {
    run_storage_clear(page, "local").await
}

async fn run_session_storage_list(page: &chromiumoxide::Page) -> Result<Value> {
    let remote = page
        .evaluate("(function(){ var out={}; for (var i=0;i<sessionStorage.length;i++){ var k=sessionStorage.key(i); out[k]=sessionStorage.getItem(k);} return out; })()")
        .await
        .map_err(|e| anyhow!("sessionstorage_list failed: {}", e))?;
    Ok(json!({ "items": remote.into_value::<Value>().unwrap_or_else(|_| json!({})) }))
}

async fn run_session_storage_get(page: &chromiumoxide::Page, step: &Value) -> Result<Value> {
    let key = step
        .get("key")
        .or_else(|| step.get("target"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("sessionstorage_get: 'key' is required"))?;
    let script = format!("sessionStorage.getItem({})", serde_json::to_string(key).unwrap_or_default());
    let remote = page.evaluate(script).await.map_err(|e| anyhow!("sessionstorage_get failed: {}", e))?;
    Ok(json!({ "key": key, "value": remote.into_value::<Value>().unwrap_or(Value::Null) }))
}

async fn run_session_storage_set(page: &chromiumoxide::Page, step: &Value) -> Result<Value> {
    let key = step
        .get("key")
        .or_else(|| step.get("target"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("sessionstorage_set: 'key' is required"))?;
    let value = step.get("value").and_then(|v| v.as_str()).ok_or_else(|| anyhow!("sessionstorage_set: 'value' is required"))?;
    let script = format!("sessionStorage.setItem({}, {});", serde_json::to_string(key).unwrap_or_default(), serde_json::to_string(value).unwrap_or_default());
    page.evaluate(script).await.map_err(|e| anyhow!("sessionstorage_set failed: {}", e))?;
    Ok(json!({ "set": key }))
}

async fn run_session_storage_delete(page: &chromiumoxide::Page, step: &Value) -> Result<Value> {
    let key = step
        .get("key")
        .or_else(|| step.get("target"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("sessionstorage_delete: 'key' is required"))?;
    let script = format!("sessionStorage.removeItem({});", serde_json::to_string(key).unwrap_or_default());
    page.evaluate(script).await.map_err(|e| anyhow!("sessionstorage_delete failed: {}", e))?;
    Ok(json!({ "deleted": key }))
}

async fn run_session_storage_clear(page: &chromiumoxide::Page) -> Result<Value> {
    run_storage_clear(page, "session").await
}

async fn run_pdf_save(page: &chromiumoxide::Page, step: &Value) -> Result<Value> {
    use chromiumoxide::cdp::browser_protocol::page::PrintToPdfParams;

    let result = page
        .execute(PrintToPdfParams::builder().print_background(true).build())
        .await
        .map_err(|e| anyhow!("pdf_save failed: {}", e))?;
    let data: String = result.result.data.into();
    let path = step
        .get("filename")
        .or_else(|| step.get("target"))
        .and_then(|v| v.as_str())
        .unwrap_or("page-output.pdf");
    let written = maybe_write_base64_file(path, &data).await?;
    Ok(json!({ "path": written }))
}

async fn run_generate_locator(page: &chromiumoxide::Page, step: &Value, timeout_ms: u64) -> Result<Value> {
    let selector = step
        .get("target")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("generate_locator: 'target' selector is required"))?;
    run_wait_for_selector(page, selector, timeout_ms).await?;
    let selector_js = serde_json::to_string(selector).unwrap_or_else(|_| "\"\"".to_string());
    let script = format!(
        r#"(function() {{
  var el = document.querySelector({selector});
  if (!el) return {{ ok:false }};
  var testid = el.getAttribute('data-testid');
  if (testid) return {{ ok:true, locator:'testid', target:testid }};
  var placeholder = el.getAttribute('placeholder');
  if (placeholder) return {{ ok:true, locator:'placeholder', target:placeholder }};
  var aria = el.getAttribute('aria-label');
  if (aria) return {{ ok:true, locator:'text', target:aria }};
  if (el.id) return {{ ok:true, locator:'css', target:'#' + CSS.escape(el.id) }};
  return {{ ok:true, locator:'css', target:{selector} }};
}})()"#,
        selector = selector_js
    );
    let remote = page.evaluate(script).await.map_err(|e| anyhow!("generate_locator failed: {}", e))?;
    Ok(remote.into_value::<Value>().unwrap_or(Value::Null))
}

async fn run_verify_element_visible(page: &chromiumoxide::Page, step: &Value, timeout_ms: u64) -> Result<Value> {
    let role = step.get("role").and_then(|v| v.as_str()).ok_or_else(|| anyhow!("verify_element_visible: 'role' is required"))?;
    let accessible_name = step
        .get("accessibleName")
        .or_else(|| step.get("accessible_name"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let locator_step = json!({
        "target": role,
        "locator": "role",
        "name": accessible_name,
        "condition": "is_visible"
    });
    run_assert_locator(page, &locator_step, "is_visible", timeout_ms).await
}

async fn run_verify_text_visible(page: &chromiumoxide::Page, step: &Value, timeout_ms: u64) -> Result<Value> {
    let text = step
        .get("text")
        .or_else(|| step.get("target"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("verify_text_visible: 'text' is required"))?;
    let locator_step = json!({
        "target": text,
        "locator": "text",
        "condition": "is_visible"
    });
    run_assert_locator(page, &locator_step, "is_visible", timeout_ms).await
}

async fn run_verify_list_visible(page: &chromiumoxide::Page, step: &Value, timeout_ms: u64) -> Result<Value> {
    let selector = step
        .get("target")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("verify_list_visible: 'target' selector is required"))?;
    let items = step
        .get("items")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("verify_list_visible: 'items' array is required"))?;
    run_wait_for_selector(page, selector, timeout_ms).await?;
    let selector_js = serde_json::to_string(selector).unwrap_or_else(|_| "\"\"".to_string());
    let items_js = serde_json::to_string(items).unwrap_or_else(|_| "[]".to_string());
    let script = format!(
        r#"(function() {{
  var el = document.querySelector({selector});
  if (!el) return {{ ok:false, reason:'list not found' }};
  var text = (el.innerText || el.textContent || '');
  var items = {items};
  var missing = items.filter(function(item) {{ return text.indexOf(String(item)) === -1; }});
  return {{ ok: missing.length === 0, missing: missing }};
}})()"#,
        selector = selector_js,
        items = items_js
    );
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        let remote = page.evaluate(script.clone()).await.map_err(|e| anyhow!("verify_list_visible failed: {}", e))?;
        let result = remote.into_value::<Value>().unwrap_or(Value::Null);
        if result.get("ok").and_then(|v| v.as_bool()) == Some(true) {
            return Ok(json!({ "verified": true, "selector": selector }));
        }
        if Instant::now() >= deadline {
            return Err(anyhow!("verify_list_visible timed out: {}", result));
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

async fn run_verify_value(page: &chromiumoxide::Page, step: &Value, timeout_ms: u64) -> Result<Value> {
    let selector = step
        .get("target")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("verify_value: 'target' selector is required"))?;
    let expected = step.get("value").and_then(|v| v.as_str()).ok_or_else(|| anyhow!("verify_value: 'value' is required"))?;
    let element_type = step.get("type").and_then(|v| v.as_str()).unwrap_or("textbox");
    run_wait_for_selector(page, selector, timeout_ms).await?;
    let selector_js = serde_json::to_string(selector).unwrap_or_else(|_| "\"\"".to_string());
    let expected_js = serde_json::to_string(expected).unwrap_or_else(|_| "\"\"".to_string());
    let type_js = serde_json::to_string(element_type).unwrap_or_else(|_| "\"textbox\"".to_string());
    let script = format!(
        r#"(function() {{
  var el = document.querySelector({selector});
  if (!el) return {{ ok:false, reason:'element not found' }};
  var expected = {expected};
  var kind = {kind};
  var actual = (kind === 'checkbox') ? String(!!el.checked) : String('value' in el ? el.value : (el.textContent || ''));
  return {{ ok: actual === String(expected), actual: actual }};
}})()"#,
        selector = selector_js,
        expected = expected_js,
        kind = type_js
    );
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        let remote = page.evaluate(script.clone()).await.map_err(|e| anyhow!("verify_value failed: {}", e))?;
        let result = remote.into_value::<Value>().unwrap_or(Value::Null);
        if result.get("ok").and_then(|v| v.as_bool()) == Some(true) {
            return Ok(json!({ "verified": true, "selector": selector, "value": expected }));
        }
        if Instant::now() >= deadline {
            return Err(anyhow!("verify_value timed out: {}", result));
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
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

    let mut page = match guard.as_ref() {
        Some(s) => s.page.clone(),
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

    {
        let browser = match guard.as_ref() {
            Some(s) => &s.browser,
            None => unreachable!("session checked above"),
        };

        for (idx, step) in steps.iter().enumerate() {
            let step_result = execute_step(browser, &mut page, step, idx).await;
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
    }

    if let Some(session) = guard.as_mut() {
        session.page = page.clone();
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

    // Visible browser: force headful mode so the human can complete auth.
    let config = match build_visible_auth_config(&exe, &profile_dir) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_auth_config_is_headful_and_disables_sandbox() {
        let config =
            build_visible_auth_config("/bin/echo", Path::new("/tmp/cortex-scout-test-profile"))
                .expect("config");
        let debug = format!("{:?}", config);

        assert!(debug.contains("headless: False"), "{debug}");
        assert!(debug.contains("sandbox: false"), "{debug}");
    }
}
