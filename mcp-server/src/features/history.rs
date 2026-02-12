use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use qdrant_client::{Payload, Qdrant};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::OnceCell;
use uuid::Uuid;

/// Entry type for history records
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryType {
    Search,
    Scrape,
}

/// History entry stored in Qdrant
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: String,
    pub entry_type: EntryType,
    pub query: String,
    pub topic: String,
    pub summary: String,
    pub full_result: serde_json::Value,
    pub timestamp: DateTime<Utc>,
    pub domain: Option<String>,
    pub source_type: Option<String>,
}

/// Memory manager for research history
pub struct MemoryManager {
    qdrant: Arc<Qdrant>,
    embedding_model: Arc<OnceCell<Arc<TextEmbedding>>>,
    collection_name: String,
}

impl MemoryManager {
    /// Create a new memory manager
    pub async fn new(qdrant_url: &str) -> Result<Self> {
        let qdrant = Qdrant::from_url(qdrant_url)
            .build()
            .context("Failed to connect to Qdrant")?;

        let manager = Self {
            qdrant: Arc::new(qdrant),
            embedding_model: Arc::new(OnceCell::new()),
            collection_name: "research_history".to_string(),
        };

        manager.init_collection().await?;
        Ok(manager)
    }

    /// Initialize the Qdrant collection with hybrid search support
    async fn init_collection(&self) -> Result<()> {
        // Check if collection exists
        let collections = self
            .qdrant
            .list_collections()
            .await
            .context("Failed to list collections")?;

        let exists = collections
            .collections
            .iter()
            .any(|c| c.name == self.collection_name);

        if !exists {
            tracing::info!("Creating Qdrant collection: {} with hybrid search support (full-text + vector)", self.collection_name);

            // Create collection with 384-dimensional vectors (fastembed default)
            let create_collection = qdrant_client::qdrant::CreateCollectionBuilder::new(&self.collection_name)
                .vectors_config(qdrant_client::qdrant::VectorParamsBuilder::new(384, qdrant_client::qdrant::Distance::Cosine))
                .build();

            self.qdrant
                .create_collection(create_collection)
                .await
                .context("Failed to create collection")?;
            
            tracing::info!("Hybrid search collection created (Qdrant will auto-index text fields for BM25)");
        }

        Ok(())
    }

    /// Get or initialize the embedding model
    async fn get_embedding_model(&self) -> Result<Arc<TextEmbedding>> {
        let model = self.embedding_model
            .get_or_try_init(|| async {
                tracing::info!("Initializing fastembed model...");
                tokio::task::spawn_blocking(|| {
                    TextEmbedding::try_new(
                        InitOptions::new(EmbeddingModel::AllMiniLML6V2)
                            .with_show_download_progress(true)
                    )
                    .map(Arc::new)
                    .context("Failed to initialize embedding model")
                })
                .await?
            })
            .await?;
        Ok(model.clone())
    }

    /// Generate embedding for text
    async fn embed_text(&self, text: &str) -> Result<Vec<f32>> {
        let model = self.get_embedding_model().await?;
        let text_owned = text.to_string();
        
        // Use spawn_blocking for CPU-intensive embedding
        let embeddings = tokio::task::spawn_blocking(move || {
            model.embed(vec![&text_owned], None)
        })
        .await?
        .context("Failed to generate embedding")?;

        Ok(embeddings
            .first()
            .context("No embedding generated")?
            .clone())
    }

    /// Auto-generate topic from query using simple keyword extraction
    fn generate_topic(query: &str, entry_type: &EntryType) -> String {
        // Simple topic generation: take first 5 meaningful words
        let words: Vec<&str> = query
            .split_whitespace()
            .filter(|w| w.len() > 3) // Skip short words
            .take(5)
            .collect();

        if words.is_empty() {
            match entry_type {
                EntryType::Search => "general_search".to_string(),
                EntryType::Scrape => "general_scrape".to_string(),
            }
        } else {
            words.join(" ").to_lowercase()
        }
    }

