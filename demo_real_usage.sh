#!/bin/bash
# Real-world demonstration of all MCP features
# This script shows actual usage with visible output

set -e

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║   SEARCH-SCRAPE MCP SERVER - REAL FEATURE DEMONSTRATION       ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""

# Configuration
BINARY="/Users/hero/Documents/GitHub/search-scrape/mcp-server/target/release/search-scrape-mcp"
export QDRANT_URL="http://localhost:6334"  # gRPC port for qdrant-client
export SEARXNG_URL="http://localhost:8888"

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo "Configuration:"
echo "  • Binary: $BINARY"
echo "  • Qdrant (gRPC): $QDRANT_URL"
echo "  • SearXNG: $SEARXNG_URL"
echo ""

# Helper to send MCP request
send_request() {
    local tool_name="$1"
    local args="$2"
    
    cat <<EOF | "$BINARY" 2>&1 | grep -A 1000 '"result"' | head -100
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "initialize",
  "params": {
    "protocolVersion": "2024-11-05",
    "capabilities": {},
    "clientInfo": {
      "name": "demo-client",
      "version": "1.0.0"
    }
  }
}
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/call",
  "params": {
    "name": "$tool_name",
    "arguments": $args
  }
}
EOF
}

echo "════════════════════════════════════════════════════════════════"
echo "DEMO 1: Basic Web Search (No History)"
echo "════════════════════════════════════════════════════════════════"
echo -e "${BLUE}Searching for: 'rust async programming'${NC}"
echo ""

send_request "search_web" '{
  "query": "rust async programming",
  "max_results": 5
}'

echo ""
echo ""

echo "════════════════════════════════════════════════════════════════"
echo "DEMO 2: Developer Query with Auto-Rewriting (Phase 2 Feature)"
echo "════════════════════════════════════════════════════════════════"
echo -e "${YELLOW}This query should trigger auto-rewrite:${NC}"
echo -e "${BLUE}Query: 'rust docs tokio'${NC}"
echo ""

send_request "search_web" '{
  "query": "rust docs tokio",
  "max_results": 5
}'

echo ""
echo ""

echo "════════════════════════════════════════════════════════════════"
echo "DEMO 3: Scrape URL with Code Extraction (Priority 1 Feature)"
echo "════════════════════════════════════════════════════════════════"
echo -e "${BLUE}Scraping: https://doc.rust-lang.org/book/ch01-01-installation.html${NC}"
echo ""

send_request "scrape_url" '{
  "url": "https://doc.rust-lang.org/book/ch01-01-installation.html",
  "max_chars": 2000
}'

echo ""
echo ""

echo "════════════════════════════════════════════════════════════════"
echo "DEMO 4: JSON Output Format (Priority 1 Feature)"
echo "════════════════════════════════════════════════════════════════"
echo -e "${BLUE}Scraping with JSON output: https://example.com${NC}"
echo ""

send_request "scrape_url" '{
  "url": "https://example.com",
  "output_format": "json",
  "max_chars": 1000
}'

echo ""
echo ""

echo "════════════════════════════════════════════════════════════════"
echo "DEMO 5: Duplicate Detection (Phase 2 Feature)"
echo "════════════════════════════════════════════════════════════════"
echo -e "${YELLOW}Searching same query twice to trigger duplicate warning:${NC}"
echo -e "${BLUE}Query: 'python tutorial'${NC}"
echo ""

echo "First search:"
send_request "search_web" '{
  "query": "python tutorial",
  "max_results": 3
}'

echo ""
echo "Second search (should show duplicate warning):"
send_request "search_web" '{
  "query": "python tutorial",
  "max_results": 3
}'

echo ""
echo ""

echo "════════════════════════════════════════════════════════════════"
echo "DEMO 6: Research History Search (Phase 1 Feature)"
echo "════════════════════════════════════════════════════════════════"
echo -e "${YELLOW}Searching history for semantic matches:${NC}"
echo -e "${BLUE}Query: 'programming tutorials'${NC}"
echo ""

send_request "research_history" '{
  "query": "programming tutorials",
  "limit": 5,
  "threshold": 0.6
}'

echo ""
echo ""

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║                   DEMONSTRATION COMPLETE                       ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""
echo "Features Demonstrated:"
echo "  ✓ Basic web search"
echo "  ✓ Query rewriting (Phase 2)"
echo "  ✓ Code extraction (Priority 1)"
echo "  ✓ JSON output (Priority 1)"
echo "  ✓ Duplicate detection (Phase 2)"
echo "  ✓ History search (Phase 1)"
echo ""
