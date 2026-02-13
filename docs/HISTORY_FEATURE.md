# Research History Feature Documentation

## Overview

The Research History feature provides **100% open-source semantic memory** for the ShadowCrawl MCP server. It automatically tracks all searches and scrapes, enabling:

- 🔍 **Semantic Search**: Find related research using natural language queries
- 🔄 **Context Continuity**: Remember research across sessions
- 🚫 **Duplicate Prevention**: Check history before repeating work
- 📊 **Research Analytics**: Track what you've explored

## Architecture

### Technology Stack

1. **Qdrant Vector Database** (`v1.11+`)
   - Open-source, Rust-native vector database
   - High performance with cosine similarity search
   - Persistent storage via Docker volumes
   - HTTP API on port 6333

2. **fastembed** (`v4.0`)
   - 100% local embedding model (no external APIs)
   - Model: `AllMiniLML6V2` (384 dimensions, ~23MB)
   - Fast inference with Rust/ONNX backend
   - Auto-downloads model on first use

3. **Integration Points**
   - `history.rs`: Core memory management logic
   - `search.rs`: Auto-logs all search operations
   - `scrape.rs`: Auto-logs all scrape operations
   - `stdio_service.rs`: `research_history` MCP tool

## Setup

### 1. Start Qdrant

```bash
# Using docker-compose (recommended)
docker-compose up qdrant -d

# Or run Qdrant directly
docker run -d \
  -p 6333:6333 \
  -p 6334:6334 \
  -v $(pwd)/qdrant_storage:/qdrant/storage \
  --name qdrant \
  qdrant/qdrant:latest
```

### 2. Enable History Feature

Set the `QDRANT_URL` environment variable:

```bash
# For MCP stdio server
SEARXNG_URL=http://localhost:8888 \
QDRANT_URL=http://localhost:6333 \
./target/release/shadowcrawl-mcp

# For HTTP server
SEARXNG_URL=http://localhost:8888 \
QDRANT_URL=http://localhost:6333 \
./target/release/shadowcrawl
```

### 3. Verify Setup

On first run with `QDRANT_URL` set, you'll see:

```
INFO Initializing memory with Qdrant at: http://localhost:6333
INFO Initializing collection: research_history
INFO Initializing fastembed model...
INFO Memory initialized successfully
```

The `research_history` collection is created automatically.

## Usage

### Auto-Logging (Automatic)

All searches and scrapes are **automatically logged** when the history feature is enabled:

**Search Example:**
```json
{
  "query": "rust async programming"
}
```
Logs:
- Query text
- Number of results
- Result data (as JSON)
- Timestamp

**Scrape Example:**
```json
{
  "url": "https://docs.rs/tokio"
}
```
Logs:
- URL
- Page title
- Word count & code blocks
- Domain
- Full scraped data

### Manual Search (research_history Tool)

Search your history using natural language:

```json
{
  "query": "web scraping tutorials",
  "limit": 10,
  "threshold": 0.75
}
```

**Parameters:**
- `query` (required): Natural language search query
- `limit` (optional, default: 10): Max results to return (1-50)
- `threshold` (optional, default: 0.7): Similarity threshold (0.0-1.0)

**Threshold Guidelines:**
- `0.6-0.7`: Broad topic search (finds related concepts)
- `0.75-0.85`: Specific queries (good balance)
- `0.85-0.95`: Very specific (near-exact matches)

### Output Format

```
Found 3 relevant entries for 'web scraping tutorials':

1. [Similarity: 0.89] Web Scraping in Rust (docs.rs)
   Type: Scrape
   When: 2024-12-01 14:30 UTC
   Summary: Introduction to Web Scraping - 2500 words, 8 code blocks
   Query: https://docs.rs/scraper/latest/scraper/

2. [Similarity: 0.82] scraping best practices (stackoverflow.com)
   Type: Search
   When: 2024-12-01 10:15 UTC
   Summary: Found 15 results. Top domains: stackoverflow.com, github.com
   Query: best practices for web scraping

💡 Tip: Use threshold=0.75 for similar results, or higher (0.8-0.9) for more specific matches
```

## Data Model

### HistoryEntry Structure

```rust
pub struct HistoryEntry {
    pub id: String,              // UUID
    pub entry_type: EntryType,   // Search | Scrape
    pub query: String,           // Search query or URL
    pub topic: String,           // Generated topic/title
    pub summary: String,         // Human-readable summary
    pub full_result: serde_json::Value,  // Complete data
    pub timestamp: DateTime<Utc>,
    pub domain: Option<String>,  // Source domain (scrapes only)
    pub source_type: Option<String>,  // Classification
}
```

### Vector Storage

- **Collection**: `research_history`
- **Vector Size**: 384 dimensions
- **Distance Metric**: Cosine similarity
- **Indexed Fields**: All HistoryEntry fields stored as payload
- **Embedding Source**: `summary` field (human-readable text)

## Technical Details

### Embedding Process