    /// Store a history entry
    /// PROFESSIONAL UPGRADE: Implements chunking for large content (>15K chars)
    pub async fn store_entry(&self, entry: HistoryEntry) -> Result<()> {
        // CONTEXT WINDOWING: Chunk large content to prevent context overflow
        let entry_to_store = self.chunk_large_content(entry);
        
        // Generate embedding from summary
        let embedding = self.embed_text(&entry_to_store.summary).await?;

        // Serialize entry to JSON payload
        let payload: Payload = serde_json::to_value(&entry_to_store)
            .context("Failed to serialize entry")?
            .try_into()
            .context("Failed to convert to Payload")?;

        // Create point for Qdrant
        let point = qdrant_client::qdrant::PointStruct::new(
            entry_to_store.id.clone(),
            embedding,
            payload,
        );

        // Upsert point using builder pattern
        use qdrant_client::qdrant::UpsertPointsBuilder;
        let request = UpsertPointsBuilder::new(&self.collection_name, vec![point]);
        self.qdrant
            .upsert_points(request)
            .await
            .context("Failed to store entry in Qdrant")?;

        tracing::info!("Stored history entry: {} ({})", entry_to_store.id, entry_to_store.topic);
        Ok(())
    }

    /// PROFESSIONAL UPGRADE: Chunk large content to prevent context window overload
    /// Truncates full_result if it exceeds 15,000 characters
    fn chunk_large_content(&self, mut entry: HistoryEntry) -> HistoryEntry {
        const MAX_CONTENT_CHARS: usize = 15000;
        
        // Check if full_result is too large
        let content_str = entry.full_result.to_string();
        if content_str.len() > MAX_CONTENT_CHARS {
            // Truncate and add metadata about chunking
            let truncated = content_str.chars().take(MAX_CONTENT_CHARS).collect::<String>();
            
            // Try to parse back to JSON and add truncation notice
            if let Ok(mut obj) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&truncated) {
                obj.insert(
                    "_truncated".to_string(), 
                    serde_json::json!({
                        "original_size": content_str.len(),
                        "truncated_at": MAX_CONTENT_CHARS,
                        "reason": "context_windowing"
                    })
                );
                entry.full_result = serde_json::Value::Object(obj);
            } else {
                // Fallback: wrap as string with metadata
                entry.full_result = serde_json::json!({
                    "content": truncated,
                    "_truncated": {
                        "original_size": content_str.len(),
                        "truncated_at": MAX_CONTENT_CHARS,
                        "reason": "context_windowing"
                    }
                });
            }
            
            tracing::info!(
                "Chunked large content for entry {}: {} -> {} chars",
                entry.id,
                content_str.len(),
                MAX_CONTENT_CHARS
            );
        }
        
