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

export CARGO_TERM_COLOR=always

DRY_RUN=false
[[ "${1:-}" == "--dry-run" ]] && DRY_RUN=true

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MCP="$REPO_ROOT/mcp-server"

# ── Read version ──────────────────────────────────────────────────────────────
VERSION=$(grep '^version' "$MCP/Cargo.toml" | head -1 | cut -d '"' -f2)
TAG="v$VERSION"

# Best-effort: use all cores for faster builds.
if command -v sysctl &>/dev/null; then
  export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-$(sysctl -n hw.ncpu 2>/dev/null || echo 8)}"
fi

pass()  { printf "\033[32m✅  %s\033[0m\n" "$*"; }
info()  { printf "\033[34m──  %s\033[0m\n" "$*"; }
warn()  { printf "\033[33m⚠️   %s\033[0m\n" "$*"; }
banner(){ printf "\n\033[1;36m=== %s ===\033[0m\n" "$*"; }

die() { printf "\033[31m❌  %s\033[0m\n" "$*" >&2; exit 1; }

repo_slug_from_origin() {
  # Supports:
  #   https://github.com/OWNER/REPO.git
  #   git@github.com:OWNER/REPO.git
  # Returns: OWNER/REPO
  local origin
  origin="$(git -C "$REPO_ROOT" remote get-url origin 2>/dev/null || true)"
  [[ -n "$origin" ]] || return 1

  origin="${origin%.git}"
  origin="${origin#https://github.com/}"
  origin="${origin#http://github.com/}"
  origin="${origin#git@github.com:}"

  # Reject anything that doesn't look like owner/repo.
  if [[ "$origin" =~ ^[^/]+/[^/]+$ ]]; then
    printf '%s' "$origin"
    return 0
  fi

  return 1
}

banner "ShadowCrawl $TAG — Local Release"
info "Repo: $REPO_ROOT"
info "MCP:  $MCP"
$DRY_RUN && warn "DRY-RUN mode — skipping GitHub upload"

# ── Preflight checks ──────────────────────────────────────────────────────────
banner "Preflight"
for cmd in cargo cargo-zigbuild zig rustup python3; do
  if ! command -v "$cmd" &>/dev/null; then
    printf "\033[31m❌  Missing: %s\033[0m\n" "$cmd" >&2
    case "$cmd" in
      zig|cargo-zigbuild) echo "   brew install zig && cargo install cargo-zigbuild" ;;
    esac
    exit 1
  fi
done
pass "All tools present"

if ! $DRY_RUN; then
  if ! command -v gh &>/dev/null; then
    die "Missing: gh (install: brew install gh)"
  fi
  if ! gh auth status -h github.com &>/dev/null; then
    die "GitHub CLI is not authenticated. Run: gh auth login"
  fi
fi

if [[ -z "$(git -C "$REPO_ROOT" rev-parse --is-inside-work-tree 2>/dev/null || true)" ]]; then
  die "Not a git repo: $REPO_ROOT"
fi

if [[ -n "$(git -C "$REPO_ROOT" status --porcelain)" ]]; then
  die "Working tree is dirty. Commit/stash changes before releasing."
fi

REPO_SLUG=""
if ! $DRY_RUN; then
  REPO_SLUG="$(repo_slug_from_origin || true)"
  [[ -n "$REPO_SLUG" ]] || die "Could not parse OWNER/REPO from 'origin' remote."
  info "GitHub repo: $REPO_SLUG"
fi

banner "Version Guard"
SERVER_VER="$(python3 -c 'import json, pathlib; obj=json.loads(pathlib.Path("server.json").read_text(encoding="utf-8")); print(obj.get("version",""))')"
[[ -n "$SERVER_VER" ]] || die "server.json missing version"
if [[ "$SERVER_VER" != "$VERSION" ]]; then
  die "Version mismatch: mcp-server/Cargo.toml=$VERSION server.json=$SERVER_VER"
fi
pass "Versions match ($VERSION)"

banner "Warm dependencies"
(cd "$MCP" && cargo fetch --locked)
pass "Cargo deps fetched"

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
gh release delete "$TAG" --repo "$REPO_SLUG" --yes 2>/dev/null || true
gh release create "$TAG" \
  "$DIST"/*.tar.gz \
  "$DIST"/*.zip \
  --title "ShadowCrawl $TAG" \
  --notes "Built locally from macOS (Apple Silicon) using cargo-zigbuild." \
  --repo "$REPO_SLUG"

pass "ALL DONE — ShadowCrawl $TAG is live on GitHub Releases"
