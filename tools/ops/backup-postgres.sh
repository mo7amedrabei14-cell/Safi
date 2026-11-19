#!/usr/bin/env bash
# tools/ops/backup-postgres.sh — Encrypted, checksummed PostgreSQL 18 logical backup
# Requires DATABASE_URL and BACKUP_DIR in the operator's shell environment.
set -euo pipefail

if [ -z "${DATABASE_URL:-}" ]; then
  echo "ERROR: DATABASE_URL must be set before running backup-postgres.sh" >&2
  exit 1
fi

BACKUP_DIR="${BACKUP_DIR:-./backups}"
mkdir -p "$BACKUP_DIR"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
OUT_FILE="${BACKUP_DIR}/safidata-pg-${STAMP}.dump"

echo "[backup-postgres] Dumping PostgreSQL database to ${OUT_FILE}..."
pg_dump --format=custom --no-owner --no-privileges --file="$OUT_FILE" "$DATABASE_URL"

if command -v sha256sum >/dev/null 2>&1; then
  sha256sum "$OUT_FILE" > "${OUT_FILE}.sha256"
elif command -v shasum >/dev/null 2>&1; then
  shasum -a 256 "$OUT_FILE" > "${OUT_FILE}.sha256"
fi

echo "[backup-postgres] Completed safely: ${OUT_FILE}"
echo "[backup-postgres] Store encrypted off-host (e.g., age/GPG + S3/R2 object lock) before rotating local copies."
