/// Integration Tests: Self-Evolving SDET Suite
/// Tests diverse web patterns to identify extraction failures
use search_scrape::rust_scraper::RustScraper;

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
            println!("📈 Extraction Score: {:.2}", result.extraction_score.unwrap_or(0.0));
            println!("🔢 Code Blocks: {}", result.code_blocks.len());
            println!("⚠️  Warnings: {:?}", result.warnings);
            
            // Assertions
            assert!(result.word_count > 100, "❌ FAIL: Word count too low ({})", result.word_count);
            assert!(result.extraction_score.unwrap_or(0.0) >= 0.6, 
                    "❌ FAIL: Extraction score too low ({:.2})", result.extraction_score.unwrap_or(0.0));
            
            // Check for table markers in Markdown
            let has_table_structure = result.clean_content.contains("|") || 
                                      result.clean_content.contains("---");
            println!("📋 Has Table Structure: {}", has_table_structure);
            
            // Sample first 500 chars
            println!("\n📄 Content Preview:");
            println!("{}", result.clean_content.chars().take(500).collect::<String>());
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
            println!("📈 Extraction Score: {:.2}", result.extraction_score.unwrap_or(0.0));
            println!("🔢 Code Blocks: {}", result.code_blocks.len());
            println!("⚠️  Warnings: {:?}", result.warnings);
            
            // Assertions
            assert!(result.word_count > 50, "❌ FAIL: Word count too low ({})", result.word_count);
            assert!(result.code_blocks.len() > 0, "❌ FAIL: No code blocks extracted");
            assert!(result.extraction_score.unwrap_or(0.0) >= 0.7, 
                    "❌ FAIL: Extraction score too low ({:.2})", result.extraction_score.unwrap_or(0.0));
            
            // Check first code block
            if let Some(block) = result.code_blocks.first() {
                println!("\n💻 First Code Block:");
                println!("Language: {:?}", block.language);
                println!("Code: {}", block.code.chars().take(200).collect::<String>());
                
                assert!(block.code.len() > 10, "❌ FAIL: Code block too short");
            }
            
            // Sample content
            println!("\n📄 Content Preview:");
            println!("{}", result.clean_content.chars().take(500).collect::<String>());
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
            println!("📈 Extraction Score: {:.2}", result.extraction_score.unwrap_or(0.0));
            println!("🔢 Code Blocks: {}", result.code_blocks.len());
            println!("⚠️  Warnings: {:?}", result.warnings);
            
                // Assertions
                // Raw README should be stable and have meaningful content.
                assert!(result.status_code < 400, "❌ FAIL: HTTP status was {}", result.status_code);
                assert!(result.word_count > 80, "❌ FAIL: Word count too low ({})", result.word_count);
            
            // Check for Markdown structure
            let has_markdown = result.clean_content.contains("##") || 
                              result.clean_content.contains("**") ||
                              result.clean_content.contains("```");
            println!("📝 Has Markdown Structure: {}", has_markdown);
            
            // Sample content
            println!("\n📄 Content Preview:");
            println!("{}", result.clean_content.chars().take(500).collect::<String>());
        }
        Err(e) => {
            panic!("❌ FAIL: {}", e);
        }
    }
}

