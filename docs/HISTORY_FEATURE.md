# Research History Feature (Semantic Memory)

## Overview

ShadowCrawl's Research History feature provides **100% open-source semantic memory** for the MCP server. It automatically logs searches and scrapes so agents can:

- 🔍 Semantic search past work with natural language
- 🔄 Keep context across sessions
- 🚫 Detect duplicates (recent searches / rapid scrape iteration)
- 📊 Compute basic analytics (e.g., top domains)

This feature is **optional** and enabled only when you set `LANCEDB_URI`.

---

## Architecture

### Technology Stack

1. **LanceDB** (embedded, in-process)
   - Vector DB runs inside the ShadowCrawl process (no separate DB container)
   - Persists data on disk at `LANCEDB_URI`

2. **Model2Vec** (Rust inference via `model2vec-rs` / `model2vec_rs`)
   - Local embedding generation (no external embedding APIs)
   - Model loaded from a local path or HuggingFace model ID via `MODEL2VEC_MODEL`
   - Embedding dimension depends on the selected model

Rust implementation: https://github.com/MinishLab/model2vec-rs

3. **Integration Points**
   - `mcp-server/src/features/history.rs`: memory logic (store/search/stats)
   - `mcp-server/src/tools/search.rs` / `scrape.rs`: auto-logging
   - MCP tool: `research_history`

---

## Setup

### 1) Choose a storage location

Pick a directory that should persist across runs (example):

```bash
mkdir -p ./lancedb
```

### 2) Enable memory

Set `LANCEDB_URI` (required) and optionally `MODEL2VEC_MODEL`.

```bash
export LANCEDB_URI=./lancedb

# Optional: HF model id or local path to a Model2Vec model directory
# export MODEL2VEC_MODEL=minishlab/potion-base-8M

./mcp-server/target/release/shadowcrawl-mcp
```

### 3) Verify

On startup with memory enabled you should see logs similar to:

```
Initializing memory with LanceDB at: ...
Loading Model2Vec model: ...
Memory initialized successfully
```

---

## Usage

### Auto-Logging (Automatic)

When enabled, ShadowCrawl automatically logs:
- Searches (query + summary + full JSON result)
- Scrapes (URL + preview/summary + full JSON result)

### Manual Search (MCP tool: `research_history`)

Example:

```json
{
  "query": "web scraping tutorials",
  "limit": 10,
  "threshold": 0.75
}
```

Parameters:
- `query` (required)
- `limit` (optional, default: 10)
- `threshold` (optional, default: 0.7)

---

## Data Model

### Stored fields

Each row stores:
- `HistoryEntry` fields (id, type, query/url, topic, summary, timestamp, domain, source_type)
- `full_result` (JSON serialized as a string)
- `vector` (embedding generated from `summary`)

### Similarity + “hybrid” behavior

Search is:
1) Vector search over `vector` using cosine distance
2) Keyword boosting in the application layer (same intent as previous hybrid approach)

---

## Troubleshooting

- **Memory disabled**: ensure `LANCEDB_URI` is set.
- **Model load fails**: verify `MODEL2VEC_MODEL` points to a valid HF model id or local model directory.
- **Slow first run**: the first model load / download (if using HF id) can take time; subsequent runs reuse cached artifacts (respecting `HF_HOME` if set).
