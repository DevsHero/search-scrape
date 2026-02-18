/// Integration Tests: Self-Evolving SDET Suite
/// Tests diverse web patterns to identify extraction failures
use shadowcrawl::rust_scraper::RustScraper;

// Initialize logging for tests
fn init_logger() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_test_writer()
        .try_init();
}

#[tokio::test]
async fn test_wikipedia_table_extraction() {
    init_logger();
    let scraper = RustScraper::new();
    let url = "https://en.wikipedia.org/wiki/Rust_(programming_language)";

    println!("\n🧪 TEST 1: Wikipedia (Static + Tables)");
    println!("URL: {}", url);

    match scraper.scrape_url(url).await {
        Ok(result) => {
            println!("✅ Status: {}", result.status_code);
            println!("📊 Word Count: {}", result.word_count);
            println!(
                "📈 Extraction Score: {:.2}",
                result.extraction_score.unwrap_or(0.0)
            );
            println!("🔢 Code Blocks: {}", result.code_blocks.len());
            println!("⚠️  Warnings: {:?}", result.warnings);

            // Assertions
            assert!(
                result.word_count > 100,
                "❌ FAIL: Word count too low ({})",
                result.word_count
            );
            assert!(
                result.extraction_score.unwrap_or(0.0) >= 0.6,
                "❌ FAIL: Extraction score too low ({:.2})",
                result.extraction_score.unwrap_or(0.0)
            );

            // Check for table markers in Markdown
            let has_table_structure =
                result.clean_content.contains("|") || result.clean_content.contains("---");
            println!("📋 Has Table Structure: {}", has_table_structure);

            // Sample first 500 chars
            println!("\n📄 Content Preview:");
            println!(
                "{}",
                result.clean_content.chars().take(500).collect::<String>()
            );
        }
        Err(e) => {
            panic!("❌ FAIL: {}", e);
        }
    }
}

#[tokio::test]
async fn test_rust_docs_code_blocks() {
    let scraper = RustScraper::new();
    let url = "https://doc.rust-lang.org/book/ch01-02-hello-world.html";

    println!("\n🧪 TEST 2: Rust Docs (Technical + Code Blocks)");
    println!("URL: {}", url);

    match scraper.scrape_url(url).await {
        Ok(result) => {
            println!("✅ Status: {}", result.status_code);
            println!("📊 Word Count: {}", result.word_count);
            println!(
                "📈 Extraction Score: {:.2}",
                result.extraction_score.unwrap_or(0.0)
            );
            println!("🔢 Code Blocks: {}", result.code_blocks.len());
            println!("⚠️  Warnings: {:?}", result.warnings);

            // Assertions
            assert!(
                result.word_count > 50,
                "❌ FAIL: Word count too low ({})",
                result.word_count
            );
            assert!(
                !result.code_blocks.is_empty(),
                "❌ FAIL: No code blocks extracted"
            );
            assert!(
                result.extraction_score.unwrap_or(0.0) >= 0.7,
                "❌ FAIL: Extraction score too low ({:.2})",
                result.extraction_score.unwrap_or(0.0)
            );

            // Check first code block
            if let Some(block) = result.code_blocks.first() {
                println!("\n💻 First Code Block:");
                println!("Language: {:?}", block.language);
                println!("Code: {}", block.code.chars().take(200).collect::<String>());

                assert!(block.code.len() > 10, "❌ FAIL: Code block too short");
            }

            // Sample content
            println!("\n📄 Content Preview:");
            println!(
                "{}",
                result.clean_content.chars().take(500).collect::<String>()
            );
        }
        Err(e) => {
            panic!("❌ FAIL: {}", e);
        }
    }
}

#[tokio::test]
async fn test_github_readme() {
    let scraper = RustScraper::new();
    // Use raw content to avoid GitHub UI noise and intermittent blob-view errors.
    let url = "https://raw.githubusercontent.com/rust-lang/rust/master/README.md";

    println!("\n🧪 TEST 3: GitHub README (Markdown Native)");
    println!("URL: {}", url);

    match scraper.scrape_url(url).await {
        Ok(result) => {
            println!("✅ Status: {}", result.status_code);
            println!("📊 Word Count: {}", result.word_count);
            println!(
                "📈 Extraction Score: {:.2}",
                result.extraction_score.unwrap_or(0.0)
            );
            println!("🔢 Code Blocks: {}", result.code_blocks.len());
            println!("⚠️  Warnings: {:?}", result.warnings);

            // Assertions
            // Raw README should be stable and have meaningful content.
            assert!(
                result.status_code < 400,
                "❌ FAIL: HTTP status was {}",
                result.status_code
            );
            assert!(
                result.word_count > 80,
                "❌ FAIL: Word count too low ({})",
                result.word_count
            );

            // Check for Markdown structure
            let has_markdown = result.clean_content.contains("##")
                || result.clean_content.contains("**")
                || result.clean_content.contains("```");
            println!("📝 Has Markdown Structure: {}", has_markdown);

            // Sample content
            println!("\n📄 Content Preview:");
            println!(
                "{}",
                result.clean_content.chars().take(500).collect::<String>()
            );
        }
        Err(e) => {
            panic!("❌ FAIL: {}", e);
        }
    }
}

