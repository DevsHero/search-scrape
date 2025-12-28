#!/usr/bin/env python3
"""
Real-world demonstration of Search-Scrape MCP features
Shows actual output from each feature
"""

import json
import subprocess
import sys
import os
from datetime import datetime

# Configuration
BINARY = "/Users/hero/Documents/GitHub/search-scrape/mcp-server/target/release/search-scrape-mcp"
QDRANT_URL = "http://localhost:6334"  # gRPC port
SEARXNG_URL = "http://localhost:8888"

# Colors
class Color:
    BLUE = '\033[94m'
    GREEN = '\033[92m'
    YELLOW = '\033[93m'
    RED = '\033[91m'
    BOLD = '\033[1m'
    END = '\033[0m'

def print_header(title):
    print(f"\n{'='*70}")
    print(f"{Color.BOLD}{title}{Color.END}")
    print(f"{'='*70}\n")

def print_subheader(text):
    print(f"{Color.BLUE}{text}{Color.END}")

def send_mcp_request(tool_name, arguments):
    """Send request to MCP server via stdio"""
    
    # Prepare messages
    initialize_msg = {
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
    
    initialized_notification = {
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    }
    
    call_msg = {
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": tool_name,
            "arguments": arguments
        }
    }
    
    # Combine messages with initialized notification
    input_data = json.dumps(initialize_msg) + "\n" + json.dumps(initialized_notification) + "\n" + json.dumps(call_msg) + "\n"
    
    # Set environment
    env = os.environ.copy()
    env['QDRANT_URL'] = QDRANT_URL
    env['SEARXNG_URL'] = SEARXNG_URL
    env['RUST_LOG'] = 'info'
    
    # Run process
    try:
        result = subprocess.run(
            [BINARY],
            input=input_data,
            capture_output=True,
            text=True,
            env=env,
            timeout=30
        )
        
        # Parse output - look for the tool call result
        lines = result.stdout.strip().split('\n')
        for line in lines:
            if line.strip():
                try:
                    msg = json.loads(line)
                    if msg.get('id') == 2 and 'result' in msg:
                        return msg['result']
                except json.JSONDecodeError:
                    continue
        
        # If we got here, show what we got
        print(f"{Color.RED}Could not parse response. Raw output:{Color.END}")
        print(result.stdout)
        if result.stderr:
            print(f"\n{Color.RED}Stderr:{Color.END}")
            print(result.stderr)
        return None
        
    except subprocess.TimeoutExpired:
        print(f"{Color.RED}Request timed out{Color.END}")
        return None
    except Exception as e:
        print(f"{Color.RED}Error: {e}{Color.END}")
        return None

def demo_basic_search():
    print_header("DEMO 1: Basic Web Search")
    print_subheader("Searching for: 'rust async programming'")
    
    result = send_mcp_request("search_web", {
        "query": "rust async programming",
        "max_results": 5
    })
    
    if result and 'content' in result:
        text = result['content'][0]['text']
        print(text[:1500])  # Show first 1500 chars
        if len(text) > 1500:
            print(f"\n... (truncated, {len(text)} total chars)")
    else:
        print(f"{Color.RED}No result{Color.END}")

def demo_query_rewriting():
    print_header("DEMO 2: Developer Query Auto-Rewriting (Phase 2)")
    print_subheader("Query: 'rust docs tokio' (should trigger site:docs.rs rewrite)")
    
    result = send_mcp_request("search_web", {
        "query": "rust docs tokio",
        "max_results": 3
    })
    
    if result and 'content' in result:
        text = result['content'][0]['text']
        # Look for rewrite notification
        if "Enhanced query" in text or "site:" in text:
            print(f"{Color.GREEN}✓ Query rewriting detected!{Color.END}\n")
        print(text[:1200])
        if len(text) > 1200:
            print(f"\n... (truncated)")
    else:
        print(f"{Color.RED}No result{Color.END}")

def demo_scrape_with_code():
    print_header("DEMO 3: Scrape URL with Code Extraction (Priority 1)")
    print_subheader("URL: https://doc.rust-lang.org/book/ch01-01-installation.html")
    
    result = send_mcp_request("scrape_url", {
        "url": "https://doc.rust-lang.org/book/ch01-01-installation.html",
        "max_chars": 2000
    })
    
    if result and 'content' in result:
        text = result['content'][0]['text']
        print(text[:1500])
        if len(text) > 1500:
            print(f"\n... (truncated, {len(text)} total chars)")
    else:
        print(f"{Color.RED}No result{Color.END}")

