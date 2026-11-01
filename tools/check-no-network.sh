#!/usr/bin/env bash
# tools/check-no-network.sh — Automated prohibition of network dependencies inside Rust engine crates (crates/sf-*)
# Enforces AGENTS.md Hard Rule #7 and Specification Section 3.

set -euo pipefail

FORBIDDEN_CRATES=(
  "reqwest"
  "hyper"
  "hyper-util"
  "ureq"
  "surf"
  "isahc"
  "curl"
  "attohttpc"
  "minreq"
  "nyquest"
  "tokio-tungstenite"
  "tungstenite"
  "websocket"
  "tonic"
  "h2"
  "h3"
  "quinn"
  "rustls"
  "native-tls"
  "openssl"
  "mio"
  "socket2"
  "trust-dns-resolver"
  "hickory-resolver"
  "hickory-proto"
)

run_python() {
  if command -v python3 >/dev/null 2>&1 && python3 -c "import sys" >/dev/null 2>&1; then
    python3 "$@"
    return $?
  fi
  if command -v python >/dev/null 2>&1 && python -c "import sys" >/dev/null 2>&1; then
    python "$@"
    return $?
  fi
  if command -v py >/dev/null 2>&1 && py -3 -c "import sys" >/dev/null 2>&1; then
    py -3 "$@"
    return $?
  fi
  echo "ERROR: No working Python 3 interpreter found in PATH." >&2
  return 1
}

if [ "${1:-}" = "--self-test" ]; then
  echo "[check-no-network] Running negative self-test (verifying forbidden dependency & std::net detection)..."
  run_python - <<'PY'
import re
import sys

forbidden = {"reqwest", "hyper", "ureq", "tokio-tungstenite", "socket2", "rustls"}
simulated_engine_deps = {"serde", "thiserror", "reqwest"}
detected = forbidden.intersection(simulated_engine_deps)
if not detected:
    print("FAIL: Self-test did not detect forbidden network crate in graph!", file=sys.stderr)
    sys.exit(1)

simulated_rust_src = "use std::net::TcpStream;\npub fn connect() {}"
if not re.search(r"(\buse\s+std::net\b|\bstd::net::)", simulated_rust_src):
    print("FAIL: Self-test did not detect forbidden std::net usage!", file=sys.stderr)
    sys.exit(1)

print(
    f"[check-no-network] Self-test PASSED: detector caught forbidden crate(s) {sorted(detected)} and std::net usage."
)
PY
  exit 0
fi

echo "[check-no-network] Auditing engine crates (crates/sf-*) for zero network access..."

# 1. Check for std::net usage in crates/sf-*/src
if grep -RInE '(\buse\s+std::net\b|\bstd::net::|\buse\s+core::net\b|\bcore::net::)' crates/sf-*/src 2>/dev/null; then
  echo "ERROR [SF-SEC-NET]: Forbidden std::net or core::net usage found inside crates/sf-*/src!" >&2
  exit 1
fi

# 2. Check Cargo.toml files and full resolved Cargo dependency graph via cargo metadata
METADATA_JSON="$(cargo metadata --format-version 1)"
FORBIDDEN_CSV="$(IFS=,; echo "${FORBIDDEN_CRATES[*]}")"

METADATA_JSON="$METADATA_JSON" FORBIDDEN_CSV="$FORBIDDEN_CSV" run_python - <<'PY'
import json
import os
import pathlib
import re
import sys

forbidden = set(os.environ["FORBIDDEN_CSV"].split(","))

# A. Static scan of crates/sf-*/Cargo.toml
for cargo_toml in sorted(pathlib.Path("crates").glob("sf-*/Cargo.toml")):
    text = cargo_toml.read_text(encoding="utf-8")
    for crate_name in forbidden:
        pattern = rf"(?m)^\s*{re.escape(crate_name)}\s*="
        if re.search(pattern, text):
            print(
                f"ERROR [SF-SEC-NET]: Forbidden network crate '{crate_name}' declared in {cargo_toml}",
                file=sys.stderr,
            )
            sys.exit(1)

# B. Full transitive graph traversal via cargo metadata
metadata = json.loads(os.environ["METADATA_JSON"])
packages_by_id = {pkg["id"]: pkg["name"] for pkg in metadata.get("packages", [])}
resolve_nodes = {
    node["id"]: node.get("deps", [])
    for node in metadata.get("resolve", {}).get("nodes", [])
}

engine_root_ids = [
    pkg["id"]
    for pkg in metadata.get("packages", [])
    if pkg["name"].startswith("sf-")
]

if not engine_root_ids:
    print("ERROR [SF-SEC-NET]: No engine crates (sf-*) found in workspace!", file=sys.stderr)
    sys.exit(1)

violations = []
for root_id in engine_root_ids:
    root_name = packages_by_id[root_id]
    visited = set()
    stack = [root_id]
    while stack:
        current = stack.pop()
        if current in visited:
            continue
        visited.add(current)
        pkg_name = packages_by_id.get(current, "")
        if pkg_name in forbidden:
            violations.append((root_name, pkg_name))
        for dep in resolve_nodes.get(current, []):
            dep_pkg = dep.get("pkg")
            if dep_pkg and dep_pkg not in visited:
                stack.append(dep_pkg)

if violations:
    for root_name, bad_crate in violations:
        print(
            f"ERROR [SF-SEC-NET]: Engine crate '{root_name}' depends on forbidden network crate '{bad_crate}'",
            file=sys.stderr,
        )
    sys.exit(1)

print(
    f"[check-no-network] OK: Verified {len(engine_root_ids)} engine crates ({', '.join(sorted(packages_by_id[i] for i in engine_root_ids))}) -- 0 network dependencies found."
)
PY
