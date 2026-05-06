# RACKLOG · Homelab Inventory

A privacy-focused, container-native inventory ops platform for homelabs.
Bloomberg-style ops UI ([RACKLOG.html](static/RACKLOG.html)) backed by a
small, scalable Rust service.

> **Status:** under active implementation on
> `claude/implement-racklog-rust-dtKnX`. The public API is not yet stable.

## Why

Homelabs accumulate gear, cables, parts, and serials. Most off-the-shelf
inventory tools either ship your data offsite or are far too heavy. RACKLOG
keeps everything local by default, runs as a single binary, and packs into a
~30 MB container image you can drop into Docker, Compose, or Kubernetes.

## Highlights

- **Single static binary**, written in Rust (Axum + SQLx)
- **SQLite by default** — no external dependencies; switchable to Postgres
- **All data stays local** — no telemetry, no third-party calls at runtime
- **Tamper-evident audit log** — every mutation chained with SHA-256
- **Container-native** — Dockerfile, Compose, and Kubernetes manifests
- **Health & readiness probes** for Kubernetes
- **Optional bearer-token auth** for shared deployments
- **Strict default security headers** (CSP, X-Content-Type-Options,
  Referrer-Policy, etc.)

See [`docs/`](docs/) (incrementally added) and [`deploy/`](deploy/) for
deployment details.

## Quick start (local)

```sh
cargo run --release
# open http://127.0.0.1:8080
```

## License

Apache-2.0
