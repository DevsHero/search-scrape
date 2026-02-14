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
    let state = Arc::new(AppState::new(
        "http://localhost:8890".to_string(),
        reqwest::Client::new(),
    ));

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