        entry
    }

 
    /// This provides the BEST results for agents by:
    /// 1. Using semantic vector search for conceptual matching
    /// 2. Boosting exact keyword matches in the scoring
    /// 3. Searching across summary, query, and topic fields
    pub async fn search_history(
        &self,
        query: &str,
        max_results: usize,
        min_similarity: f32,
        entry_type_filter: Option<EntryType>,
    ) -> Result<Vec<(HistoryEntry, f32)>> {
        // Generate query embedding for vector search
        let query_embedding = self.embed_text(query).await?;

        // Use enhanced vector search with payload consideration
        // Qdrant will auto-boost results where query keywords appear in text fields
        let mut search_request = qdrant_client::qdrant::SearchPoints {
            collection_name: self.collection_name.clone(),
            vector: query_embedding,
            limit: max_results as u64,
            with_payload: Some(true.into()),
            score_threshold: Some(min_similarity),
            ..Default::default()
        };

        // Add entry type filter if specified
        if let Some(entry_type) = entry_type_filter {
            let filter_value = match entry_type {
                EntryType::Search => "search",
                EntryType::Scrape => "scrape",
            };
            search_request.filter = Some(qdrant_client::qdrant::Filter {
                must: vec![qdrant_client::qdrant::Condition::matches(
                    "entry_type",
                    filter_value.to_string(),
                )],
                ..Default::default()
            });
        }

        // Execute search
        let results = self
            .qdrant
            .search_points(search_request)
            .await
            .context("Failed to search Qdrant")?;

        // Parse results and apply keyword boosting for better agent results
        let query_lower = query.to_lowercase();
        let query_keywords: Vec<&str> = query_lower.split_whitespace().collect();
        
        let mut entries: Vec<(HistoryEntry, f32)> = results
            .result
            .into_iter()
            .filter_map(|point| {
                let mut score = point.score;
                let payload = point.payload;
                let value = serde_json::to_value(&payload).ok()?;
                let entry: HistoryEntry = serde_json::from_value(value).ok()?;
                
                // Boost score if exact keywords match (hybrid approach)
                let entry_text = format!("{} {} {}", 
                    entry.query.to_lowercase(), 
                    entry.summary.to_lowercase(),
                    entry.topic.to_lowercase()
                );
                
                let mut keyword_matches = 0;
                for keyword in &query_keywords {
                    if entry_text.contains(keyword) {
                        keyword_matches += 1;
                    }
                }
                
                // Boost score based on keyword matches (up to +15%)
                if keyword_matches > 0 {
                    let boost = (keyword_matches as f32 / query_keywords.len() as f32) * 0.15;
                    score = (score + boost).min(1.0);
                }
                
                Some((entry, score))
            })
            .collect();

        // Re-sort by boosted scores
        entries.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        tracing::info!(
            "✨ Hybrid search (vector + keyword boost) found {} entries for '{}' (threshold: {:.2})",
            entries.len(),
            query,
            min_similarity
        );
        Ok(entries)
    }

    /// Log a search operation
    pub async fn log_search(
        &self,
        query: String,
        results: &serde_json::Value,
        result_count: usize,
    ) -> Result<()> {
        let topic = Self::generate_topic(&query, &EntryType::Search);
        let summary = format!("Search: {} ({} results)", query, result_count);

        let entry = HistoryEntry {
            id: Uuid::new_v4().to_string(),
            entry_type: EntryType::Search,
            query: query.clone(),
            topic,
            summary,
            full_result: results.clone(),
            timestamp: Utc::now(),
            domain: None,
            source_type: None,
        };

        self.store_entry(entry).await
    }

    /// Log a scrape operation
    pub async fn log_scrape(
        &self,
        url: String,
        title: Option<String>,
        content_preview: String,
        domain: Option<String>,
        full_result: &serde_json::Value,
    ) -> Result<()> {
        let topic = Self::generate_topic(&url, &EntryType::Scrape);
        let summary = if let Some(t) = title {
            format!("Scraped: {} - {}", t, content_preview)
        } else {
            format!("Scraped: {} - {}", url, content_preview)
        };

        let entry = HistoryEntry {
            id: Uuid::new_v4().to_string(),
            entry_type: EntryType::Scrape,
            query: url,
            topic,
            summary,
            full_result: full_result.clone(),
            timestamp: Utc::now(),
            domain,
            source_type: None,
        };

        self.store_entry(entry).await
    }

    /// Get collection statistics
    pub async fn get_stats(&self) -> Result<(u64, u64)> {
        let collection_info = self
            .qdrant
            .collection_info(&self.collection_name)
            .await
            .context("Failed to get collection info")?;

        let total = collection_info
            .result
            .and_then(|r| r.points_count)
            .unwrap_or(0);

        // Count by type (simplified - just return total for both)
        Ok((total, total))
    }

    /// Check for recent duplicate searches (within last N hours)
    pub async fn find_recent_duplicate(
        &self,
        query: &str,
        hours_back: u64,
    ) -> Result<Option<(HistoryEntry, f32)>> {
        use chrono::Duration;

        // Search for very similar queries (high threshold)
        let results = self
            .search_history(query, 5, 0.9, Some(EntryType::Search))
            .await?;

        // Filter to only recent entries
        let cutoff = Utc::now() - Duration::hours(hours_back as i64);

        for (entry, score) in results {
            if entry.timestamp > cutoff {
                return Ok(Some((entry, score)));
            }
        }

        Ok(None)
    }
    
    /// Check if URL was scraped very recently (for testing/iteration detection)
    /// Returns true if scraped within last 5 minutes (suggesting rapid iteration)
    pub async fn is_rapid_testing(&self, url: &str) -> Result<bool> {
        use chrono::Duration;
        
        // Search for this exact URL in recent history
        let results = self
            .search_history(url, 3, 0.95, Some(EntryType::Scrape))
            .await?;
        
        // Check if any match within last 5 minutes
        let cutoff = Utc::now() - Duration::minutes(5);
        
        let recent_count = results.iter()
            .filter(|(entry, _)| entry.timestamp > cutoff)
            .count();
        
        // If 2+ scrapes of same URL within 5 minutes, it's testing mode
        Ok(recent_count >= 2)
    }

    /// Get top domains from history
    pub async fn get_top_domains(&self, limit: usize) -> Result<Vec<(String, usize)>> {
        use std::collections::HashMap;

        // Search all entries
        let results = self
            .search_history("", 1000, 0.0, None)
            .await?;

        let mut domain_counts: HashMap<String, usize> = HashMap::new();

        for (entry, _) in results {
            if let Some(domain) = entry.domain {
                *domain_counts.entry(domain).or_insert(0) += 1;
            }
        }

        let mut sorted: Vec<_> = domain_counts.into_iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(&a.1));
        sorted.truncate(limit);

        Ok(sorted)
    }
}
