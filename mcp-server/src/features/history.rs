use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use model2vec_rs::model::StaticModel;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::OnceCell;
use uuid::Uuid;

use arrow_array::Array;
use arrow_array::RecordBatchIterator;
use arrow_array::{
    types::Float32Type, FixedSizeListArray, Float32Array, Int64Array, RecordBatch, StringArray,
};
use arrow_schema::{DataType, Field, Schema};
use futures::TryStreamExt;
use lancedb::{
    query::{ExecutableQuery, QueryBase},
    Table,
};

/// Entry type for history records
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryType {
    Search,
    Scrape,
}

/// History entry stored in semantic memory
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
    table: Table,
    model_id: String,
    embedding_model: Arc<OnceCell<Arc<StaticModel>>>,
    embedding_dim: usize,
}

impl MemoryManager {
    /// Create a new memory manager
    pub async fn new(lancedb_uri: &str) -> Result<Self> {
        let model_id = std::env::var("MODEL2VEC_MODEL")
            .unwrap_or_else(|_| "minishlab/potion-base-8M".to_string());

        tracing::info!(
            "Initializing semantic memory with LanceDB at: {} (model2vec model: {})",
            lancedb_uri,
            model_id
        );

        // Load Model2Vec eagerly once to determine embedding dimension and validate config.
        let model_id_for_load = model_id.clone();
        let (model, embedding_dim) =
            tokio::task::spawn_blocking(move || -> Result<(StaticModel, usize)> {
                let model = StaticModel::from_pretrained(&model_id_for_load, None, None, None)
                    .with_context(|| {
                        format!(
                            "Failed to load Model2Vec model from '{}'",
                            model_id_for_load
                        )
                    })?;
                let probe = model.encode_single("dimension probe");
                Ok((model, probe.len()))
            })
            .await
            .context("Model2Vec init task failed")??;

        // Connect to LanceDB (in-process)
        let db = lancedb::connect(lancedb_uri)
            .execute()
            .await
            .context("Failed to connect to LanceDB")?;

        let table_name = "research_history";
        let schema = Arc::new(Self::history_schema(embedding_dim)?);

        let table = match db.open_table(table_name).execute().await {
            Ok(table) => table,
            Err(lancedb::Error::TableNotFound { .. }) => {
                tracing::info!(
                    "Creating LanceDB table '{}' (embedding_dim: {})",
                    table_name,
                    embedding_dim
                );
                db.create_empty_table(table_name, schema.clone())
                    .execute()
                    .await
                    .context("Failed to create LanceDB table")?
            }
            Err(e) => return Err(e).context("Failed to open LanceDB table"),
        };

        // Create a vector index if possible (safe to ignore failures; flat search still works)
        if let Err(e) = table
            .create_index(&["vector"], lancedb::index::Index::Auto)
            .execute()
            .await
        {
            tracing::debug!("LanceDB create_index skipped/failed: {}", e);
        }

        let embedding_model = Arc::new(OnceCell::new());
        let _ = embedding_model.set(Arc::new(model));

        Ok(Self {
            table,
            model_id,
            embedding_model,
            embedding_dim,
        })
    }

