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
    pub score: Option<f64>,
    #[serde(default)]
    pub published_at: Option<String>,
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

// SearXNG API types
#[derive(Debug, Deserialize)]
pub struct SearxngResponse {
    pub query: String,
    pub number_of_results: u32,
    pub results: Vec<SearxngResult>,
    #[serde(default)]
    pub infoboxes: Option<serde_json::Value>,
    #[serde(default)]
    pub suggestions: Option<serde_json::Value>,
    #[serde(default)]
    pub answers: Option<serde_json::Value>,
    #[serde(default)]
    pub corrections: Option<serde_json::Value>,
    #[serde(default)]
    pub unresponsive_engines: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct SearxngResult {
    pub url: String,
    pub title: String,
    pub content: String,
    pub engine: String,
    #[serde(default)]
    pub parsed_url: Option<Vec<String>>,
    #[serde(default)]
    pub template: Option<String>,
    #[serde(default)]
    pub engines: Option<Vec<String>>,
    #[serde(default)]
    pub positions: Option<serde_json::Value>,
    #[serde(default)]
    pub score: Option<f64>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub thumbnail: Option<String>,
    #[serde(default)]
    pub img_src: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(rename = "publishedDate", default)]
    pub published_date: Option<serde_json::Value>,
}
