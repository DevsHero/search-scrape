use std::collections::HashMap;
use tracing::debug;

/// Query rewriting engine to enhance search quality for developer queries
pub struct QueryRewriter {
    dev_keywords: Vec<&'static str>,
    site_mappings: HashMap<&'static str, Vec<&'static str>>,
}

impl Default for QueryRewriter {
    fn default() -> Self {
        Self::new()
    }
}

impl QueryRewriter {
    pub fn new() -> Self {
        Self {
            dev_keywords: vec![
                // Programming languages
                "rust",
                "python",
                "javascript",
                "typescript",
                "go",
                "java",
                "c++",
                "cpp",
                "ruby",
                "php",
                "swift",
                "kotlin",
                "scala",
                "haskell",
                "elixir",
                "clojure",
                // Frameworks/Libraries
                "react",
                "vue",
                "angular",
                "svelte",
                "next",
                "nuxt",
                "django",
                "flask",
                "fastapi",
                "express",
                "koa",
                "tokio",
                "actix",
                "axum",
                "rocket",
                "warp",
                "spring",
                "laravel",
                "rails",
                "phoenix",
                // Concepts
                "async",
                "await",
                "promise",
                "future",
                "mutex",
                "arc",
                "thread",
                "concurrency",
                "api",
                "rest",
                "graphql",
                "grpc",
                "websocket",
                "http",
                "tcp",
                "udp",
                "database",
                "sql",
                "nosql",
                "postgres",
                "mongodb",
                "redis",
                "sqlite",
                "kubernetes",
                "ci",
                "cd",
                "git",
                "github",
                "gitlab",
                "npm",
                "cargo",
                "pip",
                "maven",
                "gradle",
                // Dev terms
                "tutorial",
                "docs",
                "documentation",
                "guide",
                "example",
                "code",
                "install",
                "setup",
                "configure",
                "error",
                "bug",
                "fix",
                "deploy",
                "test",
                "testing",
                "debug",
                "benchmark",
                "performance",
                "optimize",
            ],
            site_mappings: {
                let mut map = HashMap::new();

                // General dev resources
                map.insert(
                    "docs",
                    vec![
                        "docs.rs",
                        "doc.rust-lang.org",
                        "developer.mozilla.org",
                        "devdocs.io",
                    ],
                );
                map.insert(
                    "documentation",
                    vec!["docs.rs", "doc.rust-lang.org", "developer.mozilla.org"],
                );

                // Language-specific
                map.insert(
                    "rust",
                    vec!["doc.rust-lang.org", "docs.rs", "rust-lang.org"],
                );
                map.insert("python", vec!["docs.python.org", "pypi.org"]);
                map.insert(
                    "javascript",
                    vec!["developer.mozilla.org", "javascript.info"],
                );
                map.insert("typescript", vec!["typescriptlang.org"]);
                map.insert("go", vec!["go.dev", "pkg.go.dev"]);

                // Frameworks
                map.insert("tokio", vec!["tokio.rs", "docs.rs"]);
                map.insert("react", vec!["react.dev", "reactjs.org"]);
                map.insert("vue", vec!["vuejs.org"]);
                map.insert("django", vec!["docs.djangoproject.com"]);

                // Q&A
                map.insert("error", vec!["stackoverflow.com", "github.com"]);
                map.insert("bug", vec!["stackoverflow.com", "github.com"]);
                map.insert("issue", vec!["stackoverflow.com", "github.com"]);

                // Packages
                map.insert("crate", vec!["crates.io", "docs.rs"]);
                map.insert("package", vec!["npmjs.com", "pypi.org", "crates.io"]);

                map
            },
        }
    }

