// Manual smoke test for the interactive non_robot_search feature.
//
// This test is ignored by default because it:
// - opens a visible browser
// - prompts for permission
// - may require macOS Accessibility permissions for the kill switch
//
// Run:
//   cargo test --features non_robot_search --test non_robot_search_manual -- --ignored --nocapture
//
// Optional env vars:
//   NON_ROBOT_SEARCH_URL=https://example.com

#![cfg(feature = "non_robot_search")]

use shadowcrawl::features::non_robot_search::{execute_non_robot_search, NonRobotSearchConfig};
use shadowcrawl::rust_scraper::QualityMode;
use shadowcrawl::AppState;
use std::sync::Arc;
use std::time::Duration;

fn init_logger() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_test_writer()
        .try_init();
}

#[tokio::test]
#[ignore]
async fn manual_non_robot_search_smoke() {
    init_logger();

    let url =
        std::env::var("NON_ROBOT_SEARCH_URL").unwrap_or_else(|_| "https://example.com".to_string());

    println!("\n🧪 MANUAL TEST: non_robot_search");
    println!("URL: {}", url);
    println!("\nInstructions:");
    println!("- You will be prompted in the terminal to allow/cancel");
    println!("- If a CAPTCHA appears, solve it in the opened browser");
    println!("- Emergency abort: hold ESC for ~3 seconds");

    let http_client = reqwest::Client::new();
    let state = Arc::new(AppState::new(http_client));

    let cfg = NonRobotSearchConfig {
        url,
        max_chars: 10_000,
        use_proxy: false,
        quality_mode: QualityMode::Balanced,
        captcha_grace: Duration::from_secs(5),
        human_timeout: Duration::from_secs(60),
        user_profile_path: None,
        auto_scroll: false,
        wait_for_selector: None,
    };

    let result = execute_non_robot_search(&state, cfg).await;
    match result {
        Ok(scrape) => {
            println!("✅ Extracted title: {}", scrape.title);
            println!("✅ Word count: {}", scrape.word_count);
            println!(
                "\nPreview:\n{}",
                scrape.clean_content.chars().take(800).collect::<String>()
            );
            assert!(scrape.word_count > 1, "Expected some extracted content");
        }
        Err(e) => {
            panic!("non_robot_search failed: {}", e);
        }
    }
}
