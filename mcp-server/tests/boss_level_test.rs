use shadowcrawl::scrape::scrape_url;
/// Boss Level Integration Tests: The Hard Cases
/// Tests the most challenging web scraping scenarios through the full AppState pipeline
use shadowcrawl::AppState;
use std::sync::Arc;

// Initialize logging for tests
fn init_logger() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_test_writer()
        .try_init();
}

// Create AppState for testing
fn create_test_state() -> Arc<AppState> {
    let searxng_url =
        std::env::var("SEARXNG_URL").unwrap_or_else(|_| "http://localhost:8888".to_string());
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap();

    Arc::new(AppState::new(searxng_url, client))
}

#[tokio::test]
#[ignore] // Run with: cargo test --test boss_level -- --ignored --nocapture
async fn boss_1_linkedin_job_description() {
    init_logger();
    let state = create_test_state();

    // LinkedIn: Use public job search page instead of specific job (requires login)
    let url = "https://www.linkedin.com/jobs/search/?keywords=software%20engineer";

    println!("\n🎯 BOSS LEVEL 1: LinkedIn Job Search");
    println!("URL: {}", url);
    println!("Challenge: Extract job listings without login wall");

    // Check if Browserless is available
    let browserless_available = std::env::var("BROWSERLESS_URL").is_ok();
    println!("🌐 Browserless Available: {}", browserless_available);

    match scrape_url(&state, url).await {
        Ok(result) => {
            println!("✅ Status: {}", result.status_code);
            println!("📊 Word Count: {}", result.word_count);
            println!(
                "📈 Extraction Score: {:.2}",
                result.extraction_score.unwrap_or(0.0)
            );
            println!("⚠️  Warnings: {:?}", result.warnings);

            // Check if Browserless was used
            let browserless_used = result
                .warnings
                .contains(&"browserless_rendered".to_string());
            println!("🎭 Browserless Used: {}", browserless_used);

            // LinkedIn is challenging, accept lower threshold
            if result.word_count > 50 {
                println!("✅ PASS: Extracted {} words", result.word_count);
            } else {
                println!(
                    "⚠️  LOW QUALITY: Only {} words extracted",
                    result.word_count
                );
                println!("   This may be expected due to login wall");
            }

            // Sample content
            println!("\n📄 Content Preview:");
            println!(
                "{}",
                result.clean_content.chars().take(500).collect::<String>()
            );
        }
        Err(e) => {
            println!("❌ ERROR: {}", e);
            println!("   Expected: LinkedIn aggressively blocks scrapers");
        }
    }
}

#[tokio::test]
#[ignore]
async fn boss_2_amazon_product_specs() {
    init_logger();
    let state = create_test_state();

    // Amazon product page (popular item like Kindle)
    let url = "https://www.amazon.com/dp/B09SWW583J";

    println!("\n🎯 BOSS LEVEL 2: Amazon Product Specs");
    println!("URL: {}", url);
    println!("Challenge: Extract clean product details without 'also bought' noise");

    match scrape_url(&state, url).await {
        Ok(result) => {
            println!("✅ Status: {}", result.status_code);
            println!("📊 Word Count: {}", result.word_count);
            println!(
                "📈 Extraction Score: {:.2}",
                result.extraction_score.unwrap_or(0.0)
            );

            // Check for noise indicators
            let has_recommendations = result
                .clean_content
                .to_lowercase()
                .contains("customers who bought")
                || result
                    .clean_content
                    .to_lowercase()
                    .contains("frequently bought");
            println!("🔊 Contains Recommendation Noise: {}", has_recommendations);

            // Amazon should have substantial product info
            if result.word_count > 100 && result.extraction_score.unwrap_or(0.0) > 0.5 {
                println!("✅ PASS: Good extraction quality");
            } else {
                println!("⚠️  NEEDS IMPROVEMENT: Low quality or too much noise");
            }

            println!("\n📄 Content Preview:");
            println!(
                "{}",
                result.clean_content.chars().take(700).collect::<String>()
            );
        }
        Err(e) => {
            println!("❌ ERROR: {}", e);
        }
    }
}

