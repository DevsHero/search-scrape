#!/usr/bin/env python3
"""
ci/smoke.py — quick HTTP smoke test for all MCP tools.
Requires the server to be running on localhost:5000.
Run via: python3 ci/smoke.py  OR  make smoke
"""
import json
import sys
import urllib.error
import urllib.request

BASE = "http://localhost:5000"


def call(name: str, arguments: dict):
    req = urllib.request.Request(
        BASE + "/mcp/call",
        data=json.dumps({"name": name, "arguments": arguments}).encode(),
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            return resp.status, json.loads(resp.read())
    except urllib.error.HTTPError as e:
        body = e.read().decode(errors="replace")
        try:
            return e.code, json.loads(body)
        except Exception:
            return e.code, {"raw": body}


CASES = [
    ("proxy_manager",    {"action": "status"}),
    ("scrape_url",       {"url": "https://example.com", "output_format": "text", "max_chars": 500, "max_links": 3}),
    ("scrape_batch",     {"urls": ["https://example.com", "https://example.org"], "output_format": "text", "max_chars": 400, "max_concurrent": 2}),
    ("extract_structured", {"url": "https://example.com", "prompt": "Extract the page title as JSON with key 'title'.", "max_chars": 1000}),
    ("crawl_website",    {"url": "https://example.com", "max_pages": 2, "max_depth": 1, "same_domain_only": True}),
    ("research_history", {"query": "example", "limit": 2}),
    ("search_structured", {"query": "example.com", "top_n": 2}),
    ("web_search",       {"query": "example.com", "max_results": 2}),
]

failed = 0
for name, args in CASES:
    status, data = call(name, args)
    is_error = data.get("is_error") if isinstance(data, dict) else True
    icon = "✅" if not is_error else "❌"
    snippet = ""
    if isinstance(data, dict) and isinstance(data.get("content"), list) and data["content"]:
        snippet = data["content"][0].get("text", "").replace("\n", " ")[:120]
    else:
        snippet = json.dumps(data)[:120]
    print(f"  {icon} {name:<22} status={status}  {snippet}")
    if is_error:
        failed += 1

print()
print(f"  non_robot_search — skipped (requires live browser / HITL)")
print()

if failed:
    print(f"❌ {failed} tool(s) returned is_error=True", file=sys.stderr)
    sys.exit(1)
else:
    print("✅ All tools responded successfully")