1. **Text Extraction**: Use the `summary` field (concise, informative)
2. **Model Loading**: Lazy-load AllMiniLML6V2 on first embed
3. **Embedding**: Convert text → 384-dim vector
4. **Storage**: Store vector + full entry as Qdrant point

### Search Process

1. **Query Embedding**: Convert search query → vector
2. **Vector Search**: Qdrant cosine similarity search
3. **Filtering**: Apply threshold, limit, type filters
4. **Results**: Return entries sorted by similarity score

### Collection Management

**Auto-initialization:**
```rust
// On startup if QDRANT_URL is set:
- Connect to Qdrant
- Check if collection exists
- If not: Create with 384-dim vectors, cosine distance
- Load fastembed model
```

**Collection Schema:**
```json
{
  "name": "research_history",
  "vectors": {
    "size": 384,
    "distance": "Cosine"
  }
}
```

## Performance

### Storage Requirements

- **Per Entry**: ~1-5 KB (entry metadata) + 1.5 KB (vector)
- **1000 entries**: ~3-7 MB
- **10,000 entries**: ~30-70 MB

### Speed Benchmarks

- **Embedding Generation**: ~5-20ms per text
- **Vector Search**: <10ms for 10K entries
- **Auto-logging Overhead**: <50ms per search/scrape

### Optimization Tips

1. **Threshold Tuning**: Higher threshold = fewer, more relevant results
2. **Limit Control**: Use smaller limits (5-10) for quick checks
3. **Periodic Cleanup**: Archive old entries if needed (manual)

## Troubleshooting

### "Memory feature is not available"

**Cause**: `QDRANT_URL` not set or Qdrant not running

**Fix:**
```bash
# Check Qdrant is running
curl http://localhost:6333/collections

# If not, start it
docker-compose up qdrant -d

# Set environment variable
export QDRANT_URL=http://localhost:6333
```

### "Failed to initialize memory"

**Cause**: Cannot connect to Qdrant at specified URL

**Fix:**
```bash
# Verify Qdrant is accessible
docker ps | grep qdrant

# Check logs
docker logs qdrant

# Ensure port 6333 is not blocked
netstat -an | grep 6333
```

### "Failed to log to history"

**Cause**: Qdrant connection lost or embedding failed

**Impact**: Non-fatal - search/scrape continues without logging

**Fix:**
```bash
# Check Qdrant health
curl http://localhost:6333/

# Restart if needed
docker-compose restart qdrant
```

### Model Download Issues

**Symptom**: "Failed to initialize embedding model"

**Cause**: First-time model download failed

**Fix:**
```bash
# Check internet connection
# Model auto-downloads from HuggingFace

# Or manually download to cache:
# ~/.cache/fastembed/sentence-transformers__all-MiniLM-L6-v2/
```

## Privacy & Security

✅ **100% Local**
- All embeddings generated on your machine
- No external API calls
- No telemetry or tracking

✅ **Data Ownership**
- All data stored in local Qdrant volume
- Full control over data retention
- Easy to backup/export via Qdrant API

✅ **Optional Feature**
- History is disabled by default
- Enable only when needed
- No data collected if disabled

## Future Enhancements (Phase 2)

Planned improvements:
1. **Smart Query Rewriting**: Use history to improve search queries
2. **Duplicate Detection**: Warn before repeating recent searches
3. **Analytics Dashboard**: Visualize research patterns
4. **Export/Import**: Backup and restore history
5. **TTL/Archival**: Auto-cleanup old entries
6. **Multi-collection**: Separate histories per project

## API Reference

### MemoryManager Methods

```rust
// Create new manager (auto-initializes collection)
pub async fn new(qdrant_url: &str) -> Result<Self>

// Search history by similarity
pub async fn search_history(
    &self,
    query: &str,
    max_results: usize,
    min_similarity: f32,
    entry_type_filter: Option<EntryType>,
) -> Result<Vec<(HistoryEntry, f32)>>

// Log a search operation
pub async fn log_search(
    &self,
    query: String,
    results: &serde_json::Value,
    result_count: usize,
) -> Result<()>

// Log a scrape operation
pub async fn log_scrape(
    &self,
    url: String,
    title: Option<String>,
    content_preview: String,
    domain: Option<String>,
    full_result: &serde_json::Value,
) -> Result<()>
```

## Dependencies

```toml
[dependencies]
qdrant-client = { version = "1.11", features = ["serde"] }
fastembed = "4.0"
```

**Size Impact:**
- Binary size increase: ~2 MB
- Runtime memory: ~50 MB (model loaded)
- Disk space: ~23 MB (model cache) + Qdrant storage

## Contributing

To extend the history feature:

1. **Add New Entry Types**: Extend `EntryType` enum in `history.rs`
2. **Custom Embeddings**: Swap `AllMiniLML6V2` for other fastembed models
3. **Advanced Filters**: Add metadata filters to `search_history()`
4. **Custom Scoring**: Modify similarity calculations in Qdrant queries

## License

Same as main project. See LICENSE file.