#[tokio::test]
#[ignore]
async fn boss_3_substack_full_article() {
    init_logger();
    let state = create_test_state();

    // Substack: Use .substack.com domain (Lenny's Newsletter)
    let url = "https://lennysnewsletter.substack.com/";

    println!("\n🎯 BOSS LEVEL 3: Substack Newsletter Homepage");
    println!("URL: {}", url);
    println!("Challenge: Extract content with Browserless + JS rendering");

    let browserless_available = std::env::var("BROWSERLESS_URL").is_ok();

    match scrape_url(&state, url).await {
        Ok(result) => {
            println!("✅ Status: {}", result.status_code);
            println!("📊 Word Count: {}", result.word_count);
            println!(
                "📈 Extraction Score: {:.2}",
                result.extraction_score.unwrap_or(0.0)
            );
            println!("⚠️  Warnings: {:?}", result.warnings);

            let browserless_used = result
                .warnings
                .contains(&"browserless_rendered".to_string());
            println!("🎭 Browserless Used: {}", browserless_used);

            // Substack articles should be substantial
            if result.word_count > 200 {
                println!(
                    "✅ PASS: Extracted full article ({} words)",
                    result.word_count
                );
            } else {
                println!("⚠️  INCOMPLETE: Only {} words", result.word_count);
                if !browserless_available {
                    println!("   💡 TIP: Enable Browserless for better JS-heavy extraction");
                }
            }

            println!("\n📄 Content Preview:");
            println!(
                "{}",
                result.clean_content.chars().take(600).collect::<String>()
            );
        }
        Err(e) => {
            println!("❌ ERROR: {}", e);
        }
    }
}

#[tokio::test]
#[ignore]
async fn boss_4_zillow_property_details() {
    init_logger();
    let state = create_test_state();

    // Zillow: Use search results page (more stable than specific listings)
    let url = "https://www.zillow.com/san-francisco-ca/";

    println!("\n🎯 BOSS LEVEL 4: Zillow Search Results");
    println!("URL: {}", url);
    println!("Challenge: Extract listings despite aggressive bot detection");

    match scrape_url(&state, url).await {
        Ok(result) => {
            println!("✅ Status: {}", result.status_code);
            println!("📊 Word Count: {}", result.word_count);
            println!(
                "📈 Extraction Score: {:.2}",
                result.extraction_score.unwrap_or(0.0)
            );

            // Check for price indicators
            let has_price = result.clean_content.contains("$")
                || result.clean_content.to_lowercase().contains("price");
            println!("💰 Contains Price Info: {}", has_price);

            // Check for property details
            let has_details = result.clean_content.to_lowercase().contains("bed")
                || result.clean_content.to_lowercase().contains("bath")
                || result.clean_content.to_lowercase().contains("sqft");
            println!("🏠 Contains Property Details: {}", has_details);

            if result.word_count > 50 && (has_price || has_details) {
                println!("✅ PASS: Extracted property information");
            } else {
                println!("⚠️  BLOCKED: Likely detected as bot");
                println!("   Zillow has aggressive anti-scraping measures");
            }

            println!("\n📄 Content Preview:");
            println!(
                "{}",
                result.clean_content.chars().take(500).collect::<String>()
            );
        }
        Err(e) => {
            println!("❌ ERROR: {}", e);
            println!("   Expected: Zillow may return 403 Forbidden");
        }
    }
}

#[tokio::test]
#[ignore]
async fn boss_5_github_search_results() {
    init_logger();
    let state = create_test_state();

    // GitHub search (should work without auth)
    let url = "https://github.com/search?q=rust+web+scraping&type=repositories";

    println!("\n🎯 BOSS LEVEL 5: GitHub Search Results");
    println!("URL: {}", url);
    println!("Challenge: Scrape result list without 429 Rate Limit");

    match scrape_url(&state, url).await {
        Ok(result) => {
            println!("✅ Status: {}", result.status_code);
            println!("📊 Word Count: {}", result.word_count);
            println!(
                "📈 Extraction Score: {:.2}",
                result.extraction_score.unwrap_or(0.0)
            );

            // Check if rate limited
            let rate_limited = result.status_code == 429
                || result.clean_content.to_lowercase().contains("rate limit");
            println!("⚠️  Rate Limited: {}", rate_limited);

            // Check for repository names
            let has_repos = result.clean_content.contains("repository")
                || result.clean_content.contains("repo");
            println!("📦 Contains Repository Info: {}", has_repos);

            if !rate_limited && result.word_count > 100 {
                println!("✅ PASS: Successfully scraped search results");
            } else if rate_limited {
                println!("⚠️  RATE LIMITED: GitHub detected scraping");
                println!("   Stealth measures may need improvement");
            } else {
                println!("⚠️  LOW QUALITY: Insufficient content extracted");
            }

            println!("\n📄 Content Preview:");
            println!(
                "{}",
                result.clean_content.chars().take(600).collect::<String>()
            );
        }
        Err(e) => {
            println!("❌ ERROR: {}", e);
        }
    }
}

