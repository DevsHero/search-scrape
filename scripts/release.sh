#!/usr/bin/env bash
# =============================================================================
# scripts/release.sh — Sovereign Local Release (macOS Apple Silicon)
#
# Builds ShadowCrawl for all platforms from your Mac and uploads to GitHub.
#
# Prerequisites (run once):
#   brew install gh zig
#   gh auth login
#   cargo install cargo-zigbuild
#   rustup target add \
#     aarch64-apple-darwin \
#     aarch64-unknown-linux-gnu \
#     x86_64-pc-windows-gnullvm \
#     aarch64-pc-windows-gnullvm
#
# Usage:
#   bash scripts/release.sh            # build all + upload
#   bash scripts/release.sh --dry-run  # build only, skip upload
# =============================================================================
set -euo pipefail

DRY_RUN=false
[[ "${1:-}" == "--dry-run" ]] && DRY_RUN=true

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MCP="$REPO_ROOT/mcp-server"

# ── Read version ──────────────────────────────────────────────────────────────
VERSION=$(grep '^version' "$MCP/Cargo.toml" | head -1 | cut -d '"' -f2)
TAG="v$VERSION"

pass()  { printf "\033[32m✅  %s\033[0m\n" "$*"; }
info()  { printf "\033[34m──  %s\033[0m\n" "$*"; }
warn()  { printf "\033[33m⚠️   %s\033[0m\n" "$*"; }
banner(){ printf "\n\033[1;36m=== %s ===\033[0m\n" "$*"; }

banner "ShadowCrawl $TAG — Local Release"
info "Repo: $REPO_ROOT"
info "MCP:  $MCP"
$DRY_RUN && warn "DRY-RUN mode — skipping GitHub upload"

# ── Preflight checks ──────────────────────────────────────────────────────────
banner "Preflight"
for cmd in cargo cargo-zigbuild gh zig rustup; do
  if ! command -v "$cmd" &>/dev/null; then
    printf "\033[31m❌  Missing: %s\033[0m\n" "$cmd"
    case "$cmd" in
      zig|cargo-zigbuild) echo "   brew install zig && cargo install cargo-zigbuild" ;;
      gh) echo "   brew install gh && gh auth login" ;;
    esac
    exit 1
  fi
done
pass "All tools present"

# ── Tag management ────────────────────────────────────────────────────────────
banner "Tagging $TAG"
git -C "$REPO_ROOT" tag -d "$TAG" 2>/dev/null && info "Deleted local tag $TAG" || true
git -C "$REPO_ROOT" push origin ":refs/tags/$TAG" 2>/dev/null && info "Deleted remote tag $TAG" || true
git -C "$REPO_ROOT" tag "$TAG"
git -C "$REPO_ROOT" push origin "$TAG"
pass "Tag $TAG created and pushed"

# ── Build targets ─────────────────────────────────────────────────────────────
banner "Building"
TARGETS=(
  "aarch64-apple-darwin"
  "aarch64-unknown-linux-gnu"
  "x86_64-pc-windows-gnullvm"
  "aarch64-pc-windows-gnullvm"
)
# NOTE: x86_64-unknown-linux-gnu is intentionally excluded.
# The lancedb/lance crate ships a pre-built AVX512 C archive that Zig's lld
# cannot resolve when cross-compiling from macOS. To build linux-x64, run
# this script inside a GitHub Actions ubuntu runner or Docker linux/amd64.
LINUX_X64_NOTE="linux-x64 skipped (lancedb AVX512 cross-compile limitation — build on native linux or GitHub Actions)"

cd "$MCP"

for target in "${TARGETS[@]}"; do
  info "Building $target ..."
  case "$target" in
    aarch64-apple-darwin)
      # Native macOS build — no zigbuild needed
      cargo build --release --locked --target "$target" --bin shadowcrawl --bin shadowcrawl-mcp
      ;;
    *)
      # Everything else via zigbuild (linux-arm64, windows-x64, windows-arm64)
      cargo zigbuild --release --locked --target "$target" --bin shadowcrawl --bin shadowcrawl-mcp
      ;;
  esac
  pass "Built $target"
done

# ── Package ───────────────────────────────────────────────────────────────────
banner "Packaging"
DIST="$REPO_ROOT/dist"
rm -rf "$DIST" && mkdir -p "$DIST"

package_tar() {
  local target="$1" platform="$2"
  local src="$MCP/target/$target/release"
  local dir="$DIST/shadowcrawl-$VERSION-$platform"
  mkdir -p "$dir"
  cp "$src/shadowcrawl"     "$dir/"
  cp "$src/shadowcrawl-mcp" "$dir/"
  cp "$REPO_ROOT/LICENSE" "$REPO_ROOT/README.md" "$REPO_ROOT/server.json" "$dir/"
  echo "$VERSION" > "$dir/VERSION"
  tar -C "$dir" -czf "$DIST/shadowcrawl-$VERSION-$platform.tar.gz" .
  rm -rf "$dir"
  pass "Packaged $platform.tar.gz"
}

package_zip() {
  local target="$1" platform="$2"
  local src="$MCP/target/$target/release"
  local dir="$DIST/shadowcrawl-$VERSION-$platform"
  mkdir -p "$dir"
  cp "$src/shadowcrawl.exe"     "$dir/"
  cp "$src/shadowcrawl-mcp.exe" "$dir/"
  cp "$REPO_ROOT/LICENSE" "$REPO_ROOT/README.md" "$REPO_ROOT/server.json" "$dir/"
  echo "$VERSION" > "$dir/VERSION"
  (cd "$dir" && zip -qr "$DIST/shadowcrawl-$VERSION-$platform.zip" .)
  rm -rf "$dir"
  pass "Packaged $platform.zip"
}

package_tar "aarch64-apple-darwin"           "macos-arm64"
package_tar "aarch64-unknown-linux-gnu"      "linux-arm64"
package_zip "x86_64-pc-windows-gnullvm"      "windows-x64"
package_zip "aarch64-pc-windows-gnullvm"     "windows-arm64"
warn "$LINUX_X64_NOTE"

info "Artifacts:"
ls -lh "$DIST/"

# ── GitHub Release ────────────────────────────────────────────────────────────
if $DRY_RUN; then
  warn "Dry-run: skipping GitHub release upload"
  exit 0
fi

banner "Uploading to GitHub Release $TAG"
gh release delete "$TAG" --repo "$REPO_ROOT" --yes 2>/dev/null || true
gh release create "$TAG" \
  "$DIST"/*.tar.gz \
  "$DIST"/*.zip \
  --title "ShadowCrawl $TAG" \
  --notes "Built locally from macOS (Apple Silicon) using cargo-zigbuild." \
  --repo "$(git -C "$REPO_ROOT" remote get-url origin)"

pass "ALL DONE — ShadowCrawl $TAG is live on GitHub Releases"
