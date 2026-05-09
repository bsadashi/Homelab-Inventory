# RACKLOG · Deployment Hardening

The shipped Dockerfile, docker-compose, and Kubernetes manifests already
encode a tight default posture (distroless runtime, non-root UID,
read-only rootfs, dropped capabilities, seccomp + AppArmor, default-deny
NetworkPolicy, no telemetry). This file covers the host-level controls
the container layer can't enforce on its own.

## Clock sync (mandatory)

The audit chain's SHA-256 hash covers the wall-clock timestamp on every
row but doesn't validate it against any trusted-time service. A drifted
host clock produces a chain that still verifies but with timestamps
that are useless for forensics — and `/api/admin/retain_activity`
deletes by absolute timestamp, which silently misbehaves on drift.

```sh
# systemd-based hosts
sudo systemctl enable --now systemd-timesyncd
timedatectl status

# or chrony, if you want a slightly tighter bound
sudo apt install chrony
chronyc tracking
```

Containers inherit the host clock; the RACKLOG image ships nothing
that touches NTP itself.

## Per-IP brute-force banning (fail2ban / CrowdSec)

RACKLOG already has a per-username login throttle (5/min default,
configurable via `RACKLOG_LOGIN_ATTEMPTS_PER_MIN`). That stops a
dictionary attack against a single account but doesn't help when the
attacker either uses many usernames or has many IPs (the throttle is
per-username). The complementary defence is a host- or proxy-level
banner that blocks attacker IPs at the firewall.

The login handler emits a structured WARN line on every failed
attempt with `event=auth.failed`. Both fail2ban and CrowdSec parse it
out of the JSON log stream that `RACKLOG_LOG_FORMAT=json` writes.

### fail2ban

`/etc/fail2ban/filter.d/racklog.conf`:

```ini
[Definition]
# Match the structured WARN line our login handler writes when an
# attempt fails. The reverse-proxy in front (nginx, Caddy, etc.) is
# what sees the real client IP; fail2ban reads its access log
# alongside ours and bans accordingly.
failregex = ^.*"event":"auth\.failed".*$
ignoreregex =
```

`/etc/fail2ban/jail.d/racklog.conf`:

```ini
[racklog]
enabled  = true
filter   = racklog
# Point at wherever your container runtime ships RACKLOG's stderr.
# Compose default: /var/lib/docker/containers/<id>/<id>-json.log
# Or pipe via journald: backend = systemd
logpath  = /var/log/racklog/racklog.log
backend  = auto
maxretry = 5
findtime = 5m
bantime  = 1h
action   = iptables-multiport[name=racklog, port="80,443"]
```

The maxretry of 5 sits one above RACKLOG's per-username throttle
default (the throttle returns 429 on the 11th attempt; `auth.failed`
fires for both throttle hits and password mismatches, so we want
fail2ban to ban *before* an attacker can sustain 11 attempts/min
against a single account from one IP).

### CrowdSec

CrowdSec ships parsers in YAML and shares signal across instances via
the central API. Drop into `/etc/crowdsec/parsers/s01-parse/racklog.yaml`:

```yaml
filter: "evt.Parsed.program == 'racklog'"
onsuccess: next_stage
name: racklog/auth
description: "Parse RACKLOG auth failure log lines"
nodes:
  - grok:
      pattern: '"event":"auth\.failed".*"username":"%{DATA:username}"'
      apply_on: message
statics:
  - meta: log_type
    value: racklog_auth_failed
  - meta: source_ip
    expression: evt.Meta.source_ip
```

Plus a scenario in `/etc/crowdsec/scenarios/racklog-bf.yaml`:

```yaml
type: leaky
name: racklog/bruteforce
description: "RACKLOG login bruteforce"
filter: "evt.Meta.log_type == 'racklog_auth_failed'"
leakspeed: "10s"
capacity: 5
groupby: evt.Meta.source_ip
blackhole: 1m
labels:
  service: racklog
  type: bruteforce
  remediation: true
```

This bans IPs that produce more than 5 `auth.failed` lines in 50
seconds for one minute, escalating with the standard CrowdSec
`crowdsecurity/iptables` bouncer.

## Container image hygiene

- **Pin distroless by digest.** The Dockerfile FROM line uses
  `gcr.io/distroless/cc-debian12:nonroot` for human readability; in
  CI / production, replace with `@sha256:…` so a trojaned tag can't
  silently re-point. Bump the digest on a regular cadence.
- **Trivy on every build.** CI runs `aquasecurity/trivy-action` against
  the built image and fails on HIGH/CRITICAL. See
  `.github/workflows/ci.yml::trivy`.
- **No latest tag in production.** docker-compose pins to a built
  image label; the K8s `kustomization.yaml` pins via `images.newTag`.
  Override on every release.

## Lynis audit (optional, ad-hoc)

For a one-shot "did we miss anything obvious?" pass, run Lynis against
a debugging container with shell access:

```sh
docker run --rm --pid=container:racklog --net=container:racklog \
  cisagov/lynis:latest audit dockerfile /Dockerfile
```

Lynis won't have anything substantive to say about the distroless
runtime image (no shell, no auditable services), but it's useful for
the host running the container.
