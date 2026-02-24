use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchRequest {
    pub query: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchStructuredRequest {
    pub query: String,
    #[serde(default)]
    pub top_n: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchResponse {
    pub results: Vec<SearchResult>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchStructuredResponse {
    pub results: Vec<SearchResult>,
    pub scraped_content: Vec<ScrapeResponse>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct SearchResult {
    pub url: String,
    pub title: String,
    pub content: String,
    pub engine: Option<String>,
    /// Primary engine label for this result (single source).
    #[serde(default)]
    pub engine_source: Option<String>,
    /// All corroborating engine labels (multi-source).
    #[serde(default)]
    pub engine_sources: Vec<String>,
    pub score: Option<f64>,
    #[serde(default)]
    pub published_at: Option<String>,
    /// Best-effort breadcrumb-like path (domain + path segments, or SERP-provided hints).
    #[serde(default)]
    pub breadcrumbs: Vec<String>,
    /// Best-effort extra SERP metadata (e.g., fact rows) beyond the normal snippet.
    #[serde(default)]
    pub rich_snippet: Option<String>,
    /// Best-effort top answer (featured snippet / answer box / PAA extraction).
    #[serde(default)]
    pub top_answer: Option<String>,
    // New Priority 2 fields for better filtering
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub source_type: Option<String>, // docs, repo, blog, news, other
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScrapeRequest {
    pub url: String,
    #[serde(default)]
    pub content_links_only: Option<bool>,
    #[serde(default)]
    pub max_links: Option<usize>,
    #[serde(default)]
    pub max_images: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ScrapeResponse {
    pub url: String,
    pub title: String,
    pub content: String,
    pub clean_content: String,
    #[serde(default)]
    pub embedded_state_json: Option<String>,
    #[serde(default)]
    pub embedded_data_sources: Vec<EmbeddedDataSource>,
    #[serde(default)]
    pub hydration_status: HydrationStatus,
    pub meta_description: String,
    pub meta_keywords: String,
    pub headings: Vec<Heading>,
    pub links: Vec<Link>,
    pub images: Vec<Image>,
    pub timestamp: String,
    pub status_code: u16,
    pub content_type: String,
    pub word_count: usize,
    pub language: String,
    #[serde(default)]
    pub canonical_url: Option<String>,
    #[serde(default)]
    pub site_name: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub published_at: Option<String>,
    #[serde(default)]
    pub og_title: Option<String>,
    #[serde(default)]
    pub og_description: Option<String>,
    #[serde(default)]
    pub og_image: Option<String>,
    #[serde(default)]
    pub reading_time_minutes: Option<u32>,
    // New Priority 1 fields
    #[serde(default)]
    pub code_blocks: Vec<CodeBlock>,
    #[serde(default)]
    pub truncated: bool,
    #[serde(default)]
    pub actual_chars: usize,
    #[serde(default)]
    pub max_chars_limit: Option<usize>,
    #[serde(default)]
    pub extraction_score: Option<f64>,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub domain: Option<String>,
    /// Populated when an Auth-Wall is detected (HTTP-200 login page).
    /// The handler uses this to return a structured `blocked_by_auth` response.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_wall_reason: Option<String>,

    /// Continuous auth-risk probability (0.0 = safe, 1.0 = almost certainly an auth wall).
    /// Agents should call `visual_scout` when this value is >= 0.4.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_risk_score: Option<f32>,

    /// Human-readable factors that contributed to `auth_risk_score`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub detection_factors: Vec<String>,

    /// Final URL after any server-side redirects, when detectable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub final_url: Option<String>,
}

// ───────────────────────────────────────────────────────────────────────────
// 🔒 Auth-Wall Blocked Response — Feature 2
// Structured JSON returned instead of garbage content when a login wall is hit.
// ───────────────────────────────────────────────────────────────────────────

/// Returned by the `scrape_url` / `crawl_website` tools when an auth-wall is
/// detected.  Never returns a broken page; always surfaces a clear action plan.
#[derive(Debug, Serialize, Deserialize)]
pub struct AuthWallBlocked {
    /// Always `"blocked_by_auth"` — lets callers pattern-match on `status`.
    pub status: String,
    /// Human-readable description of how the wall was detected.
    pub reason: String,
    /// The URL that triggered the wall.
    pub url: String,
    /// Canonical action agents should take next.
    pub suggested_action: String,
    /// For GitHub blob pages: the equivalent raw.githubusercontent.com URL that
    /// was already attempted (or should be attempted with credentials).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub github_raw_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EmbeddedDataSource {
    pub source_type: String,
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct HydrationStatus {
    #[serde(default)]
    pub json_found: bool,
    #[serde(default)]
    pub settle_time_ms: Option<u64>,
    #[serde(default)]
    pub noise_reduction_ratio: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CodeBlock {
    pub language: Option<String>,
    pub code: String,
    #[serde(default)]
    pub start_char: Option<usize>,
    #[serde(default)]
    pub end_char: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Heading {
    pub level: String,
    pub text: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Link {
    pub url: String,
    pub text: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Image {
    pub src: String,
    pub alt: String,
    pub title: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatRequest {
    pub query: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatResponse {
    pub response: String,
    pub search_results: Vec<SearchResult>,
    pub scraped_content: Vec<ScrapeResponse>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
}

// Batch scraping types
#[derive(Debug, Serialize, Deserialize)]
pub struct ScrapeBatchRequest {
    pub urls: Vec<String>,
    #[serde(default)]
    pub max_concurrent: Option<usize>,
    #[serde(default)]
    pub max_chars: Option<usize>,
    #[serde(default)]
    pub output_format: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ScrapeBatchResult {
    pub url: String,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<ScrapeResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
    pub duration_ms: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScrapeBatchResponse {
    pub total: usize,
    pub successful: usize,
    pub failed: usize,
    pub total_duration_ms: u64,
    pub results: Vec<ScrapeBatchResult>,
}

// Website crawling types
#[derive(Debug, Serialize, Deserialize)]
pub struct CrawlRequest {
    pub url: String,
    #[serde(default)]
    pub max_depth: Option<usize>,
    #[serde(default)]
    pub max_pages: Option<usize>,
    #[serde(default)]
    pub max_concurrent: Option<usize>,
    #[serde(default)]
    pub include_patterns: Option<Vec<String>>,
    #[serde(default)]
    pub exclude_patterns: Option<Vec<String>>,
    #[serde(default)]
    pub same_domain_only: Option<bool>,
    #[serde(default)]
    pub max_chars_per_page: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CrawlPageResult {
    pub url: String,
    pub depth: usize,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub word_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub links_found: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_preview: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub duration_ms: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CrawlResponse {
    pub start_url: String,
    pub pages_crawled: usize,
    pub pages_failed: usize,
    pub max_depth_reached: usize,
    pub total_duration_ms: u64,
    pub unique_domains: Vec<String>,
    pub results: Vec<CrawlPageResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sitemap: Option<Vec<String>>,
}

// Structured extraction types
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExtractField {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub field_type: Option<String>, // string, number, boolean, array, object
    #[serde(default)]
    pub required: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExtractRequest {
    pub url: String,
    #[serde(default)]
    pub schema: Option<Vec<ExtractField>>,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub max_chars: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExtractResponse {
    pub url: String,
    pub title: String,
    pub extracted_data: serde_json::Value,
    pub raw_content_preview: String,
    pub extraction_method: String,
    pub field_count: usize,
    pub confidence: f64,
    pub duration_ms: u64,
    #[serde(default)]
    pub warnings: Vec<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// 🎯 Sniper Mode — Feature 3: Token-Optimised "clean_json" Output
// Lean structured output designed to maximise information density per token.
// ─────────────────────────────────────────────────────────────────────────────

/// Token-optimised JSON output for `output_format = "clean_json"`.
///
/// Strips all navigation, headers, footers, sidebars, and boilerplate.
/// Returns only `title`, substantive body paragraphs, code blocks, and
/// minimal metadata — designed to minimise LLM context consumption.
#[derive(Debug, Serialize, Deserialize)]
pub struct SniperOutput {
    /// The page title.
    pub title: String,
    /// Condensed one-sentence summary bullets — first sentence of each key paragraph.
    /// Designed for agents that need a quick overview without reading full paragraphs.
    pub key_points: Vec<String>,
    /// Substantive body paragraphs with noise filtered out.
    pub key_paragraphs: Vec<String>,
    /// Code blocks extracted from the page.
    pub key_code_blocks: Vec<SniperCodeBlock>,
    /// Minimal provenance metadata.
    pub metadata: SniperMetadata,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SniperCodeBlock {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// Surrounding prose — the sentence or line that introduces or refers to this code block.
    /// Provides LLMs / agents with the \"where and how\" context needed to use the code correctly.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    pub code: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SniperMetadata {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_at: Option<String>,
    pub word_count: usize,
    /// Extraction quality score [0.0–1.0]. < 0.4 = low confidence.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extraction_score: Option<f64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// 🔬 Deep Research types
// ─────────────────────────────────────────────────────────────────────────────

/// A single relevant source discovered during deep research.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DeepResearchSource {
    /// Source URL.
    pub url: String,
    /// Page title.
    pub title: String,
    /// Domain this source belongs to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    /// Semantically-filtered relevant content from this page.
    pub relevant_content: String,
    /// Word count of `relevant_content`.
    pub word_count: usize,
    /// Research depth this source was discovered at (1 = first hop).
    pub depth: u8,
    /// The sub-query that led to this source, if applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub via_query: Option<String>,
}

/// Full output from the `deep_research` tool.
#[derive(Debug, Serialize, Deserialize)]
pub struct DeepResearchResult {
    /// The original research query.
    pub query: String,
    /// Maximum hop depth that was executed (may be < requested if capped).
    pub depth_used: u8,
    /// Number of unique URLs discovered across all search hops.
    pub sources_discovered: usize,
    /// Number of URLs actually scraped.
    pub sources_scraped: usize,
    /// Top relevant sources (semantically filtered), sorted by relevance.
    pub key_findings: Vec<DeepResearchSource>,
    /// All discovered URLs (deduplicated) for reference.
    pub all_urls: Vec<String>,
    /// All sub-queries used across all hops.
    pub sub_queries: Vec<String>,
    /// Non-fatal warnings accumulated during the research run.
    pub warnings: Vec<String>,
    /// Total wall-clock time for the full research pipeline.
    pub total_duration_ms: u64,
}
