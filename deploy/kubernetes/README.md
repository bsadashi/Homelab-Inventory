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
on a single PVC. If you want HA, point `DATABASE_URL` at Postgres (rebuild
the binary with `--features postgres` once that flag lands), drop the PVC,
and bump replicas — the Rust service is otherwise stateless.
