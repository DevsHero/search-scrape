use crate::types::SearchResult;
use std::cmp::Ordering;
use tracing::info;

/// Simple text similarity scoring using TF-IDF-like approach
/// Calculates relevance score between 0.0 and 1.0
pub struct Reranker {
    query_tokens: Vec<String>,
}

impl Reranker {
    /// Create a new reranker for a given query
    pub fn new(query: &str) -> Self {
        let query_tokens = Self::tokenize(query);
        Self { query_tokens }
    }

    /// Tokenize text into lowercase words
    fn tokenize(text: &str) -> Vec<String> {
        text.to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty() && s.len() > 2) // Skip short words and empty strings
            .map(|s| s.to_string())
            .collect()
    }

    /// Calculate relevance score for a search result
    pub fn score_result(&self, result: &SearchResult) -> f32 {
        if self.query_tokens.is_empty() {
            return 0.5;
        }

        let mut score = 0.0;
        let mut matches = 0;

        // Tokenize title and content
        let title_tokens = Self::tokenize(&result.title);
        let content_tokens = Self::tokenize(&result.content);

        // Count matching tokens with weights
        for query_token in &self.query_tokens {
            // Title matches (higher weight)
            if title_tokens.contains(query_token) {
                score += 0.4;
                matches += 1;
            }
            // Content matches (lower weight)
            else if content_tokens.contains(query_token) {
                score += 0.2;
                matches += 1;
            }
        }

        // Normalize: consider query complexity
        let max_score = self.query_tokens.len() as f32 * 0.4; // Max possible score
        let normalized = if max_score > 0.0 {
            (score / max_score).min(1.0)
        } else {
            0.5
        };

        // Boost by match ratio
        let match_ratio = matches as f32 / self.query_tokens.len() as f32;
        let final_score = (normalized + match_ratio) / 2.0;

        final_score.min(1.0).max(0.0)
    }

    /// Rerank search results and optionally filter by threshold
    pub fn rerank(&self, results: Vec<SearchResult>, threshold: Option<f32>) -> Vec<SearchResult> {
        // Score all results
        let mut scored: Vec<(SearchResult, f32)> = results
            .into_iter()
            .map(|r| {
                let score = self.score_result(&r);
                (r, score)
            })
            .collect();

        // Filter by threshold if provided
        if let Some(min_score) = threshold {
            scored.retain(|(_, score)| *score >= min_score);
        }

        // Sort by score descending
        scored.sort_by(|(_, a_score), (_, b_score)| {
            b_score.partial_cmp(a_score).unwrap_or(Ordering::Equal)
        });

        info!(
            "Reranked {} results (threshold: {:.2})",
            scored.len(),
            threshold.unwrap_or(0.0)
        );

        // Return only results, discard scores
        scored.into_iter().map(|(r, _)| r).collect()
    }

    /// Get top N reranked results
    pub fn rerank_top(&self, results: Vec<SearchResult>, top_n: usize) -> Vec<SearchResult> {
        let mut reranked = self.rerank(results, None);
        reranked.truncate(top_n);
        reranked
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize() {
        let tokens = Reranker::tokenize("Hello World! This is a Test-case.");
        assert!(tokens.contains(&"hello".to_string()));
        assert!(tokens.contains(&"world".to_string()));
        assert!(!tokens.contains(&"is".to_string())); // Too short
    }

    #[test]
    fn test_reranking() {
        let query = "rust programming tutorial";
        let reranker = Reranker::new(query);

        let results = vec![
            SearchResult {
                url: "https://rust-lang.org".to_string(),
                title: "The Rust Programming Language".to_string(),
                content: "Official Rust tutorial and documentation".to_string(),
                ..Default::default()
            },
            SearchResult {
                url: "https://python.org".to_string(),
                title: "Python Programming Language".to_string(),
                content: "Learn Python online".to_string(),
                ..Default::default()
            },
        ];

        let reranked = reranker.rerank(results, None);
        // Rust result should rank higher than Python result
        assert_eq!(reranked[0].title, "The Rust Programming Language");
    }
}
