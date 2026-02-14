/// Debug Test: Deep dive into extraction failures
use shadowcrawl::rust_scraper::RustScraper;

#[tokio::test]
async fn debug_wikipedia_extraction() {
    let scraper = RustScraper::new();
    let url = "https://en.wikipedia.org/wiki/Rust_(programming_language)";

    println!("\n🔍 DEBUG: Wikipedia Extraction");

    match scraper.scrape_url(url).await {
        Ok(result) => {
            println!("\n=== RAW RESULTS ===");
            println!("Title: {}", result.title);
            println!("Word Count: {}", result.word_count);
            println!("Clean Content Length: {}", result.clean_content.len());
            println!("Code Blocks: {}", result.code_blocks.len());
            println!(
                "Extraction Score: {:.2}",
                result.extraction_score.unwrap_or(0.0)
            );

            println!("\n=== CLEAN CONTENT (first 1000 chars) ===");
            println!(
                "{}",
                result.clean_content.chars().take(1000).collect::<String>()
            );

            println!("\n=== CLEAN CONTENT (last 500 chars) ===");
            let content_len = result.clean_content.len();
            if content_len > 500 {
                println!(
                    "{}",
                    result
                        .clean_content
                        .chars()
                        .skip(content_len - 500)
                        .collect::<String>()
                );
            }

            println!("\n=== CODE BLOCKS SAMPLE ===");
            for (i, block) in result.code_blocks.iter().take(3).enumerate() {
                println!(
                    "\nBlock {}: Language={:?}, Length={}",
                    i + 1,
                    block.language,
                    block.code.len()
                );
                println!("Code: {}", block.code.chars().take(100).collect::<String>());
            }

            println!("\n=== WARNINGS ===");
            for warning in &result.warnings {
                println!("⚠️  {}", warning);
            }

            // Check if content is actually empty vs just whitespace
            let trimmed = result.clean_content.trim();
            println!("\n=== ANALYSIS ===");
            println!("Original length: {}", result.clean_content.len());
            println!("Trimmed length: {}", trimmed.len());
            println!("Is empty: {}", result.clean_content.is_empty());
            println!("Is only whitespace: {}", trimmed.is_empty());
        }
        Err(e) => {
            println!("❌ Error: {}", e);
        }
    }
}

#[tokio::test]
async fn debug_rust_docs_extraction() {
    let scraper = RustScraper::new();
    let url = "https://doc.rust-lang.org/book/ch01-02-hello-world.html";

    println!("\n🔍 DEBUG: Rust Docs Extraction");

    match scraper.scrape_url(url).await {
        Ok(result) => {
            println!("\n=== RAW RESULTS ===");
            println!("Title: {}", result.title);
            println!("Word Count: {}", result.word_count);
            println!("Clean Content Length: {}", result.clean_content.len());
            println!("Code Blocks: {}", result.code_blocks.len());
            println!(
                "Extraction Score: {:.2}",
                result.extraction_score.unwrap_or(0.0)
            );

            println!("\n=== CLEAN CONTENT (first 1000 chars) ===");
            println!(
                "{}",
                result.clean_content.chars().take(1000).collect::<String>()
            );

            println!("\n=== CODE BLOCKS SAMPLE ===");
            for (i, block) in result.code_blocks.iter().take(2).enumerate() {
                println!("\nBlock {}: Language={:?}", i + 1, block.language);
                println!("Code: {}", block.code.chars().take(100).collect::<String>());
            }
        }
        Err(e) => {
            println!("❌ Error: {}", e);
        }
    }
}
