# VS Code MCP Configuration - All 3 Tools Enabled

## ✅ Confirmation: All 3 Tools Are Compiled

Binary contains:
- ✅ `search_web`
- ✅ `scrape_url`  
- ✅ `research_history` (NEW - Phase 1 feature)

## VS Code Setup Instructions

### Step 1: Update VS Code Settings

**Location:** `.vscode/settings.json` in your project root

```json
{
  "mcp.servers": {
    "search-scrape": {
      "command": "/Users/hero/Documents/GitHub/search-scrape/search-scrape/target/release/search-scrape-mcp",
      "args": [],
      "env": {
        "QDRANT_URL": "http://localhost:6334",
        "SEARXNG_URL": "http://localhost:8888",
        "RUST_LOG": "info"
      }
    }
  }
}
```



**IMPORTANT:** VS Code caches MCP tool lists. You MUST:

1. **Quit VS Code completely** (Cmd+Q on Mac)
2. **Start VS Code again**
3. Wait for MCP server to initialize (~2-3 seconds)

### Step 3: Verify Tools Are Loaded

You should now see **3 tools** in the MCP panel:

1. 🔍 **search_web** - Web search with SearXNG
2. 📄 **scrape_url** - Content extraction  
3. 🧠 **research_history** - Semantic history search (NEW!)

## Alternative: Claude Desktop Config

If using Claude Desktop instead of VS Code:

**Location:** `~/Library/Application Support/Claude/claude_desktop_config.json`

```json
{
  "mcpServers": {
    "search-scrape": {
      "command": "/Users/hero/Documents/GitHub/search-scrape/search-scrape/target/release/search-scrape-mcp",
      "env": {
        "QDRANT_URL": "http://localhost:6334",
        "SEARXNG_URL": "http://localhost:8888"
      }
    }
  }
}
```

Then **restart Claude Desktop**.

## Testing All 3 Tools

### Test 1: search_web
```
Search for "rust async programming" and show top 3 results
```

### Test 2: scrape_url
```
Scrape https://doc.rust-lang.org/book/ and extract code blocks
```

### Test 3: research_history (NEW!)
```
Search my history for "programming tutorials"
```

## Troubleshooting

### "research_history not showing"

**Cause:** VS Code is using an old cached version

**Fix:**
1. Kill all VS Code processes: `pkill -9 "Visual Studio Code"`
2. Delete MCP cache (if exists): `rm -rf ~/.vscode/mcp-cache/`
3. Restart VS Code
4. Verify binary is correct: `ls -lh search-scrape/target/release/search-scrape-mcp`

### "Cannot connect to Qdrant"

**Cause:** Qdrant not running or wrong port

**Fix:**
```bash
# Check Qdrant is running on gRPC port 6334
docker ps | grep qdrant

# Verify port
curl http://localhost:6333/  # HTTP port (should work)
# MCP uses gRPC port 6334
```

**Note:** The binary uses **port 6334** (gRPC), not 6333 (HTTP)!

### "research_history returns 'not available'"

**Cause:** QDRANT_URL not set or Qdrant not running

**Fix:**
1. Ensure Qdrant is running: `docker-compose up -d qdrant`
2. Verify environment variable is set in config
3. Restart MCP server (quit and reopen VS Code)

## Real Usage Example (All Features)

Once configured, ask the AI assistant:

```
1. Search for "rust web scraping libraries" (uses search_web + query rewriting)

2. Scrape the top result and extract code examples (uses scrape_url + code extraction)

3. Check if I've researched this before (uses research_history + semantic search)

4. Search again for "rust scraping" (duplicate detection should trigger)
```

The AI should use all 3 tools automatically and show:
- ✅ Query rewriting (Phase 2)
- ✅ Code extraction (Priority 1)
- ✅ History search (Phase 1)
- ✅ Duplicate warnings (Phase 2)

## Binary Information

```
Path: /Users/hero/Documents/GitHub/search-scrape/search-scrape/target/release/search-scrape-mcp
Size: ~37MB
Built: December 28, 2025
Features: All Phase 1, Phase 2, Priority 1 & 2
```

## Verification Command

```bash
# Verify all 3 tools are in binary
strings search-scrape/target/release/search-scrape-mcp | grep -c "research_history"
# Should output: >0 (tool is compiled in)
```

---

**After following these steps, you should see ALL 3 tools in VS Code!** 🎉
