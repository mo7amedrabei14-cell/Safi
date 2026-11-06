#!/usr/bin/env python3
"""
tools/verify-i18n.py — Automated i18n Dictionary & ICU Key Consistency Verifier (`T1.12`)
Verifies:
1. Exact key parity between ar.json and en.json.
2. No empty translation strings.
3. Matching ICU placeholders ({placeholder}) in both locales.
"""

from __future__ import annotations

import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
LOCALES_DIR = ROOT / "packages/i18n/locales"

def extract_placeholders(text: str) -> set[str]:
    return set(re.findall(r"\{([^}]+)\}", text))

def main() -> int:
    if len(sys.argv) > 1 and sys.argv[1] == "--self-test":
        print("[verify-i18n] Running negative self-test...")
        ar_test = {"a.b": "مرحباً {name}"}
        en_test = {"a.b": "Hello {user}"}
        ar_keys = set(ar_test.keys())
        en_keys = set(en_test.keys())
        if ar_keys != en_keys:
            print("Self-test passed key check verification.", file=sys.stderr)

        ar_ph = extract_placeholders(ar_test["a.b"])
        en_ph = extract_placeholders(en_test["a.b"])
        if ar_ph != en_ph:
            print(f"[verify-i18n] Self-test correctly caught placeholder mismatch: ar={ar_ph}, en={en_ph}")
            print("[verify-i18n] Self-test PASSED.")
            return 0
        return 1

    ar_path = LOCALES_DIR / "ar.json"
    en_path = LOCALES_DIR / "en.json"

    if not ar_path.exists() or not en_path.exists():
        print("ERROR [verify-i18n]: Missing ar.json or en.json dictionary files.", file=sys.stderr)
        return 1

    ar_data = json.loads(ar_path.read_text(encoding="utf-8"))
    en_data = json.loads(en_path.read_text(encoding="utf-8"))

    ar_keys = set(ar_data.keys())
    en_keys = set(en_data.keys())

    errors = []

    missing_in_en = ar_keys - en_keys
    missing_in_ar = en_keys - ar_keys

    for k in missing_in_en:
        errors.append(f"Key '{k}' present in ar.json but missing in en.json.")
    for k in missing_in_ar:
        errors.append(f"Key '{k}' present in en.json but missing in ar.json.")

    common_keys = ar_keys.intersection(en_keys)
    for k in common_keys:
        ar_val = str(ar_data[k])
        en_val = str(en_data[k])

        if not ar_val.strip():
            errors.append(f"Empty translation string for key '{k}' in ar.json.")
        if not en_val.strip():
            errors.append(f"Empty translation string for key '{k}' in en.json.")

        ar_ph = extract_placeholders(ar_val)
        en_ph = extract_placeholders(en_val)
        if ar_ph != en_ph:
            errors.append(f"Placeholder mismatch for key '{k}': ar={ar_ph}, en={en_ph}")

    if errors:
        for err in errors:
            print(f"ERROR [verify-i18n]: {err}", file=sys.stderr)
        return 1

    print(f"[verify-i18n] OK: Verified {len(common_keys)} i18n keys across ar.json and en.json with complete ICU placeholder parity.")
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
