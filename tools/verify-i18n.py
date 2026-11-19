#!/usr/bin/env python3
"""
tools/verify-i18n.py — Automated i18n Dictionary & ICU Key Consistency Verifier (`T1.12`, Phase 12)
Verifies across ALL 7 launch languages (`en`, `ar`, `fr`, `es`, `tr`, `fa`, `ur`):
1. Exact key parity against `en.json` (canonical default locale).
2. No empty translation strings.
3. Matching ICU placeholders ({placeholder}) in every locale.
"""

from __future__ import annotations

import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
LOCALES_DIR = ROOT / "packages/i18n/locales"
REQUIRED_LOCALES = ("en", "ar", "fr", "es", "tr", "fa", "ur")


def extract_placeholders(text: str) -> set[str]:
    return set(re.findall(r"\{([^}]+)\}", text))


def main() -> int:
    if len(sys.argv) > 1 and sys.argv[1] == "--self-test":
        print("[verify-i18n] Running negative self-test...")
        ar_test = {"a.b": "مرحباً {name}"}
        en_test = {"a.b": "Hello {user}"}
        ar_ph = extract_placeholders(ar_test["a.b"])
        en_ph = extract_placeholders(en_test["a.b"])
        if ar_ph != en_ph:
            print(
                f"[verify-i18n] Self-test correctly caught placeholder mismatch: ar={ar_ph}, en={en_ph}"
            )
            print("[verify-i18n] Self-test PASSED.")
            return 0
        return 1

    loaded: dict[str, dict[str, str]] = {}
    errors: list[str] = []

    for code in REQUIRED_LOCALES:
        path = LOCALES_DIR / f"{code}.json"
        if not path.exists():
            errors.append(f"Missing required locale dictionary: {path}")
            continue
        loaded[code] = json.loads(path.read_text(encoding="utf-8"))

    if "en" not in loaded:
        for err in errors:
            print(f"ERROR [verify-i18n]: {err}", file=sys.stderr)
        return 1

    en_data = loaded["en"]
    en_keys = set(en_data.keys())

    for code, data in loaded.items():
        keys = set(data.keys())
        for missing in sorted(en_keys - keys):
            errors.append(f"Key '{missing}' present in en.json but missing in {code}.json.")
        for extra in sorted(keys - en_keys):
            errors.append(f"Key '{extra}' present in {code}.json but missing in en.json.")
        for k in sorted(en_keys.intersection(keys)):
            val = str(data[k])
            if not val.strip():
                errors.append(f"Empty translation string for key '{k}' in {code}.json.")
            en_ph = extract_placeholders(str(en_data[k]))
            loc_ph = extract_placeholders(val)
            if en_ph != loc_ph:
                errors.append(
                    f"Placeholder mismatch for key '{k}' in {code}.json: expected={en_ph}, actual={loc_ph}"
                )

    if errors:
        for err in errors:
            print(f"ERROR [verify-i18n]: {err}", file=sys.stderr)
        return 1

    print(
        f"[verify-i18n] OK: Verified {len(en_keys)} i18n keys across all {len(REQUIRED_LOCALES)} launch locales "
        f"({', '.join(REQUIRED_LOCALES)}) with complete ICU placeholder parity."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
