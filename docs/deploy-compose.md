# Deployment with Docker Compose

The whole stack on one host, from [`deploy/compose`](../deploy/compose):

| Service | Role | Started |
| --- | --- | --- |
| `db` | PostgreSQL 18, data in the `db` volume (mounted on `/var/lib/postgresql`), not published | always |
| `lectern` | API server + front-end (the repository's `Dockerfile`), uploaded assets in the `data` volume, published on `127.0.0.1:8080` only | always |
| `proxy` | nginx: TLS, HTTP→HTTPS redirect, rate limits, upload size | profile `proxy` (on by default in `.env.example`) |
| `backup` | `pg_dump` + assets tarball into `./backups`, kept 14 days | on demand: `docker compose run --rm backup` |

Requirements: Docker Engine 24+ with the Compose plugin, a DNS name for the academy, a TLS
certificate for it, an SMTP relay (see [configuration](configuration.md)).

## Install

```sh
cd deploy/compose
cp .env.example .env                    # public URL, host name, database password, profiles
cp lectern.env.example lectern.env      # OIDC client secret, SMTP URL
cp lectern.example.toml lectern.toml    # instance: name, theme, sign-in, certificates
mkdir -p certs branding backups
cp ../../examples/branding/* branding/  # then replace with your logo and favicon
# certs/fullchain.pem and certs/privkey.pem: your certificate (internal PKI, certbot...)
```

In `.env`, set `LECTERN_BRANDING_DIR=./branding` and generate the database password with
`openssl rand -hex 32` (letters and digits only: it is embedded in `DATABASE_URL`).
`.env`, `lectern.env`, `lectern.toml`, `branding/`, `certs/` and `backups/` are ignored by
git: they belong to the instance.

Build and start:

```sh
docker compose up -d --build
docker compose logs -f lectern          # migrations run at start-up, then "listening"
docker compose exec lectern lectern check-config
docker compose exec lectern lectern grant-role you@company.com admin
```

Import a content pack (directory or `.zip`):

```sh
docker compose run --rm -v "$PWD/my-pack:/pack:ro" lectern import-pack /pack
```

## Using a pre-built image

Build the image once in CI (the `image` job of `.github/workflows/ci.yml` builds and scans
it) and push it to your registry, then on the host set `LECTERN_IMAGE` in `.env`:

```sh
docker build -t registry.example.com/lectern:0.1.0 .      # from the repository root
docker push registry.example.com/lectern:0.1.0
# on the host, LECTERN_IMAGE=registry.example.com/lectern:0.1.0
docker compose pull && docker compose up -d
```

## Your own reverse proxy

Remove `proxy` from `COMPOSE_PROFILES` and point your reverse proxy at
`http://127.0.0.1:8080` (`LECTERN_HTTP_PORT`). It must keep the `Host` header, terminate
TLS, accept request bodies up to 200 MB and ideally rate-limit `/auth/` and `/api/auth/`.
[`nginx/lectern.conf.template`](../deploy/compose/nginx/lectern.conf.template) is a
complete example.

## Backups

```sh
docker compose run --rm backup
```

Schedule it daily, e.g. in root's crontab:

```
17 2 * * * cd /opt/lectern/deploy/compose && docker compose run --rm backup >/dev/null
```

Copy `backups/` off-site. Restore (the volume is named after the Compose project,
`lectern_data` by default):

```sh
docker compose stop lectern
docker compose exec -T db pg_restore --clean --no-owner -U lectern -d lectern < backups/db-YYYYMMDD-HHMMSS.dump
docker run --rm --user 65532:65532 -v lectern_data:/data -v "$PWD/backups:/backups:ro" \
  postgres:18 tar -C /data -xzf /backups/assets-YYYYMMDD-HHMMSS.tar.gz
docker compose start lectern
```

Test a restore regularly.

## Upgrades

```sh
git pull                                   # or change LECTERN_IMAGE
docker compose run --rm backup
docker compose up -d --build               # migrations run at start-up
```

A new PostgreSQL **major** version (e.g. a Dependabot update from 18 to 19) does not start
on the old data directory: back up, stop the stack, remove the `lectern_db` volume, start
`db` alone, restore the dump (see *Backups*), then start the rest. Minor versions and
digest updates need nothing special.

## Hardening applied

- `lectern`: read-only root filesystem, every capability dropped, `no-new-privileges`,
  non-root user (65532), distroless image, published on the loopback interface only.
- `proxy`: unprivileged nginx (user 101, ports 8080/8443 inside), read-only, no
  capabilities.
- `db`: not published, SCRAM authentication.
- Images pinned by digest; Dependabot proposes updates.

See also [operations](operations.md) for monitoring and [on-premises deployment](deploy-onprem.md)
to run each component without containers.
