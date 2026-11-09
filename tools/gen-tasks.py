#!/usr/bin/env python3
"""
tools/gen-tasks.py — Automated Task File Generator for Task T0.01 (Section 19.6)
Parses all task tables across Slices S1 through S6 in docs/spec/19-handoff.md and generates
docs/tasks/<ID>.md for T0.01 and every task in S1..S6 with:
- Title (العنوان)
- Deliverables (المخرجات)
- Referenced Sections (الأقسام المرجعية)
- Acceptance Criteria (معيار القبول)
"""

from __future__ import annotations

import pathlib
import re
import sys


ROOT = pathlib.Path(__file__).resolve().parent.parent
SPEC_HANDOFF = ROOT / "docs/spec/19-handoff.md"
TASKS_DIR = ROOT / "docs/tasks"

SLICE_HEADERS = {
    "T1": "الشريحة S1: الأساس والتنضيف والتقرير (أسابيع 1 إلى 4 — القسم 19.7)",
    "T2": "الشريحة S2: الوصفات والفهم الكامل (أسابيع 5 إلى 8 — القسم 19.8)",
    "T3": "الشريحة S3: الرسوم والتقارير والقوالب والبعد الإنساني (أسابيع 9 إلى 14 — القسم 19.9)",
    "T4": "الشريحة S4: الخلفية والحسابات والدفع (أسابيع 15 إلى 22 — القسم 19.10)",
    "T5": "الشريحة S5: الديسكتوب والأوفلاين ولوحة المالك (أسابيع 23 إلى 32 — القسم 19.11)",
    "T6": "الشريحة S6: المزامنة والذكاء الاصطناعي والفرق واللغات والإطلاق (من الأسبوع 33 — القسم 19.12)",
}

SECTION_MAP = {
    "1": "docs/spec/01-rules-vision.md",
    "2": "docs/spec/02-users.md",
    "3": "docs/spec/03-architecture.md",
    "4": "docs/spec/04-engine.md",
    "5": "docs/spec/05-features.md",
    "6": "docs/spec/06-ui-design.md",
    "7": "docs/spec/07-identity-logo-motion.md",
    "8": "docs/spec/08-security-privacy.md",
    "9": "docs/spec/09-backend-owner-console.md",
    "10": "docs/spec/10-apps-distribution.md",
    "11": "docs/spec/11-quality-testing.md",
    "12": "docs/spec/12-site-pricing-launch.md",
    "13": "docs/spec/13-roadmap.md",
    "14": "docs/spec/14-languages.md",
    "15": "docs/spec/15-logging.md",
    "16": "docs/spec/16-understanding.md",
    "17": "docs/spec/17-chart-studio.md",
    "18": "docs/spec/18-review.md",
    "19": "docs/spec/19-handoff.md",
    "20": "docs/spec/20-v5-architecture-upgrades.md",
}


def resolve_spec_files(refs_raw: str) -> list[str]:
    nums = re.findall(r"\b(\d{1,2})(?:\.\d+)?\b", refs_raw)
    files: list[str] = []
    for n in nums:
        if n in SECTION_MAP and SECTION_MAP[n] not in files:
            files.append(SECTION_MAP[n])
    for mandatory in ("docs/spec/18-review.md", "docs/spec/19-handoff.md", "docs/spec/20-v5-architecture-upgrades.md"):
        if mandatory not in files:
            files.append(mandatory)
    return files


def format_task_markdown(task_id: str, slice_title: str, deliverables: str, refs: str, acceptance: str) -> str:
    spec_files = resolve_spec_files(refs)
    spec_bullets = "\n".join(f"- `{path}`" for path in spec_files)
    return (
        f"# المهمة `{task_id}` — {slice_title}\n\n"
        f"## 1. العنوان\n"
        f"- **معرّف المهمة**: `{task_id}`\n"
        f"- **الشريحة**: {slice_title}\n\n"
        f"## 2. المخرجات المطلوبة (Deliverables)\n"
        f"{deliverables}\n\n"
        f"## 3. الأقسام المرجعية (Referenced Specification Sections)\n"
        f"- **المراجع في جدول المواصفة**: {refs}\n"
        f"- **ملفات المواصفة الواجب قراءتها قبل البرمجة**:\n"
        f"{spec_bullets}\n\n"
        f"## 4. معيار القبول (Acceptance Criteria)\n"
        f"- {acceptance}\n"
    )


def main() -> int:
    if not SPEC_HANDOFF.exists():
        print(f"ERROR: Missing {SPEC_HANDOFF}", file=sys.stderr)
        return 1

    TASKS_DIR.mkdir(parents=True, exist_ok=True)

    # 1. Write T0.01 from Section 19.6
    t0_md = format_task_markdown(
        task_id="T0.01",
        slice_title="المرحلة التمهيدية: توليد ملفات المهام (القسم 19.6)",
        deliverables="قراءة القسم 19 من `docs/spec/19-handoff.md` وإنشاء ملف لكل مهمة في `docs/tasks/<ID>.md` من جداول المهام (S1 إلى S6) يتضمن العنوان، المخرجات، الأقسام المرجعية، ومعيار القبول.",
        refs="19.6، 19.7 إلى 19.12",
        acceptance="جميع ملفات المهام في `docs/tasks/<ID>.md` منشأة ومطابقة لجداول القسم 19.",
    )
    (TASKS_DIR / "T0.01.md").write_text(t0_md, encoding="utf-8")

    # 2. Parse all table rows | T<slice>.<num> | deliverables | refs | acceptance |
    row_re = re.compile(r"^\|\s*(T[1-6]\.\d{2})\s*\|\s*(.*?)\s*\|\s*(.*?)\s*\|\s*(.*?)\s*\|?\s*$")
    count = 1
    for line in SPEC_HANDOFF.read_text(encoding="utf-8").splitlines():
        m = row_re.match(line.strip())
        if not m:
            continue
        task_id, deliverables, refs, acceptance = m.group(1), m.group(2), m.group(3), m.group(4)
        prefix = task_id[:2]
        slice_title = SLICE_HEADERS.get(prefix, "الشريحة العامة")
        content = format_task_markdown(task_id, slice_title, deliverables, refs, acceptance)
        (TASKS_DIR / f"{task_id}.md").write_text(content, encoding="utf-8")
        count += 1

    print(f"[gen-tasks] OK: Generated {count} task files in docs/tasks/ (T0.01 + S1..S6).")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
