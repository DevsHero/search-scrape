# Build stage
FROM rustlang/rust:nightly-bookworm AS builder

# Install system dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy manifests from mcp-server directory
COPY mcp-server/Cargo.toml ./

# Create dummy main to cache dependencies
RUN mkdir -p src/bin && echo "fn main() {}" > src/main.rs && echo "fn main() {}" > src/bin/mcp-stdio.rs

# Build dependencies
RUN cargo build --release
RUN rm -rf src

# Copy source code from mcp-server directory (including subdirectories)
# Source includes optimizations:
# - Parallel scraping with concurrent limiting (batch_scrape.rs)
# - Markdown content cleaner (rust_scraper.rs)
# - Semantic reranking of search results (rerank.rs)
# - Anti-bot protection with stealth headers (antibot.rs)
COPY mcp-server/src/ ./src/

# Build application
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create app user and cache directories
RUN useradd -r -s /bin/false appuser && \
    mkdir -p /home/appuser/.cache/fastembed /home/appuser/.cache/huggingface && \
    chown -R appuser:appuser /home/appuser

# Copy binaries from builder stage
COPY --from=builder /app/target/release/mcp-server /usr/local/bin/mcp-server
COPY --from=builder /app/target/release/search-scrape-mcp /usr/local/bin/search-scrape-mcp

# Change ownership
RUN chown appuser:appuser /usr/local/bin/mcp-server /usr/local/bin/search-scrape-mcp

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
CMD ["mcp-server"]