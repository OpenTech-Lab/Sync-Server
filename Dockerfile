# ──────────────────────────────────────────────
# Stage 1: Build the Rust binary
# ──────────────────────────────────────────────
FROM rust:1.85-alpine AS builder

# Required for diesel/pq-sys (libpq) and ring (C toolchain)
RUN apk add --no-cache musl-dev openssl-dev openssl-libs-static pkgconfig

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
FROM alpine:3.21 AS runtime

RUN apk add --no-cache ca-certificates libgcc

# Non-root user for the process
RUN addgroup -S sync && adduser -S sync -G sync

WORKDIR /app

COPY --from=builder /build/target/release/sync-server /app/sync-server

USER sync

EXPOSE 8080

ENTRYPOINT ["/app/sync-server"]
