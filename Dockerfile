# ──────────────────────────────────────────────
# Stage 1: Build the Rust binary
# ──────────────────────────────────────────────
FROM rust:1.88-bookworm AS builder

# Required for diesel/pq-sys (libpq) and ring (C toolchain)
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
      build-essential \
      pkg-config \
      libssl-dev \
      libpq-dev \
      ca-certificates && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Cache dependency compilation separately from source
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs && \
    cargo build --release && \
    rm -rf src

# Now compile actual source
COPY src ./src
COPY migrations ./migrations
RUN touch src/main.rs && cargo build --release

# ──────────────────────────────────────────────
# Stage 2: Minimal runtime image
# ──────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
      ca-certificates \
      wget \
      libpq5 && \
    rm -rf /var/lib/apt/lists/*

# Non-root user for the process
RUN groupadd --system appuser && useradd --system --gid appuser appuser

WORKDIR /app

COPY --from=builder /build/target/release/sync-server /app/sync-server

USER appuser

EXPOSE 8080

ENTRYPOINT ["/app/sync-server"]
