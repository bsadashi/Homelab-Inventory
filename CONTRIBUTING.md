# Contributing to RACKLOG

A short guide so the next change you push doesn't bounce off CI.

## Local checks

Before opening a PR, run:

```sh
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test --locked
```

The CI workflow runs the same four commands plus a release build. If
they pass locally, they should pass in CI.

## Layout

| Directory | Purpose |
|---|---|
| `src/`               | Rust service (lib + bin) |
| `src/auth/`          | Identity, sessions, API keys, password hashing, middleware |
| `src/routes/`        | One module per URL nest. `routes::serve(state)` returns the fully-armed Router |
| `migrations/`        | `NNNN_short-description.sql`. Numbers are zero-padded to 4 digits and monotonically increasing |
| `static/`            | Frontend bundle. JSX runs through Babel-Standalone in-browser; plain `.js` files are served as-is |
| `tests/`             | Integration tests; `tests/common/mod.rs` is the shared harness |
| `deploy/kubernetes/` | Bundled manifests + a `kustomization.yaml` |

## Coding conventions

- **No `unsafe_code`** — forbidden by the lints table.
- **No `unreachable_pub`** — make symbols `pub(crate)` unless they
  cross the lib boundary.
- **Audit every mutation.** Use `audit::log_in_tx(tx, &auth.username,
  "kind", Some(&id), &description).await?` inside the same
  transaction as the data write so the chain stays atomic.
- **Role gates.** Mutations take `RequireOperator(auth):
  RequireOperator`. Admin endpoints take `RequireAdmin`.
- **Configuration** comes from env via `Config::from_env`. Add a
  matching field to `Config::for_test` so the test harness defaults
  match production semantics.
- **Errors.** Handlers return `ApiResult<T>`. Map domain conditions to
  the matching `ApiError` variant (`NotFound`, `Conflict`,
  `BadRequest`, `Unauthorized`, `Forbidden`); raw `sqlx::Error` bubbles
  up via `?` and renders as a 500.

## Migrations

Add a new file `migrations/NNNN_…_.sql`. The runner is idempotent and
applied at startup; the test harness re-runs every migration on a
fresh tempdir DB on each test, so migrations have to stay
deterministic and self-contained.

## Tests

- New endpoints get an integration test in `tests/api.rs`.
- New auth flows go in `tests/auth.rs`; user/key admin flows go in
  `tests/admin_users.rs`.
- The `Harness` in `tests/common/mod.rs` provides isolated SQLite
  pools per test. Use `Harness::boot()` for seeded data,
  `Harness::boot_with(None, false)` for an empty DB,
  `Harness::boot_open_signup()` for second-user-as-viewer flows.
- For end-to-end coverage of a new feature, extend
  `tests/workflow.rs::full_ops_workflow`.

## Pull requests

- Keep commits small and self-describing — the project's git history
  is part of the documentation.
- Run the local checks above. CI is fast (~3 min from cold), but
  rustfmt nits cost a round trip.
- If you touch the audit chain, the schema, or auth, mention it in the
  PR description so reviewers know to look at the security-critical
  changes first.
