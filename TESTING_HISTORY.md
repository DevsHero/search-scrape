# Testing the Research History Feature

Quick guide to test the new research history functionality.

## Prerequisites

```bash
# 1. Ensure SearXNG is running
docker ps | grep searxng

# 2. Ensure Qdrant is running
docker ps | grep qdrant

# 3. If not, start them:
docker-compose up -d
```

## Test 1: Basic Auto-Logging

### Step 1: Run server with history enabled

```bash
cd mcp-server
SEARXNG_URL=http://localhost:8888 \
QDRANT_URL=http://localhost:6333 \
RUST_LOG=info \
./target/release/search-scrape-mcp
```

**Expected Output:**
```
INFO Starting MCP Service
INFO SearXNG URL: http://localhost:8888
INFO Initializing memory with Qdrant at: http://localhost:6333
INFO Initializing collection: research_history
INFO Initializing fastembed model...
INFO Memory initialized successfully
INFO MCP stdio server running
```

### Step 2: Perform a search (via your AI assistant)

Use the `search_web` tool with:
```json
{
  "query": "rust programming tutorials",
  "max_results": 5
}
```

**Expected:**
- Search completes normally
- Server logs: `INFO Stored history entry: <uuid> (Search: rust programming tutorials)`

### Step 3: Perform a scrape

Use the `scrape_url` tool with:
```json
{
  "url": "https://doc.rust-lang.org/book/"
}
```

**Expected:**
- Scrape completes normally
- Server logs: `INFO Stored history entry: <uuid> (Scraped: The Rust Programming Language)`

## Test 2: Semantic History Search

### Step 1: Search history with exact match

Use `research_history` tool:
```json
{
  "query": "rust programming",
  "limit": 5,
  "threshold": 0.8
}
```

**Expected Output:**
```
Found 1 relevant entries for 'rust programming':

1. [Similarity: 0.92] rust programming tutorials (google.com)
   Type: Search
   When: 2024-12-28 15:20 UTC
   Summary: Found 5 results. Top domains: doc.rust-lang.org, rust-lang.org
   Query: rust programming tutorials
```

### Step 2: Search with related terms

```json
{
  "query": "async programming in rust",
  "limit": 10,
  "threshold": 0.65
}
```

**Expected:**
- Should find the previous "rust programming" search
- Similarity score around 0.7-0.85
- Shows semantic matching works

### Step 3: Search with unrelated query

```json
{
  "query": "javascript frameworks",
  "limit": 5,
  "threshold": 0.7
}
```

**Expected:**
```
No relevant history found for: 'javascript frameworks'

Try:
- Lower threshold (currently 0.70)
- Broader search terms
- Check if you have any saved history
```

## Test 3: Verify Data Persistence

### Step 1: Stop the server
Press Ctrl+C to stop

### Step 2: Restart the server
```bash
SEARXNG_URL=http://localhost:8888 \
QDRANT_URL=http://localhost:6333 \
./target/release/search-scrape-mcp
```

### Step 3: Search history again
```json
{
  "query": "rust programming",
  "threshold": 0.7
}
```

**Expected:**
- Previous entries still present
- Proves persistence via Qdrant volume

## Test 4: Check Qdrant Directly

### View collection info
```bash
curl http://localhost:6333/collections/research_history
```

**Expected Response:**
```json
{
  "result": {
    "status": "green",
    "vectors_count": 2,
    "indexed_vectors_count": 2,
    "points_count": 2,
    "segments_count": 1,
    ...
  },
  "status": "ok"
}
```

### View stored points
```bash
curl -X POST http://localhost:6333/collections/research_history/points/scroll \
  -H "Content-Type: application/json" \
  -d '{"limit": 10, "with_payload": true, "with_vector": false}'
```

**Expected:**
- Returns stored history entries
- Shows full HistoryEntry payload
- Confirms data is properly stored

## Test 5: Threshold Behavior

### Very high threshold (0.95)
```json
{
  "query": "rust programming tutorials",
  "threshold": 0.95
}
```
**Expected:** Only near-exact matches

### Medium threshold (0.75)
```json
{
  "query": "learning rust",
  "threshold": 0.75
}
```
**Expected:** Similar topics

### Low threshold (0.6)
```json
{
  "query": "programming",
  "threshold": 0.6
}
```
**Expected:** Broad matches (rust, python, etc.)

