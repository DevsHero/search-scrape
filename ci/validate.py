#!/usr/bin/env python3
"""
ci/validate.py — mirrors the check-trigger validation block in release.yml.
Run via: python3 ci/validate.py  OR  make validate
"""
import sys
import json
import pathlib
import tomllib
import yaml  # PyYAML — install with: pip3 install pyyaml

ROOT = pathlib.Path(__file__).parent.parent


def check_versions() -> str:
    cargo_toml = ROOT / "mcp-server" / "Cargo.toml"
    server_json = ROOT / "server.json"

    ct = tomllib.loads(cargo_toml.read_text(encoding="utf-8"))
    cargo_v = ct.get("package", {}).get("version", "")
    assert cargo_v, f"Missing [package].version in {cargo_toml}"

    sj = json.loads(server_json.read_text(encoding="utf-8"))
    srv_v = sj.get("version", "")
    assert srv_v, f"Missing version in {server_json}"

    assert cargo_v == srv_v, (
        f"Version mismatch: Cargo.toml={cargo_v} server.json={srv_v}\n"
        "  Fix: update the version in both files to match, then recommit."
    )
    print(f"✅ version={cargo_v} — Cargo.toml and server.json match")
    return cargo_v


def check_smithery() -> None:
    p = ROOT / "smithery.yaml"
    obj = yaml.safe_load(p.read_text(encoding="utf-8"))
    assert isinstance(obj, dict), "smithery.yaml is not a YAML mapping"
    sc = obj.get("startCommand")
    assert isinstance(sc, dict), "smithery.yaml missing 'startCommand' mapping"
    assert sc.get("type") == "stdio", f"startCommand.type must be 'stdio', got {sc.get('type')!r}"
    assert "configSchema" in sc, "smithery.yaml missing 'startCommand.configSchema'"
    assert "commandFunction" in sc, "smithery.yaml missing 'startCommand.commandFunction'"
    print("✅ smithery.yaml OK")


def check_config_schema() -> None:
    p = ROOT / "smithery.config-schema.json"
    obj = json.loads(p.read_text(encoding="utf-8"))
    assert obj.get("type") == "object", "smithery.config-schema.json: top-level 'type' must be 'object'"
    assert isinstance(obj.get("properties"), dict), "smithery.config-schema.json: missing 'properties' dict"
    print("✅ smithery.config-schema.json OK")


def check_server_json_valid() -> None:
    p = ROOT / "server.json"
    json.loads(p.read_text(encoding="utf-8"))
    print("✅ server.json is valid JSON")


if __name__ == "__main__":
    errors = []
    for fn in [check_versions, check_smithery, check_config_schema, check_server_json_valid]:
        try:
            fn()
        except (AssertionError, Exception) as exc:
            errors.append(f"❌ {fn.__name__}: {exc}")

    if errors:
        print("\n" + "\n".join(errors), file=sys.stderr)
        sys.exit(1)
