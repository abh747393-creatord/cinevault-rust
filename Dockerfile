# Multi-stage production build for CineVault Rust API on Render / Linux

# Stage 1: Builder
FROM rust:1.90-bookworm AS builder

WORKDIR /usr/src/app

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy dependency manifests
COPY Cargo.toml Cargo.lock ./

# Copy full source tree
COPY src ./src

# Build the optimized release binary for cinevault_server
RUN cargo build --release --bin cinevault_server

# Stage 2: Runtime
FROM debian:bookworm-slim AS runtime

# Install minimal runtime certificates and curl for health check
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create unprivileged system user
RUN groupadd -r cinevault && useradd -r -g cinevault -s /bin/false -d /app cinevault

WORKDIR /app

# Copy compiled binary from builder
COPY --from=builder /usr/src/app/target/release/cinevault_server /app/cinevault_server

# Set permissions
RUN chown -R cinevault:cinevault /app
USER cinevault

# Render injects PORT dynamically (typically 10000 on Render, 8080 default)
ENV PORT=8080
EXPOSE 8080

# Health check
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
  CMD curl -f http://localhost:${PORT}/health || exit 1

ENTRYPOINT ["/app/cinevault_server"]
