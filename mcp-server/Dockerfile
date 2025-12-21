FROM rust:1.82 as builder

# Install system dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Create app directory
WORKDIR /app

# Copy manifests
COPY Cargo.toml ./

# Create a dummy main.rs to build dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs

# Build dependencies
RUN cargo build --release
RUN rm src/main.rs

# Copy source code
COPY src ./src

# Build application
RUN touch src/main.rs && cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create app user
RUN useradd -r -s /bin/false appuser

# Copy binary from builder stage
COPY --from=builder /app/target/release/mcp-server /usr/local/bin/mcp-server

# Change ownership
RUN chown appuser:appuser /usr/local/bin/mcp-server

# Switch to app user
USER appuser

# Expose port
EXPOSE 5000

# Set environment variables
ENV RUST_LOG=info
ENV SEARXNG_URL=http://searxng:8080

# Start the application
CMD ["mcp-server"]