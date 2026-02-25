/// visual_scout — lightweight headless screenshot tool.
///
/// Launches a headless Chromium session, navigates to the target URL, captures
/// a full-viewport PNG screenshot, and returns it as a base64-encoded string so
/// a Vision-capable AI model can inspect the rendered page (login gates, modals,
/// layout) without needing to parse HTML.  This is intentionally lightweight:
/// *no* text extraction pipeline, *no* JS interaction — just a camera shot.
///
/// # Why a separate tool?
/// `web_fetch` (scrape_url) is optimised for text extraction.  Having a dedicated
/// screenshot tool avoids burning text-extraction budget on pages whose auth state
/// we only need to visually confirm.
use crate::scraping::browser_manager;
use anyhow::{anyhow, Result};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{error, info, warn};

/// Response returned by `take_screenshot`.
#[derive(Debug, Serialize, Deserialize)]
pub struct VisualScoutResult {
    /// The URL that was actually navigated to (may differ from input after redirects).
    pub url: String,
    /// Page `<title>` text extracted from the DOM after load.
    pub page_title: String,
    /// Absolute path to the saved PNG file on disk.
    /// Pass this path to your Vision model or open it in an image viewer.
    /// Located under `{TMPDIR}/.cortex-scout-screenshots/`.
    pub screenshot_path: String,
    /// Screenshot size in bytes (before base64 encoding).
    pub screenshot_bytes: usize,
    /// Viewport width used during capture.
    pub viewport_width: u32,
    /// Viewport height used during capture.
    pub viewport_height: u32,
    /// ISO-8601 timestamp of capture.
    pub captured_at: String,
    /// Human-readable hint for the agent.
    pub hint: String,
}

/// Capture a screenshot of `url` using a headless Chromium instance.
///
/// `proxy_url` — optional `http(s)://host:port` or `socks5://host:port` proxy.
/// `width` / `height` — viewport dimensions (defaults: 1280 × 800).
pub async fn take_screenshot(
    url: &str,
    proxy_url: Option<&str>,
    width: Option<u32>,
    height: Option<u32>,
) -> Result<VisualScoutResult> {
    let exe = browser_manager::find_chrome_executable().ok_or_else(|| {
        anyhow!(
            "No browser found for visual_scout. \
             Install Brave, Chrome, or Chromium and make it available in PATH."
        )
    })?;

    let vp_width = width.unwrap_or(1280);
    let vp_height = height.unwrap_or(800);

    info!(
        "visual_scout: launching headless {} @ {}×{} → {}",
        exe, vp_width, vp_height, url
    );

    let config = browser_manager::build_headless_config(&exe, proxy_url, vp_width, vp_height)?;

    let (mut browser, mut handler) = chromiumoxide::Browser::launch(config)
        .await
        .map_err(|e| anyhow!("visual_scout: browser launch failed ({}): {}", exe, e))?;

    let handle = tokio::spawn(async move {
        while let Some(event) = handler.next().await {
            if let Err(e) = event {
                error!("visual_scout CDP handler error: {}", e);
            }
        }
    });

    // Open page, navigate, wait for load.
    let page = browser
        .new_page("about:blank")
        .await
        .map_err(|e| anyhow!("visual_scout: new_page failed: {}", e))?;

    // Auto-inject stored session cookies before navigation so auth-walled pages
    // are captured in an authenticated state when a prior HITL session exists.
    super::session_store::auto_inject(&page, url).await;

    page.goto(url)
        .await
        .map_err(|e| anyhow!("visual_scout: goto({url}) failed: {}", e))?;

    // Brief settle to let lazy-loaded elements appear (spinner removal, etc.)
    tokio::time::sleep(Duration::from_millis(1200)).await;

    // Grab the page title for context.
    let page_title = page
        .evaluate("document.title")
        .await
        .ok()
        .and_then(|h| h.into_value::<String>().ok())
        .unwrap_or_default();

    // Grab the final URL (after any client-side redirects).
    let final_url = page
        .evaluate("location.href")
        .await
        .ok()
        .and_then(|h| h.into_value::<String>().ok())
        .unwrap_or_else(|| url.to_string());

    // Capture PNG screenshot using the high-level page.screenshot() API.
    use chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotFormat;
    use chromiumoxide::page::ScreenshotParams;

    let screenshot_bytes: Vec<u8> = page
        .screenshot(
            ScreenshotParams::builder()
                .format(CaptureScreenshotFormat::Png)
                .build(),
        )
        .await
        .map_err(|e| anyhow!("visual_scout: screenshot capture failed: {}", e))?;

    let byte_len = screenshot_bytes.len();

    // Save screenshot to a local temp file instead of returning a large base64
    // blob.  This keeps context windows lean — the agent reads the file only
    // when it needs to perform visual analysis.
    let cache_dir = std::env::temp_dir().join(".cortex-scout-screenshots");
    std::fs::create_dir_all(&cache_dir)
        .map_err(|e| anyhow!("visual_scout: failed to create screenshot cache dir: {}", e))?;
    let host_slug = url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.replace('.', "_")))
        .unwrap_or_else(|| "unknown".to_string());
    let ts = chrono::Utc::now().timestamp_millis();
    let filename = format!("scout_{}_{}.png", host_slug, ts);
    let screenshot_path_buf = cache_dir.join(&filename);
    std::fs::write(&screenshot_path_buf, &screenshot_bytes).map_err(|e| {
        anyhow!(
            "visual_scout: failed to write screenshot to {:?}: {}",
            screenshot_path_buf,
            e
        )
    })?;
    let screenshot_path = screenshot_path_buf.to_string_lossy().to_string();

    // Shut down browser cleanly.
    if let Err(e) = browser.close().await {
        warn!("visual_scout: browser close error (non-fatal): {}", e);
    }
    handle.abort();

    let auth_hint = {
        let tl = page_title.trim().to_lowercase();
        if tl.starts_with("sign in")
            || tl.starts_with("log in")
            || tl.starts_with("login")
            || final_url.to_lowercase().contains("/login")
            || final_url.to_lowercase().contains("/signin")
        {
            "🔒 AUTH_WALL likely — page title or URL indicates a login page. \
             Escalate to human_auth_session."
                .to_string()
        } else {
            "✅ No obvious auth wall in title/URL. \
             Inspect the screenshot for modal overlays or partial content blocks."
                .to_string()
        }
    };

    info!(
        "visual_scout: captured {} bytes → {} for \u{00ab}{}\u{00bb}",
        byte_len, screenshot_path, page_title
    );

    Ok(VisualScoutResult {
        url: final_url,
        page_title,
        screenshot_path,
        screenshot_bytes: byte_len,
        viewport_width: vp_width,
        viewport_height: vp_height,
        captured_at: chrono::Utc::now().to_rfc3339(),
        hint: auth_hint,
    })
}