/// Summary test to run all boss cases sequentially
#[tokio::test]
#[ignore]
async fn boss_level_complete_suite() {
    init_logger();

    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║          BOSS LEVEL COMPLETE TEST SUITE                 ║");
    println!("║   Testing the hardest web scraping challenges          ║");
    println!("╚══════════════════════════════════════════════════════════╝\n");

    let passed = 0;
    let failed = 0;
    let total = 5;

    println!("🚀 Starting Boss Level Tests...\n");

    // Run each test and collect results
    // Note: In real scenario, you'd want better error handling

    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║                  FINAL RESULTS                           ║");
    println!("╠══════════════════════════════════════════════════════════╣");
    println!(
        "║  Total Tests: {}                                          ║",
        total
    );
    println!(
        "║  Passed: {}                                               ║",
        passed
    );
    println!(
        "║  Failed: {}                                               ║",
        failed
    );
    println!(
        "║  Success Rate: {}%                                        ║",
        (passed * 100) / total
    );
    println!("╚══════════════════════════════════════════════════════════╝\n");
}

/// 👹 GOD LEVEL 1: NowSecure - Cloudflare Turnstile Bypass
#[tokio::test]
#[ignore]
async fn god_1_nowsecure_cloudflare() {
    init_logger();
    let state = create_test_state();

    let url = "https://nowsecure.nl";

    println!("\n👹 GOD LEVEL 1: NowSecure - Cloudflare Turnstile");
    println!("URL: {}", url);
    println!("Challenge: Bypass Cloudflare protection and see success message");
    println!("Victory Condition: Extract 'OH HAI, I CAN HAZ REQUEST'");

    match scrape_url(&state, url).await {
        Ok(result) => {
            println!("✅ Status: {}", result.status_code);
            println!("📊 Word Count: {}", result.word_count);

            let success_msg = "OH HAI, I CAN HAZ REQUEST";
            let has_success = result.clean_content.contains(success_msg)
                || result.clean_content.to_uppercase().contains("OH HAI");

            let has_cloudflare_block = result.clean_content.contains("Cloudflare")
                || result.clean_content.contains("Just a moment")
                || result.clean_content.contains("Checking your browser");

            println!("🎯 Success Message Found: {}", has_success);
            println!("🛡️ Cloudflare Block Detected: {}", has_cloudflare_block);

            if has_success {
                println!("🏆 GOD LEVEL PASSED: Cloudflare Turnstile bypassed!");
            } else if has_cloudflare_block {
                println!("❌ BLOCKED: Cloudflare challenge not solved");
            } else {
                println!("⚠️  UNKNOWN: Check content below");
            }

            println!("\n📄 Content Preview:");
            println!(
                "{}",
                result.clean_content.chars().take(500).collect::<String>()
            );
        }
        Err(e) => {
            println!("❌ ERROR: {}", e);
        }
    }
}

/// 👹 GOD LEVEL 2: TikTok - Canvas Fingerprint Bypass
#[tokio::test]
#[ignore]
async fn god_2_tiktok_fingerprint() {
    init_logger();
    let state = create_test_state();

    let url = "https://www.tiktok.com/@tiktok";

    println!("\n👹 GOD LEVEL 2: TikTok - Canvas Fingerprint Bypass");
    println!("URL: {}", url);
    println!("Challenge: Bypass ByteDance canvas/WebGL fingerprinting");
    println!("Victory Condition: Extract follower count or profile data");

    match scrape_url(&state, url).await {
        Ok(result) => {
            println!("✅ Status: {}", result.status_code);
            println!("📊 Word Count: {}", result.word_count);

            let has_followers = result.clean_content.to_lowercase().contains("followers")
                || result.clean_content.to_lowercase().contains("following");

            let has_block =
                result.clean_content.contains("captcha") || result.clean_content.contains("verify");

            println!("👥 Follower Data Found: {}", has_followers);
            println!("🚫 Block Detected: {}", has_block);

            if has_followers && result.word_count > 50 {
                println!("🏆 GOD LEVEL PASSED: Fingerprint bypass successful!");
            } else if has_block {
                println!("❌ BLOCKED: Fingerprint detected or captcha triggered");
            } else {
                println!("⚠️  INCOMPLETE: Insufficient data extracted");
            }

            println!("\n📄 Content Preview:");
            println!(
                "{}",
                result.clean_content.chars().take(500).collect::<String>()
            );
        }
        Err(e) => {
            println!("❌ ERROR: {}", e);
        }
    }
}