    /// Analyze and potentially rewrite a query for better developer-focused results
    pub fn rewrite_query(&self, query: &str) -> QueryRewriteResult {
        let query_lower = query.to_lowercase();

        // Check if this is a developer query
        let is_dev_query = self.is_developer_query(&query_lower);

        if !is_dev_query {
            return QueryRewriteResult {
                original: query.to_string(),
                rewritten: None,
                suggestions: vec![],
                detected_keywords: vec![],
                is_developer_query: false,
            };
        }

        // Detect keywords in query
        let detected_keywords: Vec<String> = self
            .dev_keywords
            .iter()
            .filter(|keyword| query_lower.contains(*keyword))
            .map(|s| s.to_string())
            .collect();

        debug!("Detected developer keywords: {:?}", detected_keywords);

        // Generate site suggestions
        let mut site_suggestions = Vec::new();
        for keyword in &detected_keywords {
            if let Some(sites) = self.site_mappings.get(keyword.as_str()) {
                for site in sites {
                    if !site_suggestions.contains(&site.to_string()) {
                        site_suggestions.push(site.to_string());
                    }
                }
            }
        }

        // Generate query suggestions
        let suggestions = self.generate_suggestions(query, &detected_keywords, &site_suggestions);

        // Decide on rewritten query
        let rewritten = self.auto_rewrite_query(query, &detected_keywords, &site_suggestions);

        QueryRewriteResult {
            original: query.to_string(),
            rewritten,
            suggestions,
            detected_keywords,
            is_developer_query: true,
        }
    }

    /// Check if query is developer-related
    fn is_developer_query(&self, query_lower: &str) -> bool {
        // Check for dev keywords
        let has_dev_keyword = self
            .dev_keywords
            .iter()
            .any(|keyword| query_lower.contains(keyword));

        // Check for common dev patterns
        let has_dev_pattern = query_lower.contains("how to")
            || query_lower.contains("tutorial")
            || query_lower.contains("docs")
            || query_lower.contains("api")
            || query_lower.contains("install")
            || query_lower.contains("error")
            || query_lower.contains("example");

        has_dev_keyword || has_dev_pattern
    }

    /// Generate alternative query suggestions
    fn generate_suggestions(
        &self,
        original: &str,
        _keywords: &[String],
        sites: &[String],
    ) -> Vec<String> {
        let mut suggestions = Vec::new();

        // If query doesn't have "docs" or "tutorial", suggest adding them
        let lower = original.to_lowercase();
        if !lower.contains("docs")
            && !lower.contains("documentation")
            && !lower.contains("tutorial")
            && !_keywords.is_empty()
        {
            suggestions.push(format!("{} documentation", original));
            suggestions.push(format!("{} tutorial", original));
        }

        // Suggest site-specific searches for top 2 sites
        for site in sites.iter().take(2) {
            suggestions.push(format!("{} site:{}", original, site));
        }

        // If it's an error query, enhance it
        if (lower.contains("error") || lower.contains("bug")) && !lower.contains("stackoverflow") {
            suggestions.push(format!("{} site:stackoverflow.com", original));
        }

        suggestions
    }

    /// Auto-rewrite query if high confidence
    fn auto_rewrite_query(
        &self,
        original: &str,
        _keywords: &[String],
        sites: &[String],
    ) -> Option<String> {
        let lower = original.to_lowercase();

        // Pattern 1: Simple "rust docs" -> add site filter
        if (lower.contains("docs") || lower.contains("documentation")) && !sites.is_empty() {
            let primary_site = sites[0].clone();
            // Only rewrite if not already has site: filter
            if !lower.contains("site:") {
                return Some(format!("{} site:{}", original, primary_site));
            }
        }

        // Pattern 2: Error messages - add stackoverflow
        if (lower.contains("error:") || lower.contains("error message")) && !lower.contains("site:")
        {
            return Some(format!("{} site:stackoverflow.com", original));
        }

        // Pattern 3: "how to X in Y" where Y is a language
        for lang in &["rust", "python", "javascript", "go", "typescript"] {
            if lower.contains("how to") && lower.contains(lang) {
                if let Some(sites) = self.site_mappings.get(lang) {
                    if !lower.contains("site:") && !sites.is_empty() {
                        return Some(format!("{} site:{}", original, sites[0]));
                    }
                }
            }
        }

        // Pattern 4: Package/crate lookup
        if lower.contains("crate") && !lower.contains("site:") {
            return Some(format!("{} site:docs.rs", original));
        }

        None
    }

