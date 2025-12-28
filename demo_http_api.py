#!/usr/bin/env python3
"""
Simple HTTP API demo - easier to test than MCP stdio protocol
"""

import requests
import json
from datetime import datetime

# Configuration
BASE_URL = "http://localhost:5000"

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

def demo_search(query, max_results=5):
    """Demonstrate search feature"""
    print_header(f"Searching: '{query}'")
    
    try:
        response = requests.post(f"{BASE_URL}/search", json={
            "query": query
        }, timeout=30)
        
        if response.status_code == 200:
            data = response.json()
            results = data.get('results', [])
            
            print(f"{Color.GREEN}✓ Found {len(results)} results{Color.END}\n")
            
            for i, result in enumerate(results[:max_results], 1):
                print(f"{i}. {Color.BOLD}{result['title']}{Color.END}")
                print(f"   URL: {result['url']}")
                print(f"   Snippet: {result['content'][:150]}...")
                if result.get('domain'):
                    print(f"   Domain: {result['domain']}")
                if result.get('source_type'):
                    print(f"   Type: {result['source_type']}")
                print()
        else:
            print(f"{Color.RED}Error: {response.status_code}{Color.END}")
            print(response.text)
            
    except Exception as e:
        print(f"{Color.RED}Request failed: {e}{Color.END}")

def demo_scrape(url, max_chars=2000):
    """Demonstrate scraping feature"""
    print_header(f"Scraping: {url}")
    
    try:
        response = requests.post(f"{BASE_URL}/scrape", json={
            "url": url
        }, timeout=30)
        
        if response.status_code == 200:
            data = response.json()
            
            print(f"{Color.BOLD}{data['title']}{Color.END}")
            print(f"URL: {data['url']}")
            print(f"Word Count: {data['word_count']}")
            print(f"Language: {data['language']}")
            
            if data.get('canonical_url'):
                print(f"Canonical: {data['canonical_url']}")
            if data.get('author'):
                print(f"Author: {data['author']}")
            if data.get('published_at'):
                print(f"Published: {data['published_at']}")
            
            print(f"\n{Color.BLUE}Description:{Color.END}")
            print(data.get('meta_description', 'N/A'))
            
            if data.get('headings'):
                print(f"\n{Color.BLUE}Headings:{Color.END}")
                for h in data['headings'][:5]:
                    print(f"  {h['level']}: {h['text']}")
            
            if data.get('code_blocks'):
                print(f"\n{Color.GREEN}✓ Code blocks extracted: {len(data['code_blocks'])}{Color.END}")
                for i, block in enumerate(data['code_blocks'][:2], 1):
                    lang = block.get('language', 'unknown')
                    print(f"  Block {i} ({lang}): {len(block['code'])} chars")
            
            print(f"\n{Color.BLUE}Content Preview:{Color.END}")
            print(data['clean_content'][:max_chars])
            if len(data['clean_content']) > max_chars:
                print(f"\n... (truncated, {len(data['clean_content'])} total chars)")
            
            print(f"\n{Color.BLUE}Stats:{Color.END}")
            print(f"  Links: {len(data.get('links', []))}")
            print(f"  Images: {len(data.get('images', []))}")
            if data.get('extraction_score'):
                print(f"  Quality Score: {data['extraction_score']:.2f}")
            if data.get('warnings'):
                print(f"  Warnings: {', '.join(data['warnings'])}")
        else:
            print(f"{Color.RED}Error: {response.status_code}{Color.END}")
            print(response.text)
            
    except Exception as e:
        print(f"{Color.RED}Request failed: {e}{Color.END}")

def main():
    print(f"\n{Color.BOLD}╔════════════════════════════════════════════════════════════════╗{Color.END}")
    print(f"{Color.BOLD}║        SEARCH-SCRAPE HTTP API - FEATURE DEMONSTRATION          ║{Color.END}")
    print(f"{Color.BOLD}╚════════════════════════════════════════════════════════════════╝{Color.END}")
    
    print(f"\n{Color.BOLD}Configuration:{Color.END}")
    print(f"  • API Base URL: {BASE_URL}")
    print(f"  • Time: {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}")
    
    # Check if server is running
    try:
        response = requests.get(f"{BASE_URL}/health", timeout=5)
        if response.status_code == 200:
            print(f"  • Server Status: {Color.GREEN}✓ Running{Color.END}")
        else:
            print(f"  • Server Status: {Color.YELLOW}⚠ Running but unhealthy{Color.END}")
    except:
        print(f"  • Server Status: {Color.RED}✗ Not running{Color.END}")
        print(f"\n{Color.RED}Please start the server first:{Color.END}")
        print(f"  cd mcp-server")
        print(f"  QDRANT_URL=http://localhost:6334 cargo run --release")
        return
    
    # Run demos
    print("\n" + "="*70)
    print(f"{Color.BOLD}DEMONSTRATION 1: Basic Web Search{Color.END}")
    print("="*70)
    demo_search("rust async programming", max_results=3)
    
    print("\n" + "="*70)
    print(f"{Color.BOLD}DEMONSTRATION 2: Developer Query (with auto-rewrite){Color.END}")
    print("="*70)
    demo_search("rust docs tokio", max_results=3)
    
    print("\n" + "="*70)
    print(f"{Color.BOLD}DEMONSTRATION 3: Scrape Documentation{Color.END}")
    print("="*70)
    demo_scrape("https://doc.rust-lang.org/book/ch01-01-installation.html")
    
    print("\n" + "="*70)
    print(f"{Color.BOLD}DEMONSTRATION 4: Scrape Simple Page{Color.END}")
    print("="*70)
    demo_scrape("https://example.com", max_chars=500)
    
    print("\n" + "="*70)
    print(f"{Color.BOLD}DEMONSTRATION 5: Duplicate Detection{Color.END}")
    print("="*70)
    print(f"\n{Color.YELLOW}First search for 'python tutorial':{Color.END}")
    demo_search("python tutorial", max_results=2)
    
    print(f"\n{Color.YELLOW}Second search (same query - should log duplicate):{Color.END}")
    demo_search("python tutorial", max_results=2)
    
    # Summary
    print(f"\n{Color.BOLD}╔════════════════════════════════════════════════════════════════╗{Color.END}")
    print(f"{Color.BOLD}║                   DEMONSTRATION COMPLETE                       ║{Color.END}")
    print(f"{Color.BOLD}╚════════════════════════════════════════════════════════════════╝{Color.END}")
    
    print(f"\n{Color.GREEN}Features Demonstrated:{Color.END}")
    print("  ✓ Web search with SearXNG")
    print("  ✓ Content scraping with code extraction")
    print("  ✓ Metadata extraction")
    print("  ✓ Quality scoring")
    print("  ✓ Multiple queries (duplicate tracking)")
    print()

if __name__ == "__main__":
    main()
