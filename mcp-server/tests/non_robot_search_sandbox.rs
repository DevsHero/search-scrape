// Sandbox HITL flow test (safe): serves local pages that simulate anti-bot challenges.
//
// This does NOT target real third-party sites or attempt to bypass their protections.
// It exists to validate ShadowCrawl's state machine behavior:
// - Detect challenge
// - Enter HITL (overlay)
// - Wait for "solved" condition
// - Resume extraction
//
// Run (manual, opens visible browser):
//   SHADOWCRAWL_NON_ROBOT_AUTO_ALLOW=1 \
//   cargo test --features non_robot_search --test non_robot_search_sandbox -- --ignored --nocapture

#![cfg(feature = "non_robot_search")]

use axum::{routing::get, Router};
use shadowcrawl::features::non_robot_search::{execute_non_robot_search, NonRobotSearchConfig};
use shadowcrawl::rust_scraper::QualityMode;
use shadowcrawl::AppState;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

fn global_hitl_test_lock() -> &'static tokio::sync::Mutex<()> {
    use std::sync::OnceLock;
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

async fn page_ok() -> &'static str {
    r#"<!doctype html>
<html><head><title>OK</title></head>
<body><h1>OK</h1><p>Hello from sandbox.</p></body></html>"#
}

async fn page_challenge_auto_resolve() -> &'static str {
    // This page intentionally includes challenge markers that our detector looks for.
    // Then it removes the iframe after a short delay to simulate a solved CAPTCHA.
    r#"<!doctype html>
<html>
  <head>
    <title>Challenge Sandbox</title>
    <meta charset="utf-8" />
    <script>
      setTimeout(() => {
        const f = document.querySelector('iframe');
        if (f) f.remove();
        document.body.insertAdjacentHTML('beforeend', '<p id="solved">Solved</p>');
      }, 4000);
    </script>
  </head>
  <body>
    <h1>Checking if the site connection is secure</h1>
    <iframe src="https://challenges.cloudflare.com/sandbox" title="captcha"></iframe>
    <p>Simulated challenge…</p>
  </body>
</html>"#
}

fn init_logger() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_test_writer()
        .try_init();
}

#[tokio::test]
#[ignore]
async fn hitl_flow_sandbox_auto_resolve() {
    init_logger();

    // These tests mutate process-wide env vars and use a shared CDP port (9222).
    // Serialize them to avoid races/hangs when the test runner executes in parallel.
    let _guard = global_hitl_test_lock().lock().await;

    // Start local sandbox server on an ephemeral port.
    let app = Router::new()
        .route("/ok", get(page_ok))
        .route("/challenge", get(page_challenge_auto_resolve));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind failed");
    let addr: SocketAddr = listener.local_addr().expect("local_addr failed");

    tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });

    // Minimal AppState for scrape processing.
    let state = Arc::new(AppState::new(reqwest::Client::new()));

    // Ensure consent is non-blocking for this test.
    std::env::set_var("SHADOWCRAWL_NON_ROBOT_AUTO_ALLOW", "1");
    // Also force dialog mode off in case someone runs in a TTY.
    std::env::set_var("SHADOWCRAWL_NON_ROBOT_CONSENT", "auto");

    let url = format!("http://{}/challenge", addr);
    println!("Sandbox URL: {}", url);

    let cfg = NonRobotSearchConfig {
        url,
        max_chars: 10_000,
        use_proxy: false,
        quality_mode: QualityMode::Balanced,
        captcha_grace: Duration::from_secs(1),
        human_timeout: Duration::from_secs(15),
        user_profile_path: None,
        auto_scroll: false,
        wait_for_selector: None,
    };

    let result = execute_non_robot_search(&state, cfg)
        .await
        .expect("non_robot_search sandbox run failed");

    // We expect the page to have auto-appended "Solved" before extraction.
    assert!(
        result.content.contains("Solved") || result.clean_content.contains("Solved"),
        "expected sandbox to auto-resolve challenge"
    );
}

#[tokio::test]
#[ignore]
async fn hitl_flow_sandbox_auto_resolve_with_blocking_os_dialog() {
    init_logger();

    // These tests mutate process-wide env vars and use a shared CDP port (9222).
    // Serialize them to avoid races/hangs when the test runner executes in parallel.
    let _guard = global_hitl_test_lock().lock().await;

    // Start local sandbox server on an ephemeral port.
    let app = Router::new()
        .route("/ok", get(page_ok))
        .route("/challenge", get(page_challenge_auto_resolve));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind failed");
    let addr: SocketAddr = listener.local_addr().expect("local_addr failed");

    tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });

    // Minimal AppState for scrape processing.
    let state = Arc::new(AppState::new(reqwest::Client::new()));

    // Force a blocking OS-level consent dialog even if AUTO_ALLOW is set.
    std::env::set_var("SHADOWCRAWL_NON_ROBOT_AUTO_ALLOW", "1");
    std::env::set_var("SHADOWCRAWL_NON_ROBOT_CONSENT", "dialog");

    let url = format!("http://{}/challenge", addr);
    println!("Sandbox URL (dialog mode): {}", url);
    println!("You should see an OS consent popup now. Click OK to continue.");

    let cfg = NonRobotSearchConfig {
        url,
        max_chars: 10_000,
        use_proxy: false,
        quality_mode: QualityMode::Balanced,
        captcha_grace: Duration::from_secs(1),
        human_timeout: Duration::from_secs(20),
        user_profile_path: None,
        auto_scroll: false,
        wait_for_selector: None,
    };

    let result = execute_non_robot_search(&state, cfg)
        .await
        .expect("non_robot_search sandbox run failed");

    assert!(
        result.content.contains("Solved") || result.clean_content.contains("Solved"),
        "expected sandbox to auto-resolve challenge"
    );
}