    /// Check if a query is similar to a recent one (for deduplication)
    pub fn is_similar_query(&self, query1: &str, query2: &str) -> bool {
        let q1 = query1.to_lowercase();
        let q2 = query2.to_lowercase();

        // Exact match
        if q1 == q2 {
            return true;
        }

        // Tokenize for word-level comparison
        let tokens1: Vec<&str> = q1.split_whitespace().collect();
        let tokens2: Vec<&str> = q2.split_whitespace().collect();

        // Check if one is a complete subset of the other (e.g., "rust" vs "rust programming")
        // But only if both have meaningful tokens
        if !tokens1.is_empty() && !tokens2.is_empty() {
            let set1: std::collections::HashSet<_> = tokens1.iter().collect();
            let set2: std::collections::HashSet<_> = tokens2.iter().collect();

            // If one set is completely contained in the other
            if set1.is_subset(&set2) || set2.is_subset(&set1) {
                return true;
            }
        }

        // For multi-word queries, check token overlap
        if tokens1.len() >= 2 && tokens2.len() >= 2 {
            let common_tokens = tokens1.iter().filter(|t| tokens2.contains(t)).count();
            let total_tokens = tokens1.len().max(tokens2.len());

            // If 70%+ tokens match, consider similar
            return (common_tokens as f32 / total_tokens as f32) > 0.7;
        }

        false
    }
}

#[derive(Debug, Clone)]
pub struct QueryRewriteResult {
    pub original: String,
    pub rewritten: Option<String>,
    pub suggestions: Vec<String>,
    pub detected_keywords: Vec<String>,
    pub is_developer_query: bool,
}

impl QueryRewriteResult {
    /// Get the best query to use (rewritten or original)
    pub fn best_query(&self) -> &str {
        self.rewritten.as_deref().unwrap_or(&self.original)
    }

    /// Check if query was enhanced
    pub fn was_rewritten(&self) -> bool {
        self.rewritten.is_some()
    }

    /// Get human-readable suggestion message
    pub fn suggestion_message(&self) -> Option<String> {
        if self.suggestions.is_empty() {
            return None;
        }

        let mut msg = String::from("💡 Suggested refined searches:\n");
        for (i, suggestion) in self.suggestions.iter().take(3).enumerate() {
            msg.push_str(&format!("   {}. {}\n", i + 1, suggestion));
        }

        Some(msg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_developer_query_detection() {
        let rewriter = QueryRewriter::new();

        assert!(rewriter.is_developer_query("rust programming tutorial"));
        assert!(rewriter.is_developer_query("how to use tokio"));
        assert!(rewriter.is_developer_query("python api documentation"));
        assert!(rewriter.is_developer_query("javascript error handling"));

        assert!(!rewriter.is_developer_query("coffee shops near me"));
        assert!(!rewriter.is_developer_query("weather forecast"));
    }

    #[test]
    fn test_query_rewriting() {
        let rewriter = QueryRewriter::new();

        let result = rewriter.rewrite_query("rust docs");
        assert!(result.is_developer_query);
        assert!(result.was_rewritten());
        assert!(result.best_query().contains("site:"));

        let result = rewriter.rewrite_query("coffee shops");
        assert!(!result.is_developer_query);
        assert!(!result.was_rewritten());
    }

    #[test]
    fn test_similar_queries() {
        let rewriter = QueryRewriter::new();

        assert!(rewriter.is_similar_query("rust programming", "rust"));
        assert!(rewriter.is_similar_query("how to use rust", "how to use rust async"));
        assert!(rewriter.is_similar_query("python tutorial", "python tutorial for beginners"));

        assert!(!rewriter.is_similar_query("rust", "python"));
        assert!(!rewriter.is_similar_query("javascript", "java"));
    }
}