/// 👹 GOD LEVEL 3: OpenSea - Multi-layer Protection
#[tokio::test]
#[ignore]
async fn god_3_opensea_waf() {
    init_logger();
    let state = create_test_state();

    let url = "https://opensea.io";

    println!("\n👹 GOD LEVEL 3: OpenSea - Cloudflare + WAF");
    println!("URL: {}", url);
    println!("Challenge: Bypass multi-layer protection (Cloudflare + behavior analysis)");
    println!("Victory Condition: Extract NFT marketplace content");

    match scrape_url(&state, url).await {
        Ok(result) => {
            println!("✅ Status: {}", result.status_code);
            println!("📊 Word Count: {}", result.word_count);

            let has_nft = result.clean_content.to_lowercase().contains("nft")
                || result.clean_content.to_lowercase().contains("collection");

            let has_cloudflare = result.clean_content.contains("Cloudflare")
                || result.clean_content.contains("Just a moment");

            println!("🎨 NFT Content Found: {}", has_nft);
            println!("🛡️ Cloudflare Block: {}", has_cloudflare);

            if has_nft && result.word_count > 100 {
                println!("🏆 GOD LEVEL PASSED: Multi-layer bypass successful!");
            } else if has_cloudflare {
                println!("❌ BLOCKED: Cloudflare protection active");
            } else {
                println!("⚠️  INCOMPLETE: Limited extraction");
            }

            println!("\n📄 Content Preview:");
            println!(
                "{}",
                result.clean_content.chars().take(500).collect::<String>()
            );
        }
        Err(e) => {
            println!("❌ ERROR: {}", e);
        }
    }
}

/// 🧪 FINAL BENCHMARK 1: Bloomberg Search (WAF + Paywall)
#[tokio::test]
#[ignore]
async fn benchmark_bloomberg_search() {
    init_logger();
    let state = create_test_state();

    let url = "https://www.bloomberg.com/search?query=AI";

    println!("\n🧪 FINAL BENCHMARK: Bloomberg Search");
    println!("URL: {}", url);
    println!("Challenge: Extract article snippets despite WAF + paywall");
    println!("Victory Condition: >100 words of actual search results");

    match scrape_url(&state, url).await {
        Ok(result) => {
            println!("✅ Status: {}", result.status_code);
            println!("📊 Word Count: {}", result.word_count);

            let has_articles = result.clean_content.to_lowercase().contains("article")
                || result
                    .clean_content
                    .to_lowercase()
                    .contains("search results");

            let has_block = result.clean_content.contains("Access Denied")
                || result.clean_content.contains("denied");

            println!("📰 Article Content Found: {}", has_articles);
            println!("🚫 Access Block: {}", has_block);

            if has_articles && result.word_count > 100 && !has_block {
                println!("🏆 BENCHMARK PASSED: Bloomberg search extracted!");
            } else if has_block {
                println!("❌ BLOCKED: Bloomberg WAF active");
            } else {
                println!("⚠️  INCOMPLETE: Insufficient content");
            }

            println!("\n📄 Content Preview:");
            println!(
                "{}",
                result.clean_content.chars().take(600).collect::<String>()
            );
        }
        Err(e) => {
            println!("❌ ERROR: {}", e);
        }
    }
}

/// 🧪 FINAL BENCHMARK 2: Reddit - Sidebar Noise Filtering
#[tokio::test]
#[ignore]
async fn benchmark_reddit_subreddit() {
    init_logger();
    let state = create_test_state();

    let url = "https://www.reddit.com/r/rust/";

    println!("\n🧪 FINAL BENCHMARK: Reddit Rust Subreddit");
    println!("URL: {}", url);
    println!("Challenge: Extract thread titles WITHOUT sidebar clutter");
    println!("Victory Condition: >3 clear thread titles, <30% sidebar noise");

    match scrape_url(&state, url).await {
        Ok(result) => {
            println!("✅ Status: {}", result.status_code);
            println!("📊 Word Count: {}", result.word_count);

            let has_threads =
                result.clean_content.contains("post") || result.clean_content.contains("upvote");

            // Check for excessive sidebar noise
            let sidebar_keywords = ["Rules", "Related", "Moderators", "Community"];
            let noise_count = sidebar_keywords
                .iter()
                .filter(|&kw| result.clean_content.contains(kw))
                .count();

            println!("💬 Thread Content Found: {}", has_threads);
            println!("📏 Sidebar Noise Level: {}/4 keywords", noise_count);

            if has_threads && result.word_count > 100 && noise_count < 3 {
                println!("🏆 BENCHMARK PASSED: Clean Reddit extraction!");
            } else if noise_count >= 3 {
                println!("⚠️  HIGH NOISE: Sidebar content not filtered");
            } else {
                println!("⚠️  INCOMPLETE: Insufficient thread data");
            }

            println!("\n📄 Content Preview:");
            println!(
                "{}",
                result.clean_content.chars().take(700).collect::<String>()
            );
        }
        Err(e) => {
            println!("❌ ERROR: {}", e);
        }
    }
}
