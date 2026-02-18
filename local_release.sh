#!/usr/bin/env bash
set -euo pipefail

# local_release.sh
# Build + package + upload release assets from your local machine.
# Intended for macOS builds when GitHub Actions macOS runners are unavailable/billing-blocked.

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "❌ Missing required command: $1" >&2
    exit 1
  }
}

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT_DIR"

require_cmd python3
require_cmd cargo

# ── Ensure protoc is installed ────────────────────────────────────────────────
if ! command -v protoc >/dev/null 2>&1; then
  echo "⚠️  protoc not found — installing..." >&2
  case "$(uname -s)" in
    Darwin)
      if command -v brew >/dev/null 2>&1; then
        brew install protobuf
      else
        echo "❌ Homebrew not found. Install protoc manually: brew install protobuf" >&2
        exit 1
      fi
      ;;
    Linux)
      sudo apt-get update -qq && sudo apt-get install -y protobuf-compiler
      ;;
    *)
      echo "❌ Cannot auto-install protoc on $(uname -s). Install it manually." >&2
      exit 1
      ;;
  esac
fi
echo "✅ protoc $(protoc --version)" >&2

if command -v gh >/dev/null 2>&1; then
  GH_OK=1
else
  GH_OK=0
fi

VERSION="$(python3 -c 'import tomllib, pathlib; print(tomllib.loads(pathlib.Path("mcp-server/Cargo.toml").read_text(encoding="utf-8"))["package"]["version"])')"
TAG="v${VERSION}"

ARCH_RAW="$(uname -m)"
case "$ARCH_RAW" in
  arm64|aarch64) PLATFORM="macos-arm64" ;;
  x86_64) PLATFORM="macos-x64" ;;
  *)
    echo "❌ Unsupported uname -m: $ARCH_RAW" >&2
    exit 1
    ;;
 esac

DIST_DIR="dist/${PLATFORM}"
ASSET_DEFAULT="dist/shadowcrawl-${VERSION}-${PLATFORM}.tar.gz"
ASSET_HITL="dist/shadowcrawl-${VERSION}-${PLATFORM}-non_robot_search.tar.gz"

echo "== Building ShadowCrawl ${TAG} for ${PLATFORM}" >&2

# Guard: server.json version matches Cargo.toml version
python3 -c 'import json, pathlib, sys; v=json.loads(pathlib.Path("server.json").read_text(encoding="utf-8")).get("version"); assert v, "server.json missing version"; print(v)' >/dev/null
SERVER_VER="$(python3 -c 'import json, pathlib; print(json.loads(pathlib.Path("server.json").read_text(encoding="utf-8")).get("version",""))')"
if [[ "$SERVER_VER" != "$VERSION" ]]; then
  echo "❌ Version mismatch: Cargo.toml=$VERSION server.json=$SERVER_VER" >&2
  exit 1
fi

mkdir -p "$DIST_DIR"
rm -f "$ASSET_DEFAULT" "$ASSET_HITL"

echo "== cargo build (default)" >&2
(
  cd mcp-server
  cargo build --release --locked --bin shadowcrawl --bin shadowcrawl-mcp
)

rm -rf "$DIST_DIR"/*
cp "mcp-server/target/release/shadowcrawl" "$DIST_DIR/shadowcrawl"
cp "mcp-server/target/release/shadowcrawl-mcp" "$DIST_DIR/shadowcrawl-mcp"
cp LICENSE README.md server.json "$DIST_DIR/"
echo "$VERSION" > "$DIST_DIR/VERSION"

tar -C "$DIST_DIR" -czf "$ASSET_DEFAULT" .
echo "✅ Created $ASSET_DEFAULT" >&2

echo "== cargo build (non_robot_search)" >&2
(
  cd mcp-server
  cargo build --release --locked --features non_robot_search --bin shadowcrawl --bin shadowcrawl-mcp
)

rm -rf "$DIST_DIR"/*
cp "mcp-server/target/release/shadowcrawl" "$DIST_DIR/shadowcrawl-non_robot_search"
cp "mcp-server/target/release/shadowcrawl-mcp" "$DIST_DIR/shadowcrawl-mcp-non_robot_search"
cp LICENSE README.md server.json "$DIST_DIR/"
echo "$VERSION" > "$DIST_DIR/VERSION"

tar -C "$DIST_DIR" -czf "$ASSET_HITL" .
echo "✅ Created $ASSET_HITL" >&2

echo "== Upload" >&2
if [[ $GH_OK -eq 1 ]]; then
  echo "Using gh CLI to upload assets to release ${TAG}" >&2
  gh release upload "$TAG" "$ASSET_DEFAULT" "$ASSET_HITL" --clobber
  echo "✅ Uploaded assets to GitHub Release $TAG" >&2
else
  cat >&2 <<EOF
⚠️  gh CLI not found, skipping automatic upload.

To upload manually:
- Open the GitHub Release for ${TAG}
- Upload:
  - ${ASSET_DEFAULT}
  - ${ASSET_HITL}

Or install gh:
- https://cli.github.com/
EOF
fi

echo "Done." >&2