    fn history_schema(embedding_dim: usize) -> Result<Schema> {
        let vector_len: i32 = embedding_dim
            .try_into()
            .context("Embedding dimension too large")?;

        Ok(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("entry_type", DataType::Utf8, false),
            Field::new("query", DataType::Utf8, false),
            Field::new("topic", DataType::Utf8, false),
            Field::new("summary", DataType::Utf8, false),
            Field::new("full_result", DataType::Utf8, false),
            Field::new("timestamp_ms", DataType::Int64, false),
            Field::new("domain", DataType::Utf8, true),
            Field::new("source_type", DataType::Utf8, true),
            Field::new(
                "vector",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    vector_len,
                ),
                true,
            ),
        ]))
    }

    /// Get or initialize the embedding model
    async fn get_embedding_model(&self) -> Result<Arc<StaticModel>> {
        let model_id = self.model_id.clone();
        let model = self
            .embedding_model
            .get_or_try_init(|| async move {
                tracing::info!("Loading Model2Vec model: {}", model_id);
                tokio::task::spawn_blocking(move || {
                    StaticModel::from_pretrained(&model_id, None, None, None)
                        .map(Arc::new)
                        .with_context(|| {
                            format!("Failed to load Model2Vec model from '{}'", model_id)
                        })
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
        let embedding = tokio::task::spawn_blocking(move || {
            Ok::<_, anyhow::Error>(model.encode_single(&text_owned))
        })
        .await?
        .context("Failed to generate embedding")?;

        if embedding.len() != self.embedding_dim {
            anyhow::bail!(
                "Embedding dimension mismatch: expected {}, got {}",
                self.embedding_dim,
                embedding.len()
            );
        }

        Ok(embedding)
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

        let batch = self.entry_to_record_batch(&entry_to_store, &embedding)?;

        let schema = batch.schema();
        let batches = RecordBatchIterator::new(vec![Ok(batch)].into_iter(), schema);

        self.table
            .add(batches)
            .execute()
            .await
            .context("Failed to store entry in LanceDB")?;

        tracing::info!(
            "Stored history entry: {} ({})",
            entry_to_store.id,
            entry_to_store.topic
        );
        Ok(())
    }

    fn entry_to_record_batch(
        &self,
        entry: &HistoryEntry,
        embedding: &[f32],
    ) -> Result<RecordBatch> {
        let schema = Arc::new(Self::history_schema(self.embedding_dim)?);

        let entry_type_str = match entry.entry_type {
            EntryType::Search => "search",
            EntryType::Scrape => "scrape",
        };

        let vector_len: i32 = self
            .embedding_dim
            .try_into()
            .context("Embedding dimension too large")?;

        let vector = FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
            std::iter::once(Some(embedding.iter().map(|v| Some(*v)).collect::<Vec<_>>())),
            vector_len,
        );

        let batch = RecordBatch::try_new(
            schema,
            vec![
                Arc::new(StringArray::from(vec![entry.id.clone()])),
                Arc::new(StringArray::from(vec![entry_type_str.to_string()])),
                Arc::new(StringArray::from(vec![entry.query.clone()])),
                Arc::new(StringArray::from(vec![entry.topic.clone()])),
                Arc::new(StringArray::from(vec![entry.summary.clone()])),
                Arc::new(StringArray::from(vec![entry.full_result.to_string()])),
                Arc::new(Int64Array::from(vec![entry.timestamp.timestamp_millis()])),
                Arc::new(StringArray::from(vec![entry.domain.clone()])),
                Arc::new(StringArray::from(vec![entry.source_type.clone()])),
                Arc::new(vector),
            ],
        )
        .context("Failed to build Arrow RecordBatch")?;

        Ok(batch)
    }

    /// PROFESSIONAL UPGRADE: Chunk large content to prevent context window overload
    /// Truncates full_result if it exceeds 15,000 characters
    fn chunk_large_content(&self, mut entry: HistoryEntry) -> HistoryEntry {
        const MAX_CONTENT_CHARS: usize = 15000;

        // Check if full_result is too large
        let content_str = entry.full_result.to_string();
        if content_str.len() > MAX_CONTENT_CHARS {
            // Truncate and add metadata about chunking
            let truncated = content_str
                .chars()
                .take(MAX_CONTENT_CHARS)
                .collect::<String>();

            // Try to parse back to JSON and add truncation notice
            if let Ok(mut obj) =
                serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&truncated)
            {
                obj.insert(
                    "_truncated".to_string(),
                    serde_json::json!({
                        "original_size": content_str.len(),
                        "truncated_at": MAX_CONTENT_CHARS,
                        "reason": "context_windowing"
                    }),
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
        // Special case: empty query means "scan" (used by analytics helpers like get_top_domains)
        if query.trim().is_empty() {
            let mut scan = self.table.query().limit(max_results);
            if let Some(entry_type) = entry_type_filter {
                let filter_value = match entry_type {
                    EntryType::Search => "search",
                    EntryType::Scrape => "scrape",
                };
                scan = scan.only_if(format!("entry_type = '{}'", filter_value));
            }

            let stream = scan.execute().await.context("Failed to scan LanceDB")?;
            let batches: Vec<RecordBatch> = stream
                .try_collect()
                .await
                .context("Failed to read scan results")?;
            let mut out = Vec::new();
            for batch in batches {
                out.extend(Self::batches_to_entries(&batch, None)?);
            }
            return Ok(out.into_iter().map(|(e, _)| (e, 0.0)).collect());
        }

        let query_embedding = self.embed_text(query).await?;

        let mut vector_query = self
            .table
            .query()
            .nearest_to(query_embedding.as_slice())
            .context("Failed to build vector query")?
            .distance_type(lancedb::DistanceType::Cosine)
            .limit(max_results);

        if let Some(entry_type) = entry_type_filter {
            let filter_value = match entry_type {
                EntryType::Search => "search",
                EntryType::Scrape => "scrape",
            };
            vector_query = vector_query.only_if(format!("entry_type = '{}'", filter_value));
        }

        let stream = vector_query
            .execute()
            .await
            .context("Failed to search LanceDB")?;
        let batches: Vec<RecordBatch> = stream
            .try_collect()
            .await
            .context("Failed to read search results")?;

        let query_lower = query.to_lowercase();
        let query_keywords: Vec<&str> = query_lower.split_whitespace().collect();

        let mut entries: Vec<(HistoryEntry, f32)> = Vec::new();
        for batch in batches {
            for (entry, mut score) in Self::batches_to_entries(&batch, Some("_distance"))? {
                // Convert distance (smaller=better) into a similarity-like score (larger=better)
                // For cosine distance, similarity ~= 1 - distance
                if score.is_nan() {
                    score = 0.0;
                }

                // Keyword boosting (hybrid approach) to preserve prior behavior
                let entry_text = format!(
                    "{} {} {}",
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

                if !query_keywords.is_empty() && keyword_matches > 0 {
                    let boost = (keyword_matches as f32 / query_keywords.len() as f32) * 0.15;
                    score = (score + boost).min(1.0);
                }

                if score >= min_similarity {
                    entries.push((entry, score));
                }
            }
        }

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
        let total = self
            .table
            .count_rows(None)
            .await
            .context("Failed to get LanceDB row count")?;
        Ok((total as u64, total as u64))
    }

    fn batches_to_entries(
        batch: &RecordBatch,
        distance_column: Option<&str>,
    ) -> Result<Vec<(HistoryEntry, f32)>> {
        let id_col = batch
            .column_by_name("id")
            .context("Missing column: id")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("Invalid type for column: id")?;
        let entry_type_col = batch
            .column_by_name("entry_type")
            .context("Missing column: entry_type")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("Invalid type for column: entry_type")?;
        let query_col = batch
            .column_by_name("query")
            .context("Missing column: query")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("Invalid type for column: query")?;
        let topic_col = batch
            .column_by_name("topic")
            .context("Missing column: topic")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("Invalid type for column: topic")?;
        let summary_col = batch
            .column_by_name("summary")
            .context("Missing column: summary")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("Invalid type for column: summary")?;
        let full_result_col = batch
            .column_by_name("full_result")
            .context("Missing column: full_result")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("Invalid type for column: full_result")?;
        let ts_col = batch
            .column_by_name("timestamp_ms")
            .context("Missing column: timestamp_ms")?
            .as_any()
            .downcast_ref::<Int64Array>()
            .context("Invalid type for column: timestamp_ms")?;
        let domain_col = batch
            .column_by_name("domain")
            .context("Missing column: domain")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("Invalid type for column: domain")?;
        let source_type_col = batch
            .column_by_name("source_type")
            .context("Missing column: source_type")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("Invalid type for column: source_type")?;

        let distance_col: Option<&Float32Array> = distance_column
            .and_then(|name| batch.column_by_name(name))
            .and_then(|arr| arr.as_any().downcast_ref::<Float32Array>());

        let mut out = Vec::with_capacity(batch.num_rows());
        for row in 0..batch.num_rows() {
            let entry_type_raw = entry_type_col.value(row).to_string();
            let entry_type = match entry_type_raw.as_str() {
                "search" => EntryType::Search,
                "scrape" => EntryType::Scrape,
                other => {
                    tracing::debug!("Unknown entry_type '{}', defaulting to search", other);
                    EntryType::Search
                }
            };

            let timestamp_ms = ts_col.value(row);
            let timestamp =
                DateTime::<Utc>::from_timestamp_millis(timestamp_ms).unwrap_or_else(|| Utc::now());

            let full_result_str = full_result_col.value(row);
            let full_result = serde_json::from_str(full_result_str)
                .unwrap_or_else(|_| serde_json::json!({"_raw": full_result_str}));

            let domain = if domain_col.is_null(row) {
                None
            } else {
                Some(domain_col.value(row).to_string())
            };
            let source_type = if source_type_col.is_null(row) {
                None
            } else {
                Some(source_type_col.value(row).to_string())
            };

            let entry = HistoryEntry {
                id: id_col.value(row).to_string(),
                entry_type,
                query: query_col.value(row).to_string(),
                topic: topic_col.value(row).to_string(),
                summary: summary_col.value(row).to_string(),
                full_result,
                timestamp,
                domain,
                source_type,
            };

            let score = if let Some(dist) = distance_col {
                let d = dist.value(row);
                (1.0 - d).clamp(0.0, 1.0)
            } else {
                0.0
            };

            out.push((entry, score));
        }

        Ok(out)
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

        let recent_count = results
            .iter()
            .filter(|(entry, _)| entry.timestamp > cutoff)
            .count();

        // If 2+ scrapes of same URL within 5 minutes, it's testing mode
        Ok(recent_count >= 2)
    }

    /// Get top domains from history
    pub async fn get_top_domains(&self, limit: usize) -> Result<Vec<(String, usize)>> {
        use std::collections::HashMap;

        // Search all entries
        let results = self.search_history("", 1000, 0.0, None).await?;

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