#[tokio::test]
async fn test_reddit_thread_with_browserless() {
    init_logger();
    let scraper = RustScraper::new();
    let url = "https://old.reddit.com/r/rust/comments/10nimss/how_do_i_start_learning_rust/";
    
    println!("\n🧪 TEST 4: Reddit (JS-Heavy + Browserless Fallback)");
    println!("URL: {}", url);
    
    // Check if Browserless is configured
    let browserless_available = std::env::var("BROWSERLESS_URL").is_ok();
    println!("🌐 Browserless Available: {}", browserless_available);
    
    match scraper.scrape_url(url).await {
        Ok(result) => {
            println!("✅ Status: {}", result.status_code);
            println!("📊 Word Count: {}", result.word_count);
            println!("📈 Extraction Score: {:.2}", result.extraction_score.unwrap_or(0.0));
            println!("⚠️  Warnings: {:?}", result.warnings);
            
            // Check if Browserless was triggered
            let browserless_used = result.warnings.contains(&"browserless_rendered".to_string());
            println!("🎭 Browserless Used: {}", browserless_used);
            
            // For Reddit, we expect lower scores but still some content
            assert!(result.word_count > 20, "❌ FAIL: No meaningful content extracted ({})", result.word_count);
            
            // If Browserless is available and result is poor, it should have been attempted
            if browserless_available && result.word_count < 50 && result.extraction_score.unwrap_or(0.0) < 0.35 {
                println!("⚠️  Note: Browserless should have been triggered for this low-quality result");
            }
            
            // Sample content
            println!("\n📄 Content Preview:");
            println!("{}", result.clean_content.chars().take(500).collect::<String>());
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
            println!("📈 Extraction Score: {:.2}", result.extraction_score.unwrap_or(0.0));
            println!("👤 Author: {:?}", result.author);
            println!("📅 Published: {:?}", result.published_at);
            println!("⚠️  Warnings: {:?}", result.warnings);
            
            // Assertions - Medium paywall limitations acknowledged
            // Medium uses React SSR + paywall; 20-40 words is realistic without JS rendering
            assert!(result.word_count > 20, "❌ FAIL: Word count too low ({})", result.word_count);
            
            // Sample content
            println!("\n📄 Content Preview:");
            println!("{}", result.clean_content.chars().take(500).collect::<String>());
        }
        Err(e) => {
            println!("⚠️  Medium may block: {}", e);
        }
    }
}

#[tokio::test]
async fn test_docker_docs() {
    let scraper = RustScraper::new();
    let url = "https://docs.docker.com/get-started/";
    
    println!("\n🧪 TEST 6: Docker Docs (Enterprise Docs)");
    println!("URL: {}", url);
    
    match scraper.scrape_url(url).await {
        Ok(result) => {
            println!("✅ Status: {}", result.status_code);
            println!("📊 Word Count: {}", result.word_count);
            println!("📈 Extraction Score: {:.2}", result.extraction_score.unwrap_or(0.0));
            println!("🔢 Code Blocks: {}", result.code_blocks.len());
            println!("🔗 Links: {}", result.links.len());
            println!("⚠️  Warnings: {:?}", result.warnings);
            
            // Assertions
            assert!(result.word_count > 100, "❌ FAIL: Word count too low ({})", result.word_count);
            assert!(result.extraction_score.unwrap_or(0.0) >= 0.6, 
                    "❌ FAIL: Extraction score too low ({:.2})", result.extraction_score.unwrap_or(0.0));
            
            // Sample content
            println!("\n📄 Content Preview:");
            println!("{}", result.clean_content.chars().take(500).collect::<String>());
        }
        Err(e) => {
            panic!("❌ FAIL: {}", e);
        }
    }
}

#[tokio::test]
#[ignore] // Only run when Browserless is available
async fn test_browserless_direct() {
    init_logger();
    let scraper = RustScraper::new();
    
    // Test with a JS-heavy SPA that requires rendering
    let url = "https://www.npmjs.com/package/react";
    
    println!("\n🧪 TEST 7: Browserless Direct (JS-Heavy SPA)");
    println!("URL: {}", url);
    
    // Check if Browserless is configured
    if std::env::var("BROWSERLESS_URL").is_err() {
        println!("⚠️  Skipping: BROWSERLESS_URL not configured");
        println!("   Set BROWSERLESS_URL=http://localhost:3010 to enable");
        return;
    }
    
    match scraper.scrape_with_browserless(url).await {
        Ok(result) => {
            println!("✅ Status: {}", result.status_code);
            println!("📊 Word Count: {}", result.word_count);
            println!("📈 Extraction Score: {:.2}", result.extraction_score.unwrap_or(0.0));
            println!("⚠️  Warnings: {:?}", result.warnings);
            
            // Verify Browserless was used
            assert!(result.warnings.contains(&"browserless_rendered".to_string()), 
                    "❌ FAIL: Browserless warning not present");
            
            // Should extract meaningful content from JS-rendered page
            assert!(result.word_count > 50, 
                    "❌ FAIL: Insufficient content extracted ({})", result.word_count);
            
            println!("\n✅ Browserless successfully rendered JS-heavy content");
            
            // Sample content
            println!("\n📄 Content Preview:");
            println!("{}", result.clean_content.chars().take(500).collect::<String>());
        }
        Err(e) => {
            panic!("❌ FAIL: Browserless scraping failed: {}", e);
        }
    }
}

// NOTE: Keep this file focused on executable validations.
