#!/usr/bin/env python3
"""Split docs/spec-full.md into per-section Markdown files under docs/spec/ (Section 19.3)."""

import pathlib
import re


def main() -> None:
    src = pathlib.Path("docs/spec-full.md").read_text(encoding="utf-8")
    names = {
        1: "01-rules-vision",
        2: "02-users",
        3: "03-architecture",
        4: "04-engine",
        5: "05-features",
        6: "06-ui-design",
        7: "07-identity-logo-motion",
        8: "08-security-privacy",
        9: "09-backend-owner-console",
        10: "10-apps-distribution",
        11: "11-quality-testing",
        12: "12-site-pricing-launch",
        13: "13-roadmap",
        14: "14-languages",
        15: "15-logging",
        16: "16-understanding",
        17: "17-chart-studio",
        18: "18-review",
        19: "19-handoff",
        20: "20-v5-architecture-upgrades",
    }
    parts = re.split(r"(?m)^## (\d+)\. ", src)
    out = pathlib.Path("docs/spec")
    out.mkdir(parents=True, exist_ok=True)
    (out / "00-title.md").write_text(parts[0], encoding="utf-8")
    for i in range(1, len(parts), 2):
        n = int(parts[i])
        (out / (names.get(n, f"{n:02d}-section") + ".md")).write_text(
            f"## {n}. " + parts[i + 1], encoding="utf-8"
        )
    print("done")


if __name__ == "__main__":
    main()