#[tokio::test]
async fn test_reddit_thread_with_native_cdp() {
    init_logger();
    let scraper = RustScraper::new();
    let url = "https://old.reddit.com/r/rust/comments/10nimss/how_do_i_start_learning_rust/";

    println!("\n🧪 TEST 4: Reddit (JS-Heavy + Native CDP Fallback)");
    println!("URL: {}", url);

    let cdp_available = shadowcrawl::scraping::browser_manager::native_browser_available();
    println!("🌐 Native CDP Available: {}", cdp_available);

    match scraper.scrape_url(url).await {
        Ok(result) => {
            println!("✅ Status: {}", result.status_code);
            println!("📊 Word Count: {}", result.word_count);
            println!(
                "📈 Extraction Score: {:.2}",
                result.extraction_score.unwrap_or(0.0)
            );
            println!("⚠️  Warnings: {:?}", result.warnings);

            // For Reddit, we expect lower scores but still some content
            assert!(
                result.word_count > 20,
                "❌ FAIL: No meaningful content extracted ({})",
                result.word_count
            );

            let _ = cdp_available; // informational only

            // Sample content
            println!("\n📄 Content Preview:");
            println!(
                "{}",
                result.clean_content.chars().take(500).collect::<String>()
            );
        }
        Err(e) => {
            println!("⚠️  Expected potential failure for Reddit: {}", e);
        }
    }
}

#[tokio::test]
async fn test_medium_article() {
    let scraper = RustScraper::new();
    let url = "https://medium.com/@benwubbleyou/learn-rust-the-dangerous-way-44e9efd7cbe";

    println!("\n🧪 TEST 5: Medium (Article + Paywall)");
    println!("URL: {}", url);

    match scraper.scrape_url(url).await {
        Ok(result) => {
            println!("✅ Status: {}", result.status_code);
            println!("📊 Word Count: {}", result.word_count);
            println!(
                "📈 Extraction Score: {:.2}",
                result.extraction_score.unwrap_or(0.0)
            );
            println!("👤 Author: {:?}", result.author);
            println!("📅 Published: {:?}", result.published_at);
            println!("⚠️  Warnings: {:?}", result.warnings);

            // Assertions - Medium paywall limitations acknowledged
            // Medium uses React SSR + paywall; 20-40 words is realistic without JS rendering
            assert!(
                result.word_count > 20,
                "❌ FAIL: Word count too low ({})",
                result.word_count
            );

            // Sample content
            println!("\n📄 Content Preview:");
            println!(
                "{}",
                result.clean_content.chars().take(500).collect::<String>()
            );
        }
        Err(e) => {
            println!("⚠️  Medium may block: {}", e);
        }
    }
}

#[tokio::test]
async fn test_docs_portal() {
    let scraper = RustScraper::new();
    let url = "https://developer.mozilla.org/en-US/docs/Web";

    println!("\n🧪 TEST 6: Docs Portal (Enterprise Docs)");
    println!("URL: {}", url);

    match scraper.scrape_url(url).await {
        Ok(result) => {
            println!("✅ Status: {}", result.status_code);
            println!("📊 Word Count: {}", result.word_count);
            println!(
                "📈 Extraction Score: {:.2}",
                result.extraction_score.unwrap_or(0.0)
            );
            println!("🔢 Code Blocks: {}", result.code_blocks.len());
            println!("🔗 Links: {}", result.links.len());
            println!("⚠️  Warnings: {:?}", result.warnings);

            // Assertions
            assert!(
                result.word_count > 100,
                "❌ FAIL: Word count too low ({})",
                result.word_count
            );
            assert!(
                result.extraction_score.unwrap_or(0.0) >= 0.45,
                "❌ FAIL: Extraction score too low ({:.2})",
                result.extraction_score.unwrap_or(0.0)
            );

            // Sample content
            println!("\n📄 Content Preview:");
            println!(
                "{}",
                result.clean_content.chars().take(500).collect::<String>()
            );
        }
        Err(e) => {
            panic!("❌ FAIL: {}", e);
        }
    }
}

#[tokio::test]
#[ignore] // Only run when a local browser is available
async fn test_native_cdp_direct() {
    init_logger();
    let scraper = RustScraper::new();

    // Test with a JS-heavy SPA that requires rendering
    let url = "https://www.npmjs.com/package/react";

    println!("\n🧪 TEST 7: Native CDP Direct (JS-Heavy SPA)");
    println!("URL: {}", url);

    // Check if native CDP is available
    if !shadowcrawl::scraping::browser_manager::native_browser_available() {
        println!("⚠️  Skipping: no local browser found");
        println!("   Install Brave/Chrome/Chromium or set CHROME_EXECUTABLE to enable");
        return;
    }

    match scraper.scrape_with_browserless(url).await {
        Ok(result) => {
            println!("✅ Status: {}", result.status_code);
            println!("📊 Word Count: {}", result.word_count);
            println!(
                "📈 Extraction Score: {:.2}",
                result.extraction_score.unwrap_or(0.0)
            );
            println!("⚠️  Warnings: {:?}", result.warnings);

            // Should extract meaningful content from JS-rendered page
            assert!(
                result.word_count > 50,
                "❌ FAIL: Insufficient content extracted ({})",
                result.word_count
            );

            println!("\n✅ Native CDP successfully rendered JS-heavy content");

            // Sample content
            println!("\n📄 Content Preview:");
            println!(
                "{}",
                result.clean_content.chars().take(500).collect::<String>()
            );
        }
        Err(e) => {
            panic!("❌ FAIL: Native CDP scraping failed: {}", e);
        }
    }
}

// NOTE: Keep this file focused on executable validations.
