# RACKLOG · Kubernetes manifests

```sh
# 1. Build & push the image
docker build -t ghcr.io/your-user/racklog:0.1.0 .
docker push ghcr.io/your-user/racklog:0.1.0

# 2. Edit kustomization.yaml → images.newTag, then apply
kubectl apply -k deploy/kubernetes/

# 3. (Optional) Create a token-protected deployment
kubectl create secret generic racklog-secret -n racklog \
  --from-literal=RACKLOG_AUTH_TOKEN="$(openssl rand -base64 48)"

# 4. Port-forward for a quick smoke test
kubectl port-forward -n racklog svc/racklog 8080:80
open http://localhost:8080
```

## Files

| File                 | Purpose                                                        |
|----------------------|----------------------------------------------------------------|
| `namespace.yaml`     | Dedicated namespace                                            |
| `configmap.yaml`     | Non-secret runtime config (bind, log format, …)                |
| `secret.example.yaml`| Template for the optional bearer token (do **not** commit real)|
| `pvc.yaml`           | 2 GiB ReadWriteOnce volume for the SQLite WAL                  |
| `deployment.yaml`    | One non-root replica, probes, read-only fs, capabilities=ALL   |
| `service.yaml`       | ClusterIP service on port 80                                   |
| `ingress.yaml`       | Optional NGINX ingress (edit host first)                       |
| `networkpolicy.yaml` | Optional default-deny with explicit DNS egress                 |
| `kustomization.yaml` | Bundle for `kubectl apply -k`                                  |

## Scaling

The shipped manifest pins `replicas: 1` because the SQLite database lives
on a single PVC. The Rust service is otherwise stateless. Postgres
support is **not implemented today** — see [`docs/postgres.md`](../../docs/postgres.md)
for what it would actually take.

## Prerequisites

### Clock sync is mandatory

Every audit-log row records a wall-clock timestamp and the SHA-256 chain
covers it (canonical-JSON v2). If the host clock jumps backwards or sits
in the future, the chain still verifies, but the timestamps stop being
useful for forensics — and `retain_activity` deletes rows by absolute
timestamp, which silently misbehaves on a drifted clock.

Run `chrony` / `systemd-timesyncd` on every node and confirm with
`timedatectl`. Containers inherit the host clock; nothing inside the
RACKLOG image manages NTP. The k8s manifest does not mount
`/etc/localtime` — keep all timestamps in UTC, which is also what the
audit canonical-JSON encoding assumes.

### Trusted proxy and SSO

If you set `RACKLOG_TRUST_FORWARDED_HEADERS=1` to honour
`X-Forwarded-User` from Authelia / oauth2-proxy / Authentik, also set
`RACKLOG_TRUSTED_PROXY_CIDR` to the proxy's pod or node CIDR (e.g.
`10.244.0.0/16`). The middleware drops forwarded headers from any
peer outside the allowlist. Empty list = trust whoever can reach the
bind socket — fine on a loopback bind, dangerous otherwise.