def demo_json_output():
    print_header("DEMO 4: JSON Output Format (Priority 1)")
    print_subheader("URL: https://example.com (output_format=json)")
    
    result = send_mcp_request("scrape_url", {
        "url": "https://example.com",
        "output_format": "json",
        "max_chars": 1000
    })
    
    if result and 'content' in result:
        text = result['content'][0]['text']
        try:
            # Parse the JSON to verify it's valid
            data = json.loads(text)
            print(f"{Color.GREEN}✓ Valid JSON output{Color.END}\n")
            print(json.dumps(data, indent=2)[:1000])
        except:
            print(text[:1000])
    else:
        print(f"{Color.RED}No result{Color.END}")

def demo_duplicate_detection():
    print_header("DEMO 5: Duplicate Detection (Phase 2)")
    print_subheader("Searching 'python tutorial' twice")
    
    print(f"\n{Color.YELLOW}First search:{Color.END}")
    result1 = send_mcp_request("search_web", {
        "query": "python tutorial",
        "max_results": 3
    })
    
    if result1 and 'content' in result1:
        text = result1['content'][0]['text']
        print(text[:600])
        print()
    
    print(f"\n{Color.YELLOW}Second search (should warn about duplicate):{Color.END}")
    result2 = send_mcp_request("search_web", {
        "query": "python tutorial",
        "max_results": 3
    })
    
    if result2 and 'content' in result2:
        text = result2['content'][0]['text']
        if "searched within" in text or "recent" in text.lower():
            print(f"{Color.GREEN}✓ Duplicate warning detected!{Color.END}\n")
        print(text[:600])
    else:
        print(f"{Color.RED}No result{Color.END}")

def demo_history_search():
    print_header("DEMO 6: Research History Search (Phase 1)")
    print_subheader("Semantic search: 'programming tutorials'")
    
    result = send_mcp_request("research_history", {
        "query": "programming tutorials",
        "limit": 5,
        "threshold": 0.6
    })
    
    if result and 'content' in result:
        text = result['content'][0]['text']
        if "not available" in text.lower():
            print(f"{Color.YELLOW}Note: History feature requires Qdrant connection{Color.END}\n")
        print(text[:1200])
    else:
        print(f"{Color.RED}No result{Color.END}")

def main():
    print(f"\n{Color.BOLD}╔════════════════════════════════════════════════════════════════╗{Color.END}")
    print(f"{Color.BOLD}║   SEARCH-SCRAPE MCP SERVER - REAL FEATURE DEMONSTRATION       ║{Color.END}")
    print(f"{Color.BOLD}╚════════════════════════════════════════════════════════════════╝{Color.END}")
    
    print(f"\n{Color.BOLD}Configuration:{Color.END}")
    print(f"  • Binary: {BINARY}")
    print(f"  • Qdrant (gRPC): {QDRANT_URL}")
    print(f"  • SearXNG: {SEARXNG_URL}")
    print(f"  • Time: {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}")
    
    # Check binary exists
    if not os.path.exists(BINARY):
        print(f"\n{Color.RED}Error: Binary not found at {BINARY}{Color.END}")
        sys.exit(1)
    
    try:
        # Run all demos
        demo_basic_search()
        demo_query_rewriting()
        demo_scrape_with_code()
        demo_json_output()
        demo_duplicate_detection()
        demo_history_search()
        
        # Summary
        print(f"\n{Color.BOLD}╔════════════════════════════════════════════════════════════════╗{Color.END}")
        print(f"{Color.BOLD}║                   DEMONSTRATION COMPLETE                       ║{Color.END}")
        print(f"{Color.BOLD}╚════════════════════════════════════════════════════════════════╝{Color.END}")
        
        print(f"\n{Color.GREEN}Features Demonstrated:{Color.END}")
        print("  ✓ Basic web search")
        print("  ✓ Query rewriting (Phase 2)")
        print("  ✓ Code extraction (Priority 1)")
        print("  ✓ JSON output (Priority 1)")
        print("  ✓ Duplicate detection (Phase 2)")
        print("  ✓ History search (Phase 1)")
        print()
        
    except KeyboardInterrupt:
        print(f"\n\n{Color.YELLOW}Demonstration interrupted{Color.END}")
        sys.exit(0)

if __name__ == "__main__":
    main()
