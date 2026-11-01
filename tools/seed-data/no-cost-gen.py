#!/usr/bin/env python3
"""
tools/seed-data/no-cost-gen.py — No-Cost AI CLI Entrypoint (`T1.00` — Section 19.4 & Section 20.1 #19)
Foundational script boundary for `tools/seed-data/`.
Actual synthetic golden dataset generation (20 files in S1, 300+ files in S2+) is implemented in `T1.20`.
"""

from __future__ import annotations

import sys


TOOL_ID = "no-cost-ai"
TARGET_TASK = "T1.20"


def main() -> int:
    if "--verify" in sys.argv:
        print(
            f"[{TOOL_ID}] OK: CLI entrypoint ready (golden dataset generation deferred to {TARGET_TASK})."
        )
        return 0

    print(f"{TOOL_ID}: ready (target task {TARGET_TASK})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
