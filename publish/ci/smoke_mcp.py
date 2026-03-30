#!/usr/bin/env python3
"""Local MCP smoke test for Cortex Scout.

Runs a newline-delimited JSON-RPC stdio session against the local `cortex-scout-mcp`
binary and exercises the main public tools using safe example inputs.

This is intended for local validation before release or after runtime changes.
It is network-dependent and should not be treated as a deterministic CI gate.
"""

from __future__ import annotations

import argparse
import json
import os
import pathlib
import subprocess
import sys
import time
from typing import Any


ROOT = pathlib.Path(__file__).resolve().parents[2]
DEFAULT_BINARY = ROOT / "mcp-server" / "target" / "release" / "cortex-scout-mcp"


def send(proc: subprocess.Popen[bytes], payload: dict[str, Any]) -> None:
    assert proc.stdin is not None
    proc.stdin.write((json.dumps(payload) + "\n").encode())
    proc.stdin.flush()


def read_msg(proc: subprocess.Popen[bytes], timeout: int) -> dict[str, Any]:
    start = time.time()
    line = b""
    assert proc.stdout is not None
    while not line.endswith(b"\n"):
        if time.time() - start > timeout:
            raise TimeoutError("timeout waiting for MCP response")
        chunk = proc.stdout.read(1)
        if not chunk:
            raise RuntimeError("server stdout closed while reading MCP response")
        line += chunk
    return json.loads(line)


def tool_text_preview(
    response: dict[str, Any], tool_name: str, max_chars: int = 220
) -> str:
    result = response.get("result", {})
    content = result.get("content", [])
    if not content:
        return ""
    text = content[0].get("text", "")
    if tool_name == "scout_browser_automate":
        try:
            steps = json.loads(text)
            errors = [
                {
                    "step": item.get("step"),
                    "action": item.get("action"),
                    "error": item.get("error"),
                }
                for item in steps
                if item.get("status") == "error"
            ]
            if errors:
                return json.dumps({"errors": errors}, indent=2)
            return json.dumps(
                {"steps": len(steps), "last_action": steps[-1].get("action") if steps else None},
                indent=2,
            )
        except Exception:
            return text[:2500]
    return text[:max_chars]