## Test 6: Without QDRANT_URL

### Step 1: Run server without history
```bash
SEARXNG_URL=http://localhost:8888 \
./target/release/search-scrape-mcp
```

**Expected Log:**
```
INFO Starting MCP Service
INFO QDRANT_URL not set. Memory feature disabled.
```

### Step 2: Try using research_history tool

**Expected Output:**
```
Research history feature is not available. Set QDRANT_URL environment variable to enable.

Example: QDRANT_URL=http://localhost:6333
```

## Test 7: Performance Check

### Measure auto-logging overhead

Run 10 searches in quick succession and check logs:

**Expected:**
- Each search completes in normal time
- Logging happens asynchronously
- No noticeable delay
- Logs show: "Stored history entry" for each

### Check memory usage
```bash
docker stats qdrant --no-stream
```

**Expected:**
- Memory: ~100-200 MB
- CPU: <5% when idle
- Stable over time

## Troubleshooting Tests

### Test: Qdrant not running

1. Stop Qdrant: `docker stop qdrant`
2. Start server with QDRANT_URL set

**Expected:**
```
WARN Failed to initialize memory: <connection error>. Continuing without memory feature.
```

3. Server continues to work normally
4. search_web and scrape_url still function
5. research_history returns "not available" message

### Test: Qdrant stops during operation

1. Start server with history enabled
2. Perform a search (works)
3. Stop Qdrant: `docker stop qdrant`
4. Perform another search

**Expected:**
- Search completes successfully
- Log shows: `WARN Failed to log search to history: <error>`
- Server continues operating

### Test: Invalid QDRANT_URL

```bash
QDRANT_URL=http://invalid:1234 \
./target/release/search-scrape-mcp
```

**Expected:**
```
WARN Failed to initialize memory: Failed to connect to Qdrant
INFO Memory feature disabled
```

## Success Criteria

✅ **Auto-logging works**: Searches and scrapes are stored
✅ **Semantic search works**: Finds related entries
✅ **Persistence works**: Data survives restarts
✅ **Optional feature**: Works without QDRANT_URL
✅ **Non-blocking**: Failures don't stop operations
✅ **Performance**: <50ms overhead, no noticeable delay
✅ **Threshold filtering**: Correctly filters by similarity
✅ **Graceful degradation**: Continues without Qdrant

## Common Issues

### "Failed to initialize embedding model"

**Cause:** First-time model download
**Solution:** Wait 30-60s, check internet connection
**Cache location:** `~/.cache/fastembed/`

### "No relevant history found"

**Cause:** Threshold too high or no matching entries
**Solution:** Lower threshold to 0.6-0.7

### "Vector count is 0"

**Cause:** No searches/scrapes performed yet
**Solution:** Use search_web or scrape_url first

## Clean Up

### Reset history (optional)
```bash
# Delete collection
curl -X DELETE http://localhost:6333/collections/research_history

# Or reset Qdrant completely
docker-compose down qdrant
docker volume rm search-scrape_qdrant_storage
docker-compose up qdrant -d
```

### Stop services
```bash
docker-compose down
```

## Quick Test Script

Save as `test_history.sh`:

```bash
#!/bin/bash

echo "=== Testing Research History Feature ==="

# 1. Check dependencies
echo "1. Checking Qdrant..."
curl -s http://localhost:6333 > /dev/null && echo "✅ Qdrant OK" || echo "❌ Qdrant not running"

echo "2. Checking SearXNG..."
curl -s http://localhost:8888 > /dev/null && echo "✅ SearXNG OK" || echo "❌ SearXNG not running"

# 3. Check collection
echo "3. Checking research_history collection..."
curl -s http://localhost:6333/collections/research_history | grep -q "green" && echo "✅ Collection exists" || echo "⚠️ Collection will be created on first use"

# 4. Count entries
POINTS=$(curl -s http://localhost:6333/collections/research_history | grep -o '"points_count":[0-9]*' | cut -d: -f2)
echo "4. History entries: $POINTS"

echo ""
echo "=== Ready to test! ==="
echo "Run server with: QDRANT_URL=http://localhost:6333 SEARXNG_URL=http://localhost:8888 ./target/release/search-scrape-mcp"
```

Run: `chmod +x test_history.sh && ./test_history.sh`
