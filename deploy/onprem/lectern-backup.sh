#!/bin/sh
# Lectern backup: PostgreSQL dump (custom format) + tarball of the uploaded assets,
# kept LECTERN_BACKUP_RETENTION_DAYS days. Install as /usr/local/bin/lectern-backup.
# Needs DATABASE_URL and pg_dump of the same major version as the server (or newer).
set -eu

data_dir=${LECTERN_DATA_DIR:-/var/lib/lectern/data}
backup_dir=${LECTERN_BACKUP_DIR:-/var/backups/lectern}
retention=${LECTERN_BACKUP_RETENTION_DAYS:-14}
: "${DATABASE_URL:?DATABASE_URL is not set}"

stamp=$(date +%Y%m%d-%H%M%S)
pg_dump --format=custom --no-owner "$DATABASE_URL" > "$backup_dir/db-$stamp.dump"
tar -C "$data_dir" -czf "$backup_dir/assets-$stamp.tar.gz" assets
find "$backup_dir" -type f -mtime +"$retention" -delete
ls -lh "$backup_dir"
