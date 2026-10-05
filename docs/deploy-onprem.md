# On-premises deployment, component by component

Each component runs on hosts you manage, without containers. They can share one machine or
be split: database server, application server, web front.

```
            HTTPS                        HTTP (loopback or private network)
browser ──────────▶ nginx ──── /api, /auth, /verify, /ob, /branding, *.css ──▶ lectern (API)
                     │                                                          │
                     └── front-end (static files, option B)                     ▼
                                                                          PostgreSQL 16
```

Files: [`deploy/onprem`](../deploy/onprem). Commands below are for Debian 12 / Ubuntu 24.04;
adapt package names elsewhere.

## 1. Build

On a build machine (or in CI) of the same OS family as the servers (glibc):
Rust stable, Node.js 22.

```sh
scripts/build-release.sh
# -> target/dist/lectern-<version>-linux-<arch>.tar.gz (+ .sha256)
```

The bundle holds `bin/lectern` (API server), `bin/lectern-backup`, `web/` (built
front-end), `systemd/`, `nginx/`, example configuration and branding. The two halves can
also be built separately:

| Component | Command | Output |
| --- | --- | --- |
| API server | `cargo build --release --locked -p lectern-server` | `target/release/lectern-server` |
| Front-end | `cd web && npm ci --ignore-scripts && npm run build` | `web/dist/` |

Without a Rust toolchain, the binary and the front-end can be taken from the container
image: `docker create --name x <image>`, `docker cp x:/usr/local/bin/lectern .`,
`docker cp x:/app/web/dist web`, `docker rm x`.

## 2. PostgreSQL

PostgreSQL 14+ (16 recommended), UTF-8. On the database host:

```sh
apt install postgresql-16
sudo -u postgres createuser --pwprompt lectern
sudo -u postgres createdb --owner=lectern --encoding=UTF8 lectern
```

If the API runs on another host: set `listen_addresses` in `postgresql.conf`, allow only the
application host in `pg_hba.conf` (`hostssl lectern lectern 10.0.0.12/32 scram-sha-256`),
enable TLS and use `?sslmode=require` (or `verify-full`) in `DATABASE_URL`. A managed or
existing PostgreSQL cluster works the same way. Migrations are applied by the API at
start-up; no extension is needed.

## 3. API server

On the application host:

```sh
tar xzf lectern-<version>-linux-<arch>.tar.gz && cd lectern-<version>-linux-<arch>
useradd --system --home-dir /var/lib/lectern --shell /usr/sbin/nologin lectern
install -m 0755 bin/lectern bin/lectern-backup /usr/local/bin/
install -d -m 0750 -o root -g lectern /etc/lectern /etc/lectern/branding
install -m 0640 -o root -g lectern lectern.example.toml /etc/lectern/lectern.toml
install -m 0640 -o root -g lectern lectern.env.example /etc/lectern/lectern.env
install -m 0644 branding/* /etc/lectern/branding/     # then replace with your logo and favicon
install -m 0644 systemd/lectern.service /etc/systemd/system/
```

Edit `/etc/lectern/lectern.toml` (instance, `public_url`, sign-in, see
[configuration](configuration.md)) and `/etc/lectern/lectern.env` (`DATABASE_URL`, OIDC
client secret, SMTP URL). Then:

```sh
set -a; . /etc/lectern/lectern.env; set +a          # as root, in a shell for the admin tasks
sudo -E -u lectern lectern --config /etc/lectern/lectern.toml check-config
systemctl daemon-reload && systemctl enable --now lectern
journalctl -u lectern -f                 # migrations, then "listening"
curl -fsS http://127.0.0.1:8080/readyz   # "ready" once the database is reachable
```

The service listens on `server.bind` (`127.0.0.1:8080` in the example; use the private
address when nginx runs on another host, and firewall it). Uploaded assets go to
`/var/lib/lectern/data`. The unit is sandboxed (read-only system, no capabilities, only its
state directory writable).

Administration commands use the same configuration and environment (shell above):

```sh
sudo -E -u lectern lectern --config /etc/lectern/lectern.toml grant-role you@company.com admin
sudo -E -u lectern lectern --config /etc/lectern/lectern.toml import-pack /path/to/pack
```

## 4. Front-end and nginx

The browser must see the front-end and the API on **one origin**: the API checks the
`Origin` of state-changing requests against `public_url`, its session cookie is
host-only, and the CSP only allows `'self'`. Two ways to serve the front-end:

- **Option A — the API serves it.** Copy `web/` to `/var/www/lectern/current` on the API
  host (the `server.static_dir` of the example) and let nginx forward everything.
- **Option B — nginx serves it** (default in [`nginx/lectern.conf`](../deploy/onprem/nginx/lectern.conf)).
  Copy `web/` to `/var/www/lectern/current` on the nginx host; nginx serves the static
  files and forwards only the server-owned paths (`/api/`, `/auth/`, `/verify/`, `/ob/`,
  `/branding/`, `/theme.css`, `/server.css`, `/healthz`, `/readyz`). nginx then sets the
  security headers of the front-end pages itself: keep its `Content-Security-Policy` in
  sync with `server.media_origins`.

Release directories make front-end updates atomic:

```sh
install -d /var/www/lectern/releases/<version>
cp -R web/. /var/www/lectern/releases/<version>/
ln -sfn /var/www/lectern/releases/<version> /var/www/lectern/current
```

nginx (1.22+):

```sh
apt install nginx
install -m 0644 nginx/lectern.conf /etc/nginx/conf.d/lectern.conf
# edit: server_name, certificate paths, upstream address (when the API is on another host)
nginx -t && systemctl reload nginx
```

The configuration redirects HTTP to HTTPS, limits uploads to 200 MB and rate-limits the
whole site and, more strictly, the sign-in endpoints (same values as the Kubernetes
overlay). Any other reverse proxy works under the same rules.

## 5. Backups

On the application host (needs `pg_dump` of the server's major version: `postgresql-client-16`):

```sh
install -d -m 0750 -o lectern -g lectern /var/backups/lectern
install -m 0644 systemd/lectern-backup.service systemd/lectern-backup.timer /etc/systemd/system/
systemctl daemon-reload && systemctl enable --now lectern-backup.timer
systemctl start lectern-backup.service   # first run, check with journalctl -u lectern-backup
```

Daily at 02:17: database dump and assets tarball, kept 14 days in `/var/backups/lectern`.
Copy them off-site. Restore:

```sh
set -a; . /etc/lectern/lectern.env; set +a
systemctl stop lectern
pg_restore --clean --no-owner -d "$DATABASE_URL" /var/backups/lectern/db-YYYYMMDD-HHMMSS.dump
sudo -u lectern tar -C /var/lib/lectern/data -xzf /var/backups/lectern/assets-YYYYMMDD-HHMMSS.tar.gz
systemctl start lectern
```

## 6. Upgrades

1. Back up (`systemctl start lectern-backup.service`).
2. API: replace `/usr/local/bin/lectern`, `systemctl restart lectern` (migrations run at
   start-up, additive within a minor version).
3. Front-end: copy the new `web/` into a new release directory and switch the `current`
   link. Upgrade the API first: a new front-end may call new endpoints.

Monitoring is described in [operations](operations.md). To run everything in containers
instead, see [Docker Compose deployment](deploy-compose.md).
