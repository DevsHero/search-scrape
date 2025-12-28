#!/bin/bash
# Integration test script for Phase 1 & Phase 2 features
# Tests history, query rewriting, and duplicate detection

set -e

BINARY="/Users/hero/Documents/GitHub/search-scrape/mcp-server/target/release/search-scrape-mcp"
QDRANT_URL="http://localhost:6333"
SEARXNG_URL="http://localhost:8888"

echo "=================================================="
echo "Integration Tests for Search-Scrape MCP Server"
echo "=================================================="
echo ""

# Check if binary exists
if [ ! -f "$BINARY" ]; then
    echo "❌ Binary not found at $BINARY"
    exit 1
fi

echo "✅ Binary found: $BINARY"

# Check services
echo ""
echo "Checking services..."
if curl -s "$QDRANT_URL/health" > /dev/null; then
    echo "✅ Qdrant is running at $QDRANT_URL"
else
    echo "❌ Qdrant is not accessible at $QDRANT_URL"
    exit 1
fi

if curl -s "$SEARXNG_URL" > /dev/null; then
    echo "✅ SearXNG is running at $SEARXNG_URL"
else
    echo "❌ SearXNG is not accessible at $SEARXNG_URL"
    exit 1
fi

echo ""
echo "=================================================="
echo "TEST 1: Basic Web Search"
echo "=================================================="

TEST1=$(cat <<'EOF'
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "search_web",
    "arguments": {
      "query": "rust programming",
      "max_results": 5
    }
  }
}
EOF
)

echo "$TEST1" | QDRANT_URL="$QDRANT_URL" "$BINARY" 2>/dev/null | jq -r 'select(.result != null) | .result.content[0].text' | head -20

echo ""
echo "=================================================="
echo "TEST 2: Developer Query with Rewriting"
echo "=================================================="

TEST2=$(cat <<'EOF'
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/call",
  "params": {
    "name": "search_web",
    "arguments": {
      "query": "rust docs async",
      "max_results": 5
    }
  }
}
EOF
)

echo "$TEST2" | QDRANT_URL="$QDRANT_URL" "$BINARY" 2>/dev/null | jq -r 'select(.result != null) | .result.content[0].text' | head -30

echo ""
echo "=================================================="
echo "TEST 3: Scrape URL"
echo "=================================================="

TEST3=$(cat <<'EOF'
{
  "jsonrpc": "2.0",
  "id": 3,
  "method": "tools/call",
  "params": {
    "name": "scrape_url",
    "arguments": {
      "url": "https://doc.rust-lang.org/book/ch01-00-getting-started.html",
      "max_chars": 500
    }
  }
}
EOF
)

echo "$TEST3" | QDRANT_URL="$QDRANT_URL" "$BINARY" 2>/dev/null | jq -r 'select(.result != null) | .result.content[0].text' | head -20

echo ""
echo "=================================================="
echo "TEST 4: Research History (after previous searches)"
echo "=================================================="

TEST4=$(cat <<'EOF'
{
  "jsonrpc": "2.0",
  "id": 4,
  "method": "tools/call",
  "params": {
    "name": "research_history",
    "arguments": {
      "query": "rust programming",
      "max_results": 3
    }
  }
}
EOF
)

echo "$TEST4" | QDRANT_URL="$QDRANT_URL" "$BINARY" 2>/dev/null | jq -r 'select(.result != null) | .result.content[0].text' | head -30

echo ""
echo "=================================================="
echo "TEST 5: Duplicate Detection (same query twice)"
echo "=================================================="

# First query
TEST5A=$(cat <<'EOF'
{
  "jsonrpc": "2.0",
  "id": 5,
  "method": "tools/call",
  "params": {
    "name": "search_web",
    "arguments": {
      "query": "python tutorial",
      "max_results": 3
    }
  }
}
EOF
)

echo "First search for 'python tutorial'..."
echo "$TEST5A" | QDRANT_URL="$QDRANT_URL" "$BINARY" 2>/dev/null | jq -r 'select(.result != null) | .result.content[0].text' | head -15

echo ""
echo "Immediate duplicate search for 'python tutorial'..."

# Duplicate query (should trigger warning)
TEST5B=$(cat <<'EOF'
{
  "jsonrpc": "2.0",
  "id": 6,
  "method": "tools/call",
  "params": {
    "name": "search_web",
    "arguments": {
      "query": "python tutorial",
      "max_results": 3
    }
  }
}
EOF
)

echo "$TEST5B" | QDRANT_URL="$QDRANT_URL" "$BINARY" 2>/dev/null | jq -r 'select(.result != null) | .result.content[0].text' | head -20

echo ""
echo "=================================================="
echo "TEST 6: JSON Output Format"
echo "=================================================="

TEST6=$(cat <<'EOF'
{
  "jsonrpc": "2.0",
  "id": 7,
  "method": "tools/call",
  "params": {
    "name": "scrape_url",
    "arguments": {
      "url": "https://example.com",
      "output_format": "json",
      "max_chars": 1000
    }
  }
}
EOF
)

echo "$TEST6" | QDRANT_URL="$QDRANT_URL" "$BINARY" 2>/dev/null | jq -r 'select(.result != null) | .result.content[0].text' | head -25

echo ""
echo "=================================================="
echo "All tests completed!"
echo "=================================================="
