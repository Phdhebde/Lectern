#!/usr/bin/env bash
# Builds an on-premises release bundle: the API server binary, the built front-end and
# the deployment files, in target/dist/lectern-<version>-<arch>.tar.gz.
# Run it on the same OS family as the target host (glibc): e.g. Debian 12 for Debian 12+.
# See docs/deploy-onprem.md.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
version=$(sed -n 's/^version = "\(.*\)"/\1/p' server/Cargo.toml | head -n1)
arch=$(uname -m)
name="lectern-$version-linux-$arch"
out="target/dist/$name"

echo "==> API server"
cargo build --release --locked -p lectern-server

echo "==> Front-end"
(cd web && npm ci --ignore-scripts && npm run build)

echo "==> Bundle $out.tar.gz"
rm -rf "$out"
mkdir -p "$out/bin" "$out/web"
install -m 0755 target/release/lectern-server "$out/bin/lectern"
install -m 0755 deploy/onprem/lectern-backup.sh "$out/bin/lectern-backup"
cp -R web/dist/. "$out/web/"
cp -R deploy/onprem/systemd deploy/onprem/nginx "$out/"
cp deploy/onprem/lectern.example.toml deploy/onprem/lectern.env.example "$out/"
cp -R examples/branding "$out/branding"
cp LICENSE "$out/"
tar -C target/dist -czf "$out.tar.gz" "$name"
(cd target/dist && sha256sum "$name.tar.gz" > "$name.tar.gz.sha256")
echo "==> Done: $out.tar.gz"
