# syntax=docker/dockerfile:1.7
#
# Multi-stage build for the RACKLOG Rust service.
#
#   stage 1 (planner)  — captures the dependency graph for cache reuse
#   stage 2 (builder)  — compiles the release binary + assembles /data
#   stage 3 (runtime)  — distroless cc-debian12: libc + libssl + ca-certs
#                        only, no shell, no package manager, no apt.
#
# The runtime layer carries only the things the Rust binary actually
# links against (glibc, libgcc, libssl), the trust store, and tzdata.
# Frontend assets, migrations, and the seed dataset are embedded into
# the binary via include_bytes!/include_str! so the runtime image needs
# nothing on the host except a data volume.

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

# Pre-create the writable data directory with the right ownership so
# the distroless stage can COPY it in. Distroless has no `mkdir` or
# `chown`, so all filesystem layout has to happen in the builder.
RUN mkdir -p /out/data && chown -R 10001:10001 /out/data

# ---------- runtime --------------------------------------------------------
# gcr.io/distroless/cc-debian12 is the smallest image that still
# carries glibc, libgcc, libssl3 and a populated CA store — exactly the
# set our reqwest+rustls stack needs for outbound TLS to the SKU
# providers. It has no shell, no package manager, no setuid binaries,
# and no /etc/passwd writes. Pin by digest at deploy time:
#
#   docker pull gcr.io/distroless/cc-debian12:nonroot
#   docker images --digests gcr.io/distroless/cc-debian12
#   # then replace the tag below with @sha256:...
#
# Trivy in CI catches any known CVEs in the pinned digest.
FROM gcr.io/distroless/cc-debian12:nonroot AS runtime

COPY --from=builder /app/target/release/racklog /usr/local/bin/racklog
COPY --from=builder --chown=10001:10001 /out/data /data

# UID 10001 is the same numeric identity our Kubernetes manifest +
# docker-compose run as. Distroless doesn't ship /etc/passwd entries
# beyond `nonroot` (UID 65532), but the kernel only cares about the
# numeric UID, not the name. Specifying it here keeps `docker run`
# (without K8s overrides) consistent with the rest of the deploy.
USER 10001:10001
WORKDIR /data

ENV RACKLOG_BIND=0.0.0.0:8080 \
    RACKLOG_DATA_DIR=/data \
    RACKLOG_LOG_FORMAT=json

EXPOSE 8080
VOLUME ["/data"]

# Healthcheck is intentionally not embedded — distroless has no shell
# and no curl. Both Kubernetes (httpGet probes) and docker-compose
# (its own healthcheck stanza) talk HTTP directly. See
# deploy/kubernetes/deployment.yaml + docker-compose.yml.

ENTRYPOINT ["/usr/local/bin/racklog"]
