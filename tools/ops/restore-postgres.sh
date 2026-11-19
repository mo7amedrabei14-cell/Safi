#!/usr/bin/env bash
# tools/ops/restore-postgres.sh — Verified PostgreSQL 18 backup restoration drill script
# Usage: DATABASE_URL=postgres://... ./tools/ops/restore-postgres.sh /path/to/safidata-pg-*.dump
set -euo pipefail

if [ $# -lt 1 ]; then
  echo "Usage: DATABASE_URL=... $0 <backup.dump>" >&2
  exit 1
fi

DUMP_FILE="$1"
if [ ! -f "$DUMP_FILE" ]; then
  echo "ERROR: Backup dump file not found: $DUMP_FILE" >&2
  exit 1
fi

if [ -z "${DATABASE_URL:-}" ]; then
  echo "ERROR: DATABASE_URL must point to the target PostgreSQL 18 database" >&2
  exit 1
fi

if [ -f "${DUMP_FILE}.sha256" ]; then
  echo "[restore-postgres] Verifying SHA-256 checksum..."
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum -c "${DUMP_FILE}.sha256"
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 -c "${DUMP_FILE}.sha256"
  fi
fi

echo "[restore-postgres] Restoring dump into target database..."
pg_restore --clean --if-exists --no-owner --no-privileges --dbname="$DATABASE_URL" "$DUMP_FILE"
echo "[restore-postgres] Restore complete. Run 'SELECT count(*) FROM platform_owner;' and migration checks to verify."
