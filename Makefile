# Makefile — local CI that mirrors .github/workflows/release.yml
# Run `make ci` before pushing; it replicates every step the GitHub Action executes.
#
# Usage:
#   make env          – check all required tools are present
#   make ci           – full local CI (lint + build + smoke test)
#   make build        – default binary (current host target)
#   make build-nrs    – binary with non_robot_search feature
#   make lint         – clippy (-D warnings) + rustfmt check
#   make validate     – server.json, smithery.yaml, config-schema validation
#   make smoke        – quick HTTP smoke-test (server must already be running on :5000)
#   make release      – local macOS release build + package (same as local_release.sh)
#   make clean        – cargo clean
#
# All targets set OPENSSL_STATIC/OPENSSL_VENDOR so the vendored OpenSSL feature
# is guaranteed to be picked up on every OS (matches the CI env vars).

SHELL        := /usr/bin/env bash
export OPENSSL_STATIC  := 1
export OPENSSL_VENDOR  := 1

ROOT         := $(shell pwd)
CARGO_DIR    := $(ROOT)/mcp-server
BINS         := --bin shadowcrawl --bin shadowcrawl-mcp

# ── colour helpers ──────────────────────────────────────────────────────────
GREEN  := \033[0;32m
YELLOW := \033[0;33m
RED    := \033[0;31m
RESET  := \033[0m

.DEFAULT_GOAL := help

# ────────────────────────────────────────────────────────────────────────────
# help
# ────────────────────────────────────────────────────────────────────────────
.PHONY: help
help:
	@echo ""
	@echo "  ShadowCrawl local CI  (mirrors .github/workflows/release.yml)"
	@echo ""
	@echo "  make env          check required tools"
	@echo "  make ci           full pipeline: env + validate + lint + build + build-nrs"
	@echo "  make build        cargo build --release (default features)"
	@echo "  make build-nrs    cargo build --release --features non_robot_search"
	@echo "  make lint         clippy (-D warnings) + fmt check"
	@echo "  make validate     validate server.json / smithery.yaml / config-schema"
	@echo "  make smoke        HTTP smoke-test (requires server running on :5000)"
	@echo "  make release      macOS release package  (same as ./local_release.sh)"
	@echo "  make clean        cargo clean"
	@echo ""

# ────────────────────────────────────────────────────────────────────────────
# env — replicate 'Install protoc' + toolchain checks from release.yml
# ────────────────────────────────────────────────────────────────────────────
.PHONY: env
env:
	@echo -e "$(YELLOW)==> Checking environment$(RESET)"
	@command -v cargo   >/dev/null 2>&1 || { echo -e "$(RED)❌ cargo not found$(RESET)"; exit 1; }
	@command -v rustup  >/dev/null 2>&1 || { echo -e "$(RED)❌ rustup not found$(RESET)"; exit 1; }
	@command -v python3 >/dev/null 2>&1 || { echo -e "$(RED)❌ python3 not found$(RESET)"; exit 1; }
	@if ! command -v protoc >/dev/null 2>&1; then \
	  echo -e "$(YELLOW)⚠️  protoc not found — installing...$(RESET)"; \
	  case "$$(uname -s)" in \
	    Darwin) brew install protobuf ;; \
	    Linux)  sudo apt-get update -qq && sudo apt-get install -y protobuf-compiler ;; \
	    *)      echo -e "$(RED)❌ Cannot auto-install protoc on $$(uname -s). Install manually.$(RESET)"; exit 1 ;; \
	  esac; \
	fi
	@echo -e "$(GREEN)✅ protoc $$(protoc --version)$(RESET)"
	@echo -e "$(GREEN)✅ cargo  $$(cargo --version)$(RESET)"
	@echo -e "$(GREEN)✅ rustup $$(rustup --version 2>&1 | head -1)$(RESET)"
	@echo -e "$(GREEN)✅ rust   $$(rustc --version)$(RESET)"
	@echo -e "$(GREEN)✅ python $$(python3 --version)$(RESET)"
	@echo -e "$(GREEN)==> Environment OK$(RESET)"

# ────────────────────────────────────────────────────────────────────────────
# validate — mirrors check-trigger validation steps in release.yml
# ────────────────────────────────────────────────────────────────────────────
.PHONY: validate
validate:
	@echo -e "$(YELLOW)==> Validating metadata$(RESET)"
	@python3 ci/validate.py
	@echo -e "$(GREEN)==> Validation passed$(RESET)"

# ────────────────────────────────────────────────────────────────────────────
# lint — mirrors the spirit of CI quality gates
# ────────────────────────────────────────────────────────────────────────────
.PHONY: lint
lint: env
	@echo -e "$(YELLOW)==> clippy (all features)$(RESET)"
	cd $(CARGO_DIR) && cargo clippy --features non_robot_search $(BINS) -- -D warnings
	@echo -e "$(YELLOW)==> rustfmt check$(RESET)"
	cd $(CARGO_DIR) && cargo fmt --check
	@echo -e "$(GREEN)==> Lint passed$(RESET)"

# ────────────────────────────────────────────────────────────────────────────
# build — mirrors 'Build (release)' step in release.yml
# ────────────────────────────────────────────────────────────────────────────
.PHONY: build
build: env
	@echo -e "$(YELLOW)==> cargo build --release (default)$(RESET)"
	cd $(CARGO_DIR) && cargo build --release --locked $(BINS)
	@echo -e "$(GREEN)==> Build OK: target/release/shadowcrawl, target/release/shadowcrawl-mcp$(RESET)"

# ────────────────────────────────────────────────────────────────────────────
# build-nrs — mirrors 'Build (release, non_robot_search)' step
# ────────────────────────────────────────────────────────────────────────────
.PHONY: build-nrs
build-nrs: env
	@echo -e "$(YELLOW)==> cargo build --release --features non_robot_search$(RESET)"
	cd $(CARGO_DIR) && cargo build --release --locked --features non_robot_search $(BINS)
	@echo -e "$(GREEN)==> non_robot_search build OK$(RESET)"

# ────────────────────────────────────────────────────────────────────────────
# smoke — quick HTTP tool test (server must already be running on port 5000)
# ────────────────────────────────────────────────────────────────────────────
.PHONY: smoke
smoke:
	@echo -e "$(YELLOW)==> Smoke test (localhost:5000)$(RESET)"
	@curl -fsS http://localhost:5000/health >/dev/null && echo "✅ /health OK" || \
	  { echo -e "$(RED)❌ Server not reachable on :5000. Run: ./mcp-server/target/release/shadowcrawl --port 5000$(RESET)"; exit 1; }
	@python3 ci/smoke.py
	@echo -e "$(GREEN)==> Smoke test passed$(RESET)"

# ────────────────────────────────────────────────────────────────────────────
# ci — full pipeline: env + validate + lint + build + build-nrs
#      Run this locally before every `git push` with [build] in commit message.
# ────────────────────────────────────────────────────────────────────────────
.PHONY: ci
ci: env validate lint build build-nrs
	@echo ""
	@echo -e "$(GREEN)╔══════════════════════════════════════════════╗$(RESET)"
	@echo -e "$(GREEN)║  ✅  Local CI passed — safe to push to GH   ║$(RESET)"
	@echo -e "$(GREEN)╚══════════════════════════════════════════════╝$(RESET)"
	@echo ""

# ────────────────────────────────────────────────────────────────────────────
# release — macOS local release (same as ./local_release.sh)
# ────────────────────────────────────────────────────────────────────────────
.PHONY: release
release:
	bash local_release.sh

# ────────────────────────────────────────────────────────────────────────────
# clean
# ────────────────────────────────────────────────────────────────────────────
.PHONY: clean
clean:
	cd $(CARGO_DIR) && cargo clean
	rm -rf dist/
