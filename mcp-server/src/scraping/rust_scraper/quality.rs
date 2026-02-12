use super::RustScraper;
use crate::types::{CodeBlock, Heading};

impl RustScraper {
    /// Calculate extraction quality score (Priority 1 fix)
    /// Returns a score from 0.0 to 1.0 indicating extraction quality
    pub(super) fn calculate_extraction_score(
        &self,
        word_count: usize,
        published_at: &Option<String>,
        code_blocks: &[CodeBlock],
        headings: &[Heading],
    ) -> f64 {
        let mut score = 0.0;

        // Content presence (0.0-0.3)
        if word_count > 50 {
            score += 0.3;
        } else if word_count > 20 {
            score += 0.15;
        }

        // Has publish date (0.2)
        if published_at.is_some() {
            score += 0.2;
        }

        // Has code blocks (0.2) - good for technical content
        if !code_blocks.is_empty() {
            score += 0.2;
        }

        // Has structured headings (0.15)
        if headings.len() > 2 {
            score += 0.15;
        } else if !headings.is_empty() {
            score += 0.075;
        }

        // Content length score (0.0-0.15)
        // Optimal around 500-2000 words
        let length_score = if (500..=2000).contains(&word_count) {
            0.15
        } else if word_count > 2000 {
            0.15 * (2000.0 / word_count as f64).min(1.0)
        } else if word_count > 100 {
            0.15 * (word_count as f64 / 500.0)
        } else {
            0.0
        };
        score += length_score;

        score.min(1.0)
    }
}
