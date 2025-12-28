#!/usr/bin/env python3
"""
Test MCP server to verify all tools are registered
"""

import json
import subprocess
import sys

BINARY = "/Users/hero/Documents/GitHub/search-scrape/mcp-server/target/release/search-scrape-mcp"

def test_list_tools():
    """Test that all 3 tools are registered"""
    
    print("Testing MCP Server Tool Registration")
    print("="*60)
    
    # MCP initialize
    init_msg = {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "test-client",
                "version": "1.0.0"
            }
        }
    }
    
    initialized_msg = {
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    }
    
    list_tools_msg = {
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {}
    }
    
    input_data = (
        json.dumps(init_msg) + "\n" + 
        json.dumps(initialized_msg) + "\n" + 
        json.dumps(list_tools_msg) + "\n"
    )
    
    try:
        result = subprocess.run(
            [BINARY],
            input=input_data,
            capture_output=True,
            text=True,
            timeout=10,
            env={
                "QDRANT_URL": "http://localhost:6334",
                "SEARXNG_URL": "http://localhost:8888"
            }
        )
        
        # Parse responses
        lines = result.stdout.strip().split('\n')
        
        for line in lines:
            if not line.strip():
                continue
            try:
                msg = json.loads(line)
                if msg.get('id') == 2 and 'result' in msg:
                    tools = msg['result'].get('tools', [])
                    
                    print(f"\n✅ Found {len(tools)} tools:\n")
                    
                    for i, tool in enumerate(tools, 1):
                        name = tool['name']
                        desc = tool.get('description', '')[:80] + "..."
                        print(f"{i}. {name}")
                        print(f"   {desc}")
                        print()
                    
                    # Verify all expected tools
                    tool_names = [t['name'] for t in tools]
                    expected = ['search_web', 'scrape_url', 'research_history']
                    
                    print("="*60)
                    for expected_tool in expected:
                        if expected_tool in tool_names:
                            print(f"✅ {expected_tool}: REGISTERED")
                        else:
                            print(f"❌ {expected_tool}: MISSING")
                    
                    if set(expected) == set(tool_names):
                        print("\n🎉 ALL TOOLS REGISTERED CORRECTLY!")
                        return True
                    else:
                        print("\n⚠️  Some tools are missing")
                        return False
                        
            except json.JSONDecodeError:
                continue
        
        print("❌ Could not parse tool list response")
        print("\nRaw output:")
        print(result.stdout)
        if result.stderr:
            print("\nStderr:")
            print(result.stderr)
        return False
        
    except Exception as e:
        print(f"❌ Error: {e}")
        return False

if __name__ == "__main__":
    success = test_list_tools()
    sys.exit(0 if success else 1)
