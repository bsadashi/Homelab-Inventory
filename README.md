# RACKLOG · Homelab Inventory

A privacy-focused, container-native inventory ops platform for homelabs.
Bloomberg-style ops UI ([RACKLOG.html](static/RACKLOG.html)) backed by a
small, scalable Rust service.

> Built from the [Claude Design](https://claude.ai/design) prototype that
> shipped in `inventory-manager/` and re-implemented as a real, deployable
> stack on branch `claude/implement-racklog-rust-dtKnX`.

## Why

Homelabs accumulate gear, cables, parts, and serials. Most off-the-shelf
inventory tools either ship your data offsite or are far too heavy.
RACKLOG keeps everything local by default, runs as a single binary, and
packs into a small container you can drop into Docker, Compose, or
Kubernetes.

## Highlights

- **Single static Rust binary** (Axum + SQLx) — embeds the dashboard
  bundle and seed dataset via `include_bytes!` so the runtime image
  needs nothing on the host beyond a data volume.
- **Local-first by default** — SQLite ships in-process; no network
  egress at runtime. Swap `DATABASE_URL` to Postgres when you outgrow
  one replica.
- **Tamper-evident audit log** — every mutation flows through a
  SHA-256 chain; the boot path verifies it and `/api/activity/verify`
  reproduces it on demand. Constant-time comparison via `subtle`.
- **Optional bearer-token auth** (`RACKLOG_AUTH_TOKEN`) gating every
  `/api` write; comparison is constant time so the token can't leak
  via response timing.
- **Strict default security headers** — `X-Content-Type-Options`,
  `X-Frame-Options DENY`, `Referrer-Policy no-referrer`,
  `Permissions-Policy` locking down geolocation/microphone/camera,
  `Strict-Transport-Security` ready for TLS termination.
- **Container-native** — multi-stage Dockerfile, hardened compose file
  (read-only fs, dropped caps, no-new-privileges), Kubernetes manifests
  with probes and NetworkPolicy.

## Quick start

### Local

```sh
cargo run --release
# open http://127.0.0.1:8080
```

The dashboard mounts at `/`. The JSON API lives under `/api/*`.

### Docker

```sh
docker compose up --build
# open http://127.0.0.1:8080
```

The compose file binds to `127.0.0.1` only, runs as UID 10001, mounts
the root filesystem read-only, and persists the SQLite WAL on a named
volume. See `docker-compose.yml` for the full security profile.

### Kubernetes

```sh
docker build -t ghcr.io/your-user/racklog:0.1.0 .
docker push  ghcr.io/your-user/racklog:0.1.0

# edit deploy/kubernetes/kustomization.yaml → images.newTag
kubectl apply -k deploy/kubernetes/

# create the bearer token (optional)
kubectl create secret generic racklog-secret -n racklog \
  --from-literal=RACKLOG_AUTH_TOKEN="$(openssl rand -base64 48)"

kubectl port-forward -n racklog svc/racklog 8080:80
```

See [`deploy/kubernetes/README.md`](deploy/kubernetes/README.md) for the
full breakdown of every manifest.

## API surface

All endpoints are JSON. Authenticated endpoints require
`Authorization: Bearer $RACKLOG_AUTH_TOKEN` when the env var is set.
List endpoints accept `?limit=N&offset=M` (defaults sized for homelab
catalogs; capped to keep responses bounded).

### Catalog & ops

| Method | Path                       | Purpose                            |
|--------|----------------------------|------------------------------------|
| GET    | `/api/healthz`             | Liveness probe (always public)     |
| GET    | `/api/readyz`              | Readiness probe — pings the DB     |
| GET    | `/api/bootstrap`           | Full dashboard hydration snapshot  |
| GET    | `/api/stats`               | Aggregate KPIs (SKUs, value, …)    |
| CRUD   | `/api/items[/{id}]`        | Catalog with stock / variants / lots |
| CRUD   | `/api/locations[/{id}]`    | Rack → shelf → bin hierarchy       |
| CRUD   | `/api/suppliers[/{id}]`    | Vendors (open-PO count computed)   |
| CRUD   | `/api/pos[/{id}]`          | Purchase orders + lines            |
| CRUD   | `/api/sos[/{id}]`          | Pick / sales orders + lines        |
| CRUD   | `/api/transfers[/{id}]`    | Bin-to-bin movement                |
| CRUD   | `/api/counts[/{id}]`       | Cycle counts and audits            |
| GET    | `/api/activity?limit=200`  | Tamper-evident audit log           |
| GET    | `/api/activity/verify`     | Re-runs the SHA-256 chain check    |
| GET/POST | `/api/exports/items.csv` | Bulk CSV export / import           |
| GET    | `/api/exports/activity.csv` | Audit log as CSV                   |
| GET    | `/api/lookup/{barcode}`    | Local + external SKU lookup        |
| GET    | `/api/lookup/providers`    | List enabled external providers    |

### Server administration

All admin endpoints sit behind the same bearer-token gate. Mutating
endpoints additionally require `?confirm=1` to keep an accidental
curl with no body from triggering a heavy operation.

| Method | Path                                   | Purpose                                        |
|--------|----------------------------------------|------------------------------------------------|
| GET    | `/api/admin/info`                      | Version, uptime, DB path, pool stats, audit head |
| GET    | `/api/admin/stats`                     | Per-table row counts, on-disk DB + WAL bytes   |
| GET    | `/api/admin/integrity`                 | `PRAGMA integrity_check` + audit chain verification |
| GET    | `/api/admin/whoami`                    | Auth state + server clock (clock-skew check)   |
| GET    | `/api/admin/metrics`                   | Prometheus text format                         |
| POST   | `/api/admin/vacuum?confirm=1`          | Reclaim space; rewrites the DB file            |
| POST   | `/api/admin/wal_checkpoint?confirm=1`  | Flush WAL into main DB and truncate to 0 bytes |
| POST   | `/api/admin/retain_activity?days=N&confirm=1` | Prune activity rows older than N days   |

`retain_activity` records the deleted-tail's last hash as an *anchor*
in the response and writes the retention itself into the audit log
pointing at that anchor — so the truncation is itself verifiable.
After a prune `verify_chain` reports `broken_at: 0` because the new
first row's `prev_hash` references a row that's no longer present;
that's the documented retention behaviour, not corruption.

## Configuration

All configuration is sourced from the environment.

| Variable                         | Default                            | Notes                                  |
|----------------------------------|------------------------------------|----------------------------------------|
| `RACKLOG_BIND`                   | `0.0.0.0:8080`                     | `host:port` for the HTTP listener      |
| `RACKLOG_DATA_DIR`               | `./data`                           | Directory holding `racklog.db`         |
| `DATABASE_URL`                   | `sqlite://$DATA_DIR/racklog.db?mode=rwc` | sqlx URL                         |
| `RACKLOG_STATIC_DIR`             | _(unset)_                          | Override for hot-reloading the UI      |
| `RACKLOG_AUTH_TOKEN`             | _(unset)_                          | Enables Bearer auth on `/api`          |
| `RACKLOG_ALLOW_ORIGIN`           | _(unset → same-origin)_            | Comma-separated CORS allowlist         |
| `RACKLOG_REQUEST_TIMEOUT_SECS`   | `30`                               | Per-request timeout                    |
| `RACKLOG_MAX_BODY_BYTES`         | `2097152`                          | Upload limit (2 MiB)                   |
| `RACKLOG_LOG_FORMAT`             | `pretty`                           | `pretty` or `json`                     |
| `RACKLOG_SEED_ON_EMPTY`          | `true`                             | Load demo dataset when DB is empty     |
| `RACKLOG_LOG`                    | _(see code)_                       | `tracing-subscriber` filter            |
| `RACKLOG_LOOKUP_RATE_PER_MIN`    | `30`                               | Rate limit on `/api/lookup`            |
| `RACKLOG_LOOKUP_BURST`           | _(= rate)_                         | Burst capacity for the limiter         |
| `RACKLOG_UPCITEMDB_DISABLED`     | _(unset)_                          | Set to `1` to disable UPCitemDB        |
| `DIGIKEY_CLIENT_ID` / `DIGIKEY_CLIENT_SECRET` | _(unset)_              | Enables DigiKey provider               |
| `DIGIKEY_LOCALE_SITE` / `_LANGUAGE` / `_CURRENCY` | `US` / `en` / `USD` | DigiKey locale headers                 |

## Privacy posture

- **No telemetry.** The binary makes no outbound network calls at
  runtime *except* to the SKU-lookup providers an operator has
  explicitly configured (UPCitemDB and/or DigiKey). Both are
  opt-in: UPCitemDB is on by default for homelab convenience but
  disabled with `RACKLOG_UPCITEMDB_DISABLED=1`; DigiKey requires
  client credentials.
- **Browser-side third-party resources.** The dashboard HTML
  references Google Fonts and React/Babel/ZXing from `unpkg.com`
  via the user's browser, never the server. SRI hashes mitigate
  compromise but not availability. To eliminate the third-party
  hop entirely, vendor the assets into `static/` and update the
  script tags in `static/RACKLOG.html`.
- **Local-first by default.** SQLite lives on a volume you own.
  For encryption at rest, use disk encryption (LUKS, K8s storage
  class with encryption, etc.); the schema is the same.
- **Audit trail you can verify.** `/api/activity/verify` reproduces
  the full SHA-256 chain (canonical-JSON v2 encoding). Pin the
  `head` hash externally to detect rollback. `/api/admin/retain_activity`
  emits an *anchor hash* when pruning so truncations themselves
  remain provably timestamped.
- **Bearer token in localStorage.** When `RACKLOG_AUTH_TOKEN` is
  set, the dashboard stores its token in `localStorage` so it
  survives reloads. This is XSS-readable; for sensitive deployments
  put the service behind a reverse proxy with stricter session
  semantics or only expose `/api` and skip the dashboard.
- **Defaults err strict.** Same-origin CORS, every defensive
  header on by default, capabilities dropped in containers,
  network policy locks egress to in-cluster DNS only.

## Repository layout

```
.
├── Cargo.toml          · Rust crate manifest
├── Dockerfile          · multi-stage build → debian-slim runtime
├── docker-compose.yml  · hardened single-container deployment
├── deploy/kubernetes/  · namespace, configmap, secret template, PVC,
│                         deployment, service, ingress, networkpolicy
├── migrations/         · SQL schema applied at boot
├── src/                · Rust service
│   ├── main.rs         · entry point + graceful shutdown
│   ├── config.rs       · env-driven configuration
│   ├── db.rs           · SQLite pool + migration runner
│   ├── audit.rs        · tamper-evident SHA-256 chain
│   ├── auth.rs         · bearer-token middleware
│   ├── seed.rs         · first-boot data loader
│   ├── models.rs       · DTOs matching the frontend contract
│   ├── routes/         · /api/* handlers + web shell
│   └── ...
└── static/             · RACKLOG dashboard (HTML/CSS/JSX)
```

## Development

```sh
cargo check            # type-check
cargo run              # local dev (serves http://127.0.0.1:8080)
cargo build --release  # production binary at target/release/racklog
cargo test             # 39 unit tests + 49 integration tests + 1 workflow
```

The dashboard JSX files run in-browser through Babel Standalone — no
build step. Edit a file in `static/`, set `RACKLOG_STATIC_DIR=./static`,
and reload to see changes without rebuilding the binary.

### Database migrations

Migration files live in `migrations/0001_initial.sql`, etc. The convention
is `NNNN_short-description.sql` with monotonically-increasing numbers
(zero-padded to 4 digits). `db::migrate()` runs them at startup and is
idempotent — already-applied migrations are skipped.

## License

Apache-2.0 — see [`LICENSE`](LICENSE).
