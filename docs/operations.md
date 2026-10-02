# Operations

## Container image

`Dockerfile` builds a single image: the Rust server and the built front-end, on a
distroless base (no shell), running as the unprivileged user 65532 with a read-only root
filesystem. Base images are pinned by digest. Configuration comes from a mounted
file (`LECTERN_CONFIG`) and environment variables.

## Kubernetes (K3s, GitOps)

`deploy/k8s/base` is a Kustomize base: Deployment (hardened security context, probes),
Service, Ingress (Traefik + cert-manager), PersistentVolumeClaims, NetworkPolicy and the
daily backup CronJob. `deploy/k8s/overlays/example` shows an instance overlay:
configuration and branding as generated ConfigMaps, host name, image tag. Point Argo CD or
Flux at your own overlay in your GitOps repository.

Secrets (`lectern-secrets`) are created with your secret tooling (SOPS, Sealed Secrets,
External Secrets) with keys `DATABASE_URL`, `LECTERN__AUTH__OIDC__CLIENT_SECRET`,
`LECTERN__MAIL__SMTP_URL`. Host the database (e.g. CloudNativePG) and the cluster in
France or the EU.

Migrations run automatically at start-up (`lectern serve`), inside a transaction each.

## Backups

The `lectern-backup` CronJob runs daily at 02:17: `pg_dump` (custom format) and a tarball
of the assets directory, kept 14 days on the `lectern-backups` volume. Copy that volume
off-site with your usual tooling. Restore:

```sh
pg_restore --clean --no-owner -d "$DATABASE_URL" db-YYYYMMDD-HHMMSS.dump
tar -C /app/data -xzf assets-YYYYMMDD-HHMMSS.tar.gz
```

Test a restore regularly.

## Monitoring

- `GET /healthz` (process alive) and `GET /readyz` (database reachable).
- JSON logs with `LECTERN_LOG_FORMAT=json`; level with `RUST_LOG` (default `info`).
- Watch for: `e-mail delivery failed` warnings, rows of `email_outbox` with
  `attempts >= 8` (given up), `internal error` log lines.

## Videos

Serve videos from storage you control (object storage + CDN, or a self-hosted streaming
server). Use HLS (`.m3u8`) for adaptive streaming; MP4 also works. Add the origin to
`server.media_origins` (CSP) and allow CORS from the academy origin when captions are
served from that origin.

## Upgrades

Roll out a new image tag through GitOps. Schema migrations are additive within a minor
version. Read the release notes before a major version.
