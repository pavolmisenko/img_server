# syntax=docker/dockerfile:1

# ── Build ──────────────────────────────────────────────────────────────────────
FROM rust:1.87-bookworm AS builder

WORKDIR /build

# Cache dependency compilation separately from application code.
# Cargo compiles all crates listed in Cargo.toml on this layer; only source
# changes in the next layer trigger an incremental recompile.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && \
    echo 'fn main() {}' > src/main.rs && \
    touch src/svg_process.rs && \
    cargo build --release && \
    rm src/main.rs src/svg_process.rs

COPY src ./src
RUN cargo build --release

# ── Runtime ────────────────────────────────────────────────────────────────────
FROM debian:bookworm-slim

# fonts-dejavu-core  — required by usvg/resvg for SVG text rendering
# ca-certificates    — required by reqwest/rustls for TLS to api.open-meteo.com
# curl               — used by the HEALTHCHECK
RUN apt-get update && apt-get install -y --no-install-recommends \
        fonts-dejavu-core \
        ca-certificates \
        curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /build/target/release/img_server ./
COPY weather_template.svg descriptions.json ./
COPY icons/ ./icons/

EXPOSE 3000

# RUST_LOG controls tracing output; override at runtime if needed
ENV RUST_LOG=img_server=info

HEALTHCHECK --interval=30s --timeout=10s --start-period=15s --retries=3 \
    CMD curl -sf http://localhost:3000/health || exit 1

CMD ["./img_server"]
