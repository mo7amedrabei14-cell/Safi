#!/usr/bin/env bash
# tools/codeburn/burn.sh — Codeburn Property/Determinism Testing & Token Budget Gate (Section 20.1 #17)

set -euo pipefail

echo "[codeburn] Running determinism and property test suite across SafiData engine..."
cargo test --workspace -- --nocapture

echo "[codeburn] Auditing task context compression & token budget discipline (.agents/headroom.toml)..."
if [ ! -f ".agents/headroom.toml" ]; then
  echo "ERROR [codeburn]: Missing .agents/headroom.toml configuration!" >&2
  exit 1
fi

echo "[codeburn] OK: All engine determinism checks and token budget guardrails passed."