def validate_browser_automate_response(response: dict[str, Any]) -> list[str]:
    result = response.get("result", {})
    content = result.get("content", [])
    if not content:
        return ["missing browser automation response content"]
    text = content[0].get("text", "")
    try:
        steps = json.loads(text)
    except Exception as exc:  # noqa: BLE001
        return [f"browser automate response was not valid JSON: {exc!r}"]

    if not isinstance(steps, list) or not steps:
        return ["browser automate returned no step results"]

    failures = []
    error_steps = [item for item in steps if item.get("status") == "error"]
    if error_steps:
        failures.append(f"browser automate returned error steps: {error_steps}")

    def find_action(action: str) -> list[dict[str, Any]]:
        return [item for item in steps if item.get("action") == action]

    wait_steps = find_action("wait_for")
    if not wait_steps or wait_steps[0].get("result", {}).get("text") != "Example Domain":
        failures.append("wait_for did not confirm Example Domain")

    fill_result = find_action("fill_form")
    if not fill_result or fill_result[0].get("result", {}).get("filled") != 2:
        failures.append("fill_form did not report filling both fields")

    verify_results = [item.get("result", {}) for item in find_action("verify_value")]
    verified_pairs = {(item.get("selector"), item.get("value")) for item in verify_results if item.get("verified") is True}
    if ("#name", "copilot") not in verified_pairs:
        failures.append("verify_value did not confirm textbox value")
    if ("#agree", "true") not in verified_pairs:
        failures.append("verify_value did not confirm checkbox value")

    generate_steps = find_action("generate_locator")
    if not generate_steps or not generate_steps[0].get("result", {}).get("ok"):
        failures.append("generate_locator did not return a valid locator")

    dialog_eval = None
    mock_eval = None
    for item in find_action("evaluate"):
        value = item.get("result")
        if isinstance(value, dict) and {"confirm", "prompt"} <= set(value):
            dialog_eval = value
        if isinstance(value, dict) and {"text", "testHeader", "contentType"} <= set(value):
            mock_eval = value

    if dialog_eval != {"confirm": True, "prompt": "copilot"}:
        failures.append(f"dialog handling returned unexpected result: {dialog_eval}")

    storage_export = find_action("storage_state_export")
    if not storage_export:
        failures.append("storage_state_export step missing")
    else:
        exported = storage_export[0].get("result", {})
        if exported.get("localStorage", {}).get("smoke") != "1":
            failures.append("storage_state_export did not persist localStorage fixture")
        if exported.get("sessionStorage", {}).get("sessionSmoke") != "ok":
            failures.append("storage_state_export did not persist sessionStorage fixture")

    if mock_eval is None:
        failures.append("mock fetch verification result missing")
    else:
        if mock_eval.get("text") != '{"ok":true}':
            failures.append(f"mock fetch returned unexpected body: {mock_eval}")
        if mock_eval.get("contentType") != "application/json":
            failures.append(f"mock fetch returned unexpected content-type: {mock_eval}")
        if mock_eval.get("testHeader") is not None:
            failures.append(f"mock fetch still exposed stripped header: {mock_eval}")

    route_lists = find_action("route_list")
    if len(route_lists) < 2:
        failures.append("route_list coverage is incomplete")
    else:
        first_routes = route_lists[0].get("result", {}).get("routes", [])
        second_routes = route_lists[1].get("result", {}).get("routes", [])
        if not first_routes:
            failures.append("route_list did not show installed route")
        if second_routes:
            failures.append(f"route_list after unroute should be empty: {second_routes}")

    network_dump = find_action("network_dump")
    if not network_dump or network_dump[0].get("result", {}).get("total", 0) < 1:
        failures.append("network_dump did not capture any requests")

    console_dump = find_action("console_dump")
    if not console_dump:
        failures.append("console_dump step missing")
    else:
        events = console_dump[0].get("result", {}).get("events", [])
        joined = json.dumps(events)
        if "smoke-start" not in joined:
            failures.append("console_dump missed smoke-start log")
        if '{\\"ok\\":true}' not in joined:
            failures.append("console_dump missed mocked fetch payload log")

    tabs_list = find_action("tabs")
    if len(tabs_list) < 3:
        failures.append("tabs actions did not execute expected sequence")
    else:
        list_step = next((item for item in tabs_list if isinstance(item.get("result", {}).get("tabs"), list)), None)
        if not list_step or len(list_step.get("result", {}).get("tabs", [])) < 2:
            failures.append("tabs list did not show the newly opened tab")

    pdf_steps = find_action("pdf_save")
    if not pdf_steps:
        failures.append("pdf_save step missing")
    else:
        pdf_path = pathlib.Path(pdf_steps[0].get("result", {}).get("path", ""))
        if pdf_path.name != "browser_smoke.pdf" or not pdf_path.exists() or pdf_path.stat().st_size == 0:
            failures.append(f"pdf_save did not produce a valid artifact: {pdf_path}")

    return failures


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", default=str(DEFAULT_BINARY))
    parser.add_argument("--query", default="rust model context protocol")
    parser.add_argument(
        "--only-tool",
        action="append",
        default=[],
        help="Run only the named tool(s). Can be passed multiple times.",
    )
    args = parser.parse_args()

    binary = pathlib.Path(args.binary)
    if not binary.exists():
        raise SystemExit(f"❌ Binary not found: {binary}")

    env = os.environ.copy()
    env.setdefault("RUST_LOG", "warn")
    env.setdefault("SEARCH_ENGINES", "google,bing,duckduckgo,brave")
    env.setdefault("LANCEDB_URI", str(ROOT / "lancedb"))
    env.setdefault("HTTP_TIMEOUT_SECS", "20")
    env.setdefault("HTTP_CONNECT_TIMEOUT_SECS", "8")
    env.setdefault("MAX_CONTENT_CHARS", "6000")
    env.setdefault("IP_LIST_PATH", str(ROOT / "ip.txt"))
    env.setdefault("PROXY_SOURCE_PATH", str(ROOT / "proxy_source.json"))

    proc = subprocess.Popen(
        [str(binary)],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env=env,
        cwd=str(ROOT),
    )

    results: list[tuple[str, bool, str]] = []

    try:
        send(
            proc,
            {
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2025-03-26",
                    "capabilities": {},
                    "clientInfo": {"name": "publish/ci/smoke_mcp.py", "version": "1.0"},
                },
            },
        )
        init = read_msg(proc, 30)
        results.append(("initialize", "result" in init, ""))

        send(proc, {"jsonrpc": "2.0", "method": "notifications/initialized", "params": {}})
        send(proc, {"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}})
        listed = read_msg(proc, 30)
        tools = {tool["name"] for tool in listed["result"]["tools"]}
        results.append(("tools/list", True, ", ".join(sorted(tools))))

        tests: list[tuple[str, dict[str, Any], int, bool]] = [
            ("web_search", {"query": args.query, "max_results": 3}, 120, False),
            ("web_search", {"query": args.query, "include_content": True, "top_n": 1, "snippet_chars": 120}, 150, False),
            ("web_fetch", {"mode": "single", "url": "https://example.com", "output_format": "clean_json", "max_chars": 1200}, 120, False),
            ("web_fetch", {"mode": "batch", "urls": ["https://example.com", "https://example.org"], "max_concurrent": 1, "max_chars": 1200, "output_format": "json"}, 180, False),
            ("web_fetch", {"mode": "crawl", "url": "https://example.com", "max_depth": 0, "max_pages": 1, "max_chars_per_page": 1200, "output_format": "json"}, 120, False),
            ("extract_fields", {"url": "https://example.com", "schema": [{"name": "title", "type": "string"}, {"name": "summary", "type": "string"}], "max_chars": 1200}, 120, False),
            ("memory_search", {"query": args.query, "limit": 2, "threshold": 0.1}, 120, False),
            ("proxy_control", {"action": "status"}, 60, False),
            ("proxy_control", {"action": "test", "strict_proxy_health": False}, 60, False),
            ("visual_scout", {"url": "https://example.com", "output_format": "json", "width": 1024, "height": 768}, 180, False),
            (
                "scout_browser_automate",
                {
                    "steps": [
                        {"action": "navigate", "target": "https://example.com"},
                        {"action": "wait_for", "text": "Example Domain", "timeout_ms": 15000},
                        {"action": "verify_text_visible", "text": "Example Domain", "timeout_ms": 5000},
                        {"action": "resize", "width": 1024, "height": 700},
                        {"action": "hover", "target": "a"},
                        {"action": "console_tap"},
                        {"action": "network_tap"},
                        {
                            "action": "evaluate",
                            "value": "(function(){ console.info('smoke-start'); var upload=document.createElement('input'); upload.type='file'; upload.id='upload'; document.body.appendChild(upload); var name=document.createElement('input'); name.id='name'; name.placeholder='Name'; document.body.appendChild(name); var agree=document.createElement('input'); agree.type='checkbox'; agree.id='agree'; document.body.appendChild(agree); var list=document.createElement('ul'); list.id='items'; list.innerHTML='<li>alpha</li><li>beta</li>'; document.body.appendChild(list); return { injected:true, title: document.title }; })()",
                        },
                        {
                            "action": "fill_form",
                            "fields": [
                                {"selector": "#name", "type": "textbox", "value": "copilot"},
                                {"selector": "#agree", "type": "checkbox", "value": True},
                            ],
                        },
                        {"action": "verify_value", "target": "#name", "value": "copilot", "type": "textbox", "timeout_ms": 5000},
                        {"action": "verify_value", "target": "#agree", "value": "true", "type": "checkbox", "timeout_ms": 5000},
                        {"action": "generate_locator", "target": "#name", "timeout_ms": 5000},
                        {"action": "verify_list_visible", "target": "#items", "items": ["alpha", "beta"], "timeout_ms": 5000},
                        {"action": "file_upload", "target": "#upload", "paths": [str(ROOT / "README.md")], "timeout_ms": 5000},
                        {"action": "handle_dialog", "accept": True, "promptText": "copilot"},
                        {"action": "evaluate", "value": "(function(){ return { confirm: window.confirm('ok?'), prompt: window.prompt('name?') }; })()"},
                        {"action": "storage_state_import", "value": '{"localStorage":{"smoke":"1"},"sessionStorage":{"sessionSmoke":"ok"},"cookies":""}'},
                        {"action": "storage_state_export"},
                        {"action": "mock_api", "url_pattern": "*example.com/mock*", "response_json": '{"ok":true}', "status_code": 200, "response_headers": {"X-Test": "1", "Content-Type": "application/json"}, "remove_headers": ["X-Test"]},
                        {"action": "evaluate", "value": "(async function(){ let r = await fetch('https://example.com/mock'); let text = await r.text(); console.info(text); return { text: text, testHeader: r.headers.get('X-Test'), contentType: r.headers.get('Content-Type') }; })()"},
                        {"action": "route_list"},
                        {"action": "unroute", "pattern": "*example.com/mock*"},
                        {"action": "route_list"},
                        {"action": "network_dump"},
                        {"action": "console_dump", "level": "info"},
                        {"action": "tabs", "value": "new", "target": "about:blank"},
                        {"action": "tabs", "value": "list"},
                        {"action": "tabs", "value": "close"},
                        {"action": "pdf_save", "filename": str(ROOT / "mcp-server" / "target" / "browser_smoke.pdf")},
                    ]
                },
                240,
                False,
            ),
            ("scout_browser_close", {}, 60, False),
            ("deep_research", {"query": args.query, "depth": 1, "max_sources": 1, "max_concurrent": 1, "max_chars_per_source": 1200}, 240, False),
            ("hitl_web_fetch", {"url": "https://example.com", "auth_mode": "challenge", "human_timeout_seconds": 1, "challenge_grace_seconds": 1, "output_format": "json", "max_chars": 500}, 90, True),
        ]

        if args.only_tool:
            selected = set(args.only_tool)
            tests = [test for test in tests if test[0] in selected]

        next_id = 10
        for tool_name, tool_args, timeout, optional in tests:
            if tool_name not in tools:
                if optional:
                    results.append((tool_name, True, "optional tool missing from catalog"))
                else:
                    results.append((tool_name, False, "tool missing from catalog"))
                continue
            send(
                proc,
                {
                    "jsonrpc": "2.0",
                    "id": next_id,
                    "method": "tools/call",
                    "params": {"name": tool_name, "arguments": tool_args},
                },
            )
            next_id += 1
            try:
                response = read_msg(proc, timeout)
                preview = tool_text_preview(response, tool_name)
                ok = "result" in response and not response["result"].get("isError", False)
                if ok and tool_name == "scout_browser_automate":
                    browser_failures = validate_browser_automate_response(response)
                    if browser_failures:
                        ok = False
                        preview = json.dumps({"validation_errors": browser_failures}, indent=2)
                if optional and not ok and "is not enabled in this running binary" in preview:
                    results.append((tool_name, True, "optional tool disabled in this binary"))
                else:
                    results.append((tool_name, ok, preview))
            except Exception as exc:  # noqa: BLE001 - keep smoke diagnostics simple
                results.append((tool_name, False, repr(exc)))

        failed = [row for row in results if not row[1] and row[0] not in {"initialize", "tools/list"}]
        print(json.dumps({"results": results, "failed": failed}, indent=2))

        stderr_text = ""
        if proc.stderr is not None:
            time.sleep(1)
            if proc.poll() is None:
                proc.kill()
            stderr_text = proc.stderr.read().decode("utf-8", "replace")

        if stderr_text.strip():
            print("STDERR_START")
            print(stderr_text)
            print("STDERR_END")

        return 1 if failed else 0
    finally:
        if proc.poll() is None:
            proc.kill()


if __name__ == "__main__":
    sys.exit(main())