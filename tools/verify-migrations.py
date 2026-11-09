#!/usr/bin/env python3
"""
tools/verify-migrations.py — Static verifier for PostgreSQL 18 migrations (T4.02).

Checks, without a live database:
1. Every numbered `.up.sql` has a matching `.down.sql` (and vice versa).
2. Every table created by an up migration is dropped by its down migration.
3. Every workspace-owned table (`workspace_id`) enables and forces RLS and has a policy.
4. The schema uses native `uuidv7()`, never `SERIAL`/`BIGSERIAL`, and never Supabase.
5. Optional `pg_partman` usage is guarded by `pg_available_extensions`.

Run with `--self-test` to prove the detector rejects a deliberately broken tree.
"""

from __future__ import annotations

import pathlib
import re
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
MIGRATIONS_DIR = ROOT / "crates/sf-server/migrations"

UP_RE = re.compile(r"^(?P<number>\d{4})_(?P<name>[a-z0-9_]+)\.up\.sql$")
CREATE_TABLE_RE = re.compile(r"CREATE\s+TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?([a-z_][a-z0-9_]*)\s*\(", re.IGNORECASE)
CREATE_FUNCTION_RE = re.compile(r"CREATE\s+(?:OR\s+REPLACE\s+)?FUNCTION\s+([a-z_][a-z0-9_]*)", re.IGNORECASE)


def table_bodies(sql: str) -> dict[str, str]:
    """Map each created table to the SQL slice that defines it."""
    matches = list(CREATE_TABLE_RE.finditer(sql))
    bodies: dict[str, str] = {}
    for index, match in enumerate(matches):
        end = matches[index + 1].start() if index + 1 < len(matches) else len(sql)
        bodies[match.group(1)] = sql[match.start() : end]
    return bodies


def check_tree(directory: pathlib.Path) -> list[str]:
    errors: list[str] = []
    if not directory.exists():
        return [f"Missing migrations directory: {directory}"]

    ups = sorted(directory.glob("*.up.sql"))
    downs = sorted(directory.glob("*.down.sql"))
    if not ups:
        return ["No up migrations found."]

    for up in ups:
        match = UP_RE.match(up.name)
        if not match:
            errors.append(f"Migration '{up.name}' must be named NNNN_name.up.sql.")
            continue
        down = up.with_name(up.name.replace(".up.sql", ".down.sql"))
        if not down.exists():
            errors.append(f"Migration '{up.name}' has no matching down migration.")

    for down in downs:
        up = down.with_name(down.name.replace(".down.sql", ".up.sql"))
        if not up.exists():
            errors.append(f"Down migration '{down.name}' has no matching up migration.")

    for up in ups:
        down = up.with_name(up.name.replace(".up.sql", ".down.sql"))
        if not down.exists():
            continue
        up_sql = up.read_text(encoding="utf-8")
        down_sql = down.read_text(encoding="utf-8")

        for table, body in table_bodies(up_sql).items():
            if not re.search(
                rf"DROP\s+TABLE\s+IF\s+EXISTS\s+{re.escape(table)}\b", down_sql, re.IGNORECASE
            ):
                errors.append(f"{up.name}: down migration does not drop table '{table}'.")
            if "workspace_id" in body:
                if not re.search(
                    rf"ALTER\s+TABLE\s+{re.escape(table)}\s+ENABLE\s+ROW\s+LEVEL\s+SECURITY",
                    up_sql,
                    re.IGNORECASE,
                ):
                    errors.append(f"{up.name}: table '{table}' has workspace_id but no RLS.")
                if not re.search(
                    rf"ALTER\s+TABLE\s+{re.escape(table)}\s+FORCE\s+ROW\s+LEVEL\s+SECURITY",
                    up_sql,
                    re.IGNORECASE,
                ):
                    errors.append(f"{up.name}: table '{table}' must FORCE row level security.")
                if not re.search(
                    rf"CREATE\s+POLICY\s+\w+\s+ON\s+{re.escape(table)}\b", up_sql, re.IGNORECASE
                ):
                    errors.append(f"{up.name}: table '{table}' has workspace_id but no policy.")

        for function in CREATE_FUNCTION_RE.findall(up_sql):
            if not re.search(
                rf"DROP\s+FUNCTION\s+IF\s+EXISTS\s+{re.escape(function)}\b",
                down_sql,
                re.IGNORECASE,
            ):
                errors.append(f"{up.name}: down migration does not drop function '{function}'.")

        if "uuidv7()" not in up_sql:
            errors.append(f"{up.name}: no native uuidv7() usage.")
        if re.search(r"\b(?:BIG|SMALL)?SERIAL\b", up_sql, re.IGNORECASE):
            errors.append(f"{up.name}: SERIAL columns are forbidden; use uuidv7().")
        if re.search(r"supabase", up_sql, re.IGNORECASE):
            errors.append(f"{up.name}: Supabase references are forbidden.")

        if "partman." in up_sql:
            if "pg_available_extensions" not in up_sql:
                errors.append(
                    f"{up.name}: pg_partman must be guarded by pg_available_extensions."
                )

    return errors


def self_test() -> int:
    with tempfile.TemporaryDirectory() as temp:
        directory = pathlib.Path(temp)
        (directory / "0001_bad.up.sql").write_text(
            "CREATE TABLE widgets (id SERIAL PRIMARY KEY, workspace_id UUID);\n",
            encoding="utf-8",
        )
        (directory / "0001_bad.down.sql").write_text("-- empty down\n", encoding="utf-8")
        (directory / "0002_orphan.up.sql").write_text(
            "CREATE TABLE orphan (id UUID DEFAULT uuidv7());\n", encoding="utf-8"
        )
        errors = check_tree(directory)
        required_fragments = ["does not drop table 'widgets'", "no RLS", "SERIAL", "no matching down"]
        if not all(any(fragment in error for error in errors) for fragment in required_fragments):
            print(f"[verify-migrations --self-test] FAILED: {errors}", file=sys.stderr)
            return 1
    print("[verify-migrations] Self-test PASSED: broken pairs, missing RLS, and SERIAL were caught.")
    return 0


def main() -> int:
    if "--self-test" in sys.argv:
        return self_test()
    errors = check_tree(MIGRATIONS_DIR)
    if errors:
        for error in errors:
            print(f"ERROR [verify-migrations]: {error}", file=sys.stderr)
        return 1
    count = len(list(MIGRATIONS_DIR.glob("*.up.sql")))
    print(
        f"[verify-migrations] OK: {count} migration pair(s) verified "
        "(up/down, RLS, uuidv7, no SERIAL, guarded pg_partman)."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
