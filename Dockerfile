# syntax=docker/dockerfile:1.7
#
# Multi-stage build for the RACKLOG Rust service.
#
#   stage 1 (planner)  — captures the dependency graph for cache reuse
#   stage 2 (builder)  — compiles the release binary
#   stage 3 (runtime)  — minimal Debian slim image with the binary only
#
# The result is a small, root-less image that contains only the binary
# and the migrations directory. Frontend assets and the seed dataset are
# embedded into the binary via include_bytes!/include_str!, so no extra
# files need to land on the host.

ARG RUST_VERSION=1.83
ARG DEBIAN_VERSION=bookworm

# ---------- planner --------------------------------------------------------
FROM rust:${RUST_VERSION}-${DEBIAN_VERSION} AS planner
WORKDIR /app
RUN cargo install --locked cargo-chef
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ---------- builder --------------------------------------------------------
FROM rust:${RUST_VERSION}-${DEBIAN_VERSION} AS builder
WORKDIR /app
RUN cargo install --locked cargo-chef
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo build --release --locked
RUN strip target/release/racklog

# ---------- runtime --------------------------------------------------------
FROM debian:${DEBIAN_VERSION}-slim AS runtime

# Run as a dedicated non-root user. Persistent data lives in /data so
# operators can mount whatever volume / PVC they like.
RUN groupadd --system --gid 10001 racklog \
 && useradd --system --uid 10001 --gid 10001 --home /home/racklog --create-home racklog \
 && apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && mkdir -p /data \
 && chown -R racklog:racklog /data

COPY --from=builder /app/target/release/racklog /usr/local/bin/racklog

USER racklog
WORKDIR /home/racklog

ENV RACKLOG_BIND=0.0.0.0:8080 \
    RACKLOG_DATA_DIR=/data \
    RACKLOG_LOG_FORMAT=json

EXPOSE 8080
VOLUME ["/data"]

# Tiny shell-less healthcheck via the Rust binary's HTTP probe. We avoid
# embedding curl/wget to keep the runtime image tight; the orchestrator's
# native HTTP probe (Kubernetes httpGet, Compose healthcheck below) is the
# preferred path. Compose sets its own healthcheck override.

ENTRYPOINT ["/usr/local/bin/racklog"]
