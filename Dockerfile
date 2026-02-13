# syntax=docker/dockerfile:1.7

# Build stage
FROM rust:bookworm AS builder

# Install system dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    g++ \
    binutils \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy manifests from mcp-server directory
COPY mcp-server/Cargo.toml ./
COPY mcp-server/Cargo.lock ./

# Create dummy sources to cache dependencies
RUN mkdir -p src/bin && echo "pub fn _dummy() {}" > src/lib.rs && echo "fn main() {}" > src/main.rs && echo "fn main() {}" > src/bin/mcp-stdio.rs

# Build dependencies (cache registry & git, not target)
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo build --release --locked --bin shadowcrawl --bin shadowcrawl-mcp
RUN rm -rf src

# Copy source code from mcp-server directory (including subdirectories)
# Source includes optimizations:
# - Parallel scraping with concurrent limiting (batch_scrape.rs)
# - Markdown content cleaner (rust_scraper.rs)
# - Semantic reranking of search results (rerank.rs)
# - Anti-bot protection with stealth headers (antibot.rs)
COPY mcp-server/src/ ./src/

# Force source mtimes forward to avoid stale fingerprint reuse from dummy bootstrap build
RUN find src -type f -exec touch {} +

# Ensure dummy bootstrap binaries cannot leak into the runtime image
RUN rm -f /app/target/release/shadowcrawl /app/target/release/shadowcrawl-mcp

# Build application (cache registry & git)
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo build --release --locked --bin shadowcrawl --bin shadowcrawl-mcp

# Guardrail: fail build if binaries are suspiciously small (dummy build output)
RUN test -x /app/target/release/shadowcrawl && test -x /app/target/release/shadowcrawl-mcp && \
    test "$(stat -c%s /app/target/release/shadowcrawl)" -gt 5000000 && \
    test "$(stat -c%s /app/target/release/shadowcrawl-mcp)" -gt 5000000

# Strip binaries to reduce size
RUN strip /app/target/release/shadowcrawl /app/target/release/shadowcrawl-mcp || true

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create app user and cache directories
RUN useradd -r -s /bin/false appuser && \
    mkdir -p /home/appuser/.cache/fastembed /home/appuser/.cache/huggingface && \
    chown -R appuser:appuser /home/appuser

# Copy binaries from builder stage
COPY --from=builder /app/target/release/shadowcrawl /usr/local/bin/shadowcrawl
COPY --from=builder /app/target/release/shadowcrawl-mcp /usr/local/bin/shadowcrawl-mcp

# Change ownership
RUN chown appuser:appuser /usr/local/bin/shadowcrawl /usr/local/bin/shadowcrawl-mcp

# Switch to app user
USER appuser

# Expose port
EXPOSE 5000

# Set environment variables
ENV RUST_LOG=info
ENV SEARXNG_URL=http://searxng:8080
ENV FASTEMBED_CACHE_DIR=/home/appuser/.cache/fastembed
ENV HF_HOME=/home/appuser/.cache/huggingface

# Start the application
CMD ["shadowcrawl"]

HEALTHCHECK --interval=30s --timeout=5s --start-period=15s --retries=3 \
    CMD curl -fsS http://localhost:5000/health || exit 1