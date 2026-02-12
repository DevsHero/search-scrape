import json
import time
import urllib.request
from pathlib import Path

BASE = "http://localhost:5001"


def post_json(path, payload, timeout=120):
    data = json.dumps(payload).encode("utf-8")
    req = urllib.request.Request(
        BASE + path, data=data, headers={"Content-Type": "application/json"}
    )
    start = time.time()
    try:
        with urllib.request.urlopen(req, timeout=timeout) as response:
            body = response.read().decode("utf-8", errors="replace")
            return response.status, body, round(time.time() - start, 3), None
    except Exception as error:
        return None, "", round(time.time() - start, 3), str(error)


def get_json(path, timeout=30):
    start = time.time()
    try:
        with urllib.request.urlopen(BASE + path, timeout=timeout) as response:
            body = response.read().decode("utf-8", errors="replace")
            return response.status, body, round(time.time() - start, 3), None
    except Exception as error:
        return None, "", round(time.time() - start, 3), str(error)


CASES = [
    ("search_web_dev_docs", {"name": "search_web", "arguments": {"query": "rust async await official docs", "max_results": 5}}),
    ("search_web_recent_news", {"name": "search_web", "arguments": {"query": "AI model release 2026", "time_range": "month", "max_results": 5}}),
    ("search_structured_market", {"name": "search_structured", "arguments": {"query": "Zillow housing market trends", "top_n": 2, "use_proxy": False}}),
    ("scrape_url_docs_json", {"name": "scrape_url", "arguments": {"url": "https://doc.rust-lang.org/book/ch01-02-hello-world.html", "output_format": "json", "max_chars": 8000}}),
    ("scrape_url_js_heavy", {"name": "scrape_url", "arguments": {"url": "https://news.ycombinator.com/", "output_format": "json", "max_chars": 8000}}),
    ("scrape_batch_multi", {"name": "scrape_batch", "arguments": {"urls": ["https://example.com", "https://doc.rust-lang.org/book/", "https://docs.docker.com/get-started/"], "max_concurrent": 3, "max_chars": 5000, "output_format": "json"}}),
    ("crawl_website_docs", {"name": "crawl_website", "arguments": {"url": "https://doc.rust-lang.org/book/", "max_depth": 1, "max_pages": 3, "max_concurrent": 2, "same_domain_only": True, "max_chars_per_page": 3000}}),
    ("extract_structured_schema", {"name": "extract_structured", "arguments": {"url": "https://doc.rust-lang.org/book/ch01-02-hello-world.html", "schema": [{"name": "language_name", "description": "Programming language name", "field_type": "string"}, {"name": "main_command", "description": "Main command shown in the page", "field_type": "string"}, {"name": "code_snippets", "description": "Notable code examples", "field_type": "array"}], "prompt": "Extract the key learning elements for beginners.", "max_chars": 6000}}),
    ("research_history_semantic", {"name": "research_history", "arguments": {"query": "rust docs hello world", "entry_type": "scrape", "limit": 5, "threshold": 0.5}}),
    ("proxy_manager_list", {"name": "proxy_manager", "arguments": {"action": "list", "limit": 5, "show_proxy_type": True}}),
    ("proxy_manager_status", {"name": "proxy_manager", "arguments": {"action": "status"}}),
    ("proxy_manager_switch", {"name": "proxy_manager", "arguments": {"action": "switch", "force_new": False}}),
    ("proxy_manager_test", {"name": "proxy_manager", "arguments": {"action": "test", "proxy_url": "http://43.134.238.25:443", "target_url": "https://httpbin.org/ip"}}),
    ("proxy_manager_grab_sample", {"name": "proxy_manager", "arguments": {"action": "grab", "limit": 3, "proxy_type": "http", "store_ip_txt": False, "clear_ip_txt": False, "append": False}}),
]

result = {
    "timestamp": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    "base_url": BASE,
    "health": None,
    "tools": None,
    "cases": [],
}

status, body, latency, error = get_json("/health")
result["health"] = {
    "status": status,
    "latency_sec": latency,
    "error": error,
    "body": body[:500],
}

status, body, latency, error = get_json("/mcp/tools")
parsed_tools = None
if body:
    try:
        parsed_tools = json.loads(body)
    except Exception:
        parsed_tools = None

result["tools"] = {
    "status": status,
    "latency_sec": latency,
    "error": error,
    "tool_count": len(parsed_tools.get("tools", [])) if parsed_tools else None,
}

for case_name, payload in CASES:
    status, body, latency, error = post_json("/mcp/call", payload, timeout=180)
    parsed = None
    preview = body[:1000]
    is_error = None
    content_text = None

    if body:
        try:
            parsed = json.loads(body)
        except Exception:
            parsed = None

    if isinstance(parsed, dict):
        is_error = parsed.get("is_error")
        content = parsed.get("content")
        if isinstance(content, list) and content and isinstance(content[0], dict):
            content_text = content[0].get("text")

    result["cases"].append(
        {
            "name": case_name,
            "tool": payload.get("name"),
            "arguments": payload.get("arguments"),
            "http_status": status,
            "latency_sec": latency,
            "transport_error": error,
            "is_error": is_error,
            "content_preview": (content_text[:1200] if isinstance(content_text, str) else preview),
        }
    )

out_path = Path("/Users/hero/Documents/GitHub/search-scrape/docs/RELEASE_READINESS_2026-02-12.json")
out_path.write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding="utf-8")
print(str(out_path))
print(f"cases={len(result['cases'])}")
