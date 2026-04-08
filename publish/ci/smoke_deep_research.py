#!/usr/bin/env python3
"""Focused MCP smoke coverage for the deep_research tool.

Runs several parameterized MCP tool calls against the local `cortex-scout-mcp`
binary and validates the effective configuration returned by the tool.
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


def parse_tool_result(response: dict[str, Any]) -> dict[str, Any]:
    result = response.get("result", {})
    content = result.get("content", [])
    if not content:
        raise ValueError("missing MCP content")
    return json.loads(content[0].get("text", "{}"))


def validate_success_payload(payload: dict[str, Any], expected: dict[str, Any]) -> list[str]:
    failures: list[str] = []
    effective = payload.get("effective_config") or {}

    if payload.get("query") != expected["query"]:
        failures.append(f"query mismatch: {payload.get('query')!r}")

    for key, value in expected["effective_config"].items():
        if effective.get(key) != value:
            failures.append(
                f"effective_config.{key} mismatch: expected {value!r}, got {effective.get(key)!r}"
            )

    if payload.get("depth_used") != expected["effective_config"]["depth"]:
        failures.append(
            f"depth_used mismatch: expected {expected['effective_config']['depth']!r}, got {payload.get('depth_used')!r}"
        )

    if payload.get("sources_discovered", 0) < payload.get("sources_scraped", 0):
        failures.append("sources_discovered was smaller than sources_scraped")

    if not isinstance(payload.get("warnings", []), list):
        failures.append("warnings was not a list")

    if payload.get("total_duration_ms", 0) <= 0:
        failures.append("total_duration_ms was not positive")

    return failures


def validate_error_payload(response: dict[str, Any], expected_error: str) -> list[str]:
    if "error" in response:
        message = response["error"].get("message", "")
        if expected_error not in message:
            return [f"expected error substring {expected_error!r}, got {message!r}"]
        return []

    result = response.get("result", {})
    if not result.get("isError", False):
        return ["expected tool call to fail"]
    content = result.get("content", [])
    if not content:
        return ["missing error content"]
    text = content[0].get("text", "")
    if expected_error not in text:
        return [f"expected error substring {expected_error!r}, got {text!r}"]
    return []


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", default=str(DEFAULT_BINARY))
    parser.add_argument("--query", default="rust model context protocol")
    args = parser.parse_args()

    binary = pathlib.Path(args.binary)
    if not binary.exists():
        raise SystemExit(f"Binary not found: {binary}")

    env = os.environ.copy()
    env.setdefault("RUST_LOG", "warn")
    env.setdefault("SEARCH_ENGINES", "google")
    env.setdefault("LANCEDB_URI", str(ROOT / "lancedb"))
    env.setdefault("HTTP_TIMEOUT_SECS", "12")
    env.setdefault("HTTP_CONNECT_TIMEOUT_SECS", "4")
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

    scenarios = [
        {
            "label": "defaults",
            "args": {"query": args.query},
            "timeout": 360,
            "expected": {
                "query": args.query,
                "effective_config": {
                    "depth": 1,
                    "max_sources": 10,
                    "max_chars_per_source": 20000,
                    "max_concurrent": 3,
                    "use_proxy": False,
                    "relevance_threshold": None,
                    "quality_mode": "balanced",
                },
            },
        },
        {
            "label": "all_params_aggressive",
            "args": {
                "query": args.query,
                "depth": 2,
                "max_sources": 2,
                "max_chars_per_source": 800,
                "max_concurrent": 2,
                "use_proxy": False,
                "relevance_threshold": 0.2,
                "quality_mode": "aggressive",
            },
            "timeout": 420,
            "expected": {
                "query": args.query,
                "effective_config": {
                    "depth": 2,
                    "max_sources": 2,
                    "max_chars_per_source": 800,
                    "max_concurrent": 2,
                    "use_proxy": False,
                    "relevance_threshold": 0.2,
                    "quality_mode": "aggressive",
                },
            },
        },
        {
            "label": "all_params_high_proxy",
            "args": {
                "query": args.query,
                "depth": 3,
                "max_sources": 1,
                "max_chars_per_source": 500,
                "max_concurrent": 1,
                "use_proxy": True,
                "relevance_threshold": 1.0,
                "quality_mode": "high",
            },
            "timeout": 480,
            "expected": {
                "query": args.query,
                "effective_config": {
                    "depth": 3,
                    "max_sources": 1,
                    "max_chars_per_source": 500,
                    "max_concurrent": 1,
                    "use_proxy": True,
                    "relevance_threshold": 1.0,
                    "quality_mode": "high",
                },
            },
        },
        {
            "label": "clamped_bounds",
            "args": {
                "query": args.query,
                "depth": 99,
                "max_sources": 999,
                "max_chars_per_source": 0,
                "max_concurrent": 999,
                "use_proxy": False,
                "relevance_threshold": 2.0,
                "quality_mode": "balanced",
            },
            "timeout": 300,
            "expected": {
                "query": args.query,
                "effective_config": {
                    "depth": 3,
                    "max_sources": 20,
                    "max_chars_per_source": 1,
                    "max_concurrent": 10,
                    "use_proxy": False,
                    "relevance_threshold": 1.0,
                    "quality_mode": "balanced",
                },
            },
        },
        {
            "label": "invalid_quality_mode",
            "args": {"query": args.query, "quality_mode": "turbo"},
            "timeout": 60,
            "expected_error": "Invalid quality_mode. Allowed values: balanced, aggressive, high",
        },
        {
            "label": "empty_query",
            "args": {"query": "   "},
            "timeout": 60,
            "expected_error": "query must not be empty",
        },
    ]

    results: list[dict[str, Any]] = []

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
                    "clientInfo": {"name": "publish/ci/smoke_deep_research.py", "version": "1.0"},
                },
            },
        )
        _ = read_msg(proc, 30)
        send(proc, {"jsonrpc": "2.0", "method": "notifications/initialized", "params": {}})
        send(proc, {"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}})
        tools = read_msg(proc, 30)["result"]["tools"]
        tool_names = {tool["name"] for tool in tools}
        if "deep_research" not in tool_names:
            raise RuntimeError("deep_research missing from tool catalog")

        next_id = 10
        for scenario in scenarios:
            send(
                proc,
                {
                    "jsonrpc": "2.0",
                    "id": next_id,
                    "method": "tools/call",
                    "params": {"name": "deep_research", "arguments": scenario["args"]},
                },
            )
            next_id += 1
            response = read_msg(proc, scenario["timeout"])

            if "expected_error" in scenario:
                failures = validate_error_payload(response, scenario["expected_error"])
                results.append(
                    {
                        "label": scenario["label"],
                        "ok": not failures,
                        "failures": failures,
                    }
                )
                continue

            if "result" not in response or response["result"].get("isError", False):
                content = response.get("result", {}).get("content", [])
                preview = content[0].get("text", "") if content else repr(response)
                results.append(
                    {
                        "label": scenario["label"],
                        "ok": False,
                        "failures": [f"tool call failed: {preview}"],
                    }
                )
                continue

            payload = parse_tool_result(response)
            failures = validate_success_payload(payload, scenario["expected"])
            results.append(
                {
                    "label": scenario["label"],
                    "ok": not failures,
                    "failures": failures,
                    "preview": {
                        "depth_used": payload.get("depth_used"),
                        "sources_scraped": payload.get("sources_scraped"),
                        "effective_config": payload.get("effective_config"),
                    },
                }
            )

        failed = [row for row in results if not row["ok"]]
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