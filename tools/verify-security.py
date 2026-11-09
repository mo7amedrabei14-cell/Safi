#!/usr/bin/env python3
"""
tools/verify-security.py — Automated Security, Supply Chain & Scope Governance Verifier (`T1.00`)
Verifies:
1. Exact dependency pinning & disabled lifecycle scripts (AGENTS.md Rule 6).
2. Section 19.4 deferred directory rules (no premature server/, apps/desktop, apps/site, apps/admin, or non-S1 crates before the owning slice delivers them).
3. T1.00 scope discipline (no premature T1.01-T1.20 or T3.14 implementation files, no duplicate root/docs files).
4. T0.01 task files completeness in docs/tasks/ (110 tasks: T0.01 + S1..S6).
5. Honest 23-tool manifest classification in .agents/tools-manifest.json (Section 20.1).
6. Zero TODO/FIXME/HACK markers in shipped code (AGENTS.md Rule 2).
"""

from __future__ import annotations

import json
import pathlib
import re
import sys


ROOT = pathlib.Path(__file__).resolve().parent.parent

DEFERRED_PATHS_S1 = [
    "server",
    "apps/desktop",
    "apps/site",
    "apps/admin",
    "crates/sf-calc",
    "crates/sf-detect",
    "crates/sf-pii",
    "crates/sf-ffi",
]

FORBIDDEN_OVERSCOPE_PATHS_T1_00 = [
    "packages/i18n/locales",
    "packages/ui/src/react-bits",
    "packages/motion/src/gsap-timelines.ts",
    "packages/motion/src/lenis-provider.tsx",
    "packages/chart-studio/src/graphify",
    "apps/web/public/_headers",
]

FORBIDDEN_DUPLICATE_DOCS = [
    "DECISIONS.md",
    "COMMANDS.md",
    "docs/CHANGELOG.md",
]

VALID_TOOL_STATUSES = {
    "configured_in_t1_00",
    "deferred_to_task_or_slice",
}


def check_npmrc() -> list[str]:
    errors: list[str] = []
    npmrc = ROOT / ".npmrc"
    if not npmrc.exists():
        return ["Missing .npmrc file at repository root."]
    text = npmrc.read_text(encoding="utf-8")
    if "save-exact=true" not in text:
        errors.append(".npmrc must enforce save-exact=true (AGENTS.md Rule 6).")
    if "ignore-scripts=true" not in text:
        errors.append(".npmrc must enforce ignore-scripts=true (AGENTS.md Rule 6).")
    return errors


def check_package_json_pinning() -> list[str]:
    errors: list[str] = []
    pkg_files = (
        [ROOT / "package.json"]
        + sorted(ROOT.glob("packages/*/package.json"))
        + sorted(ROOT.glob("apps/*/package.json"))
    )
    for pkg_file in pkg_files:
        if not pkg_file.exists():
            continue
        data = json.loads(pkg_file.read_text(encoding="utf-8"))
        scripts = data.get("scripts", {})
        if "postinstall" in scripts or "preinstall" in scripts:
            errors.append(
                f"{pkg_file.relative_to(ROOT)} defines forbidden install lifecycle script."
            )
        for section in ("dependencies", "devDependencies", "optionalDependencies"):
            deps = data.get(section, {})
            for name, ver in deps.items():
                if ver.startswith("workspace:"):
                    continue
                if any(ch in ver for ch in ("^", "~", "*", ">", "<")):
                    errors.append(
                        f"{pkg_file.relative_to(ROOT)} -> {section}.{name} uses non-exact version '{ver}'."
                    )
    return errors


def check_cargo_pinning() -> list[str]:
    errors: list[str] = []
    cargo_files = [ROOT / "Cargo.toml"] + sorted(ROOT.glob("crates/*/Cargo.toml"))
    version_re = re.compile(r'version\s*=\s*"([^"]+)"')
    simple_dep_re = re.compile(r'^([a-zA-Z0-9_-]+)\s*=\s*"([^"]+)"\s*$')
    for cargo_file in cargo_files:
        if not cargo_file.exists():
            continue
        in_deps = False
        for line_no, raw_line in enumerate(
            cargo_file.read_text(encoding="utf-8").splitlines(), start=1
        ):
            line = raw_line.strip()
            if not line or line.startswith("#"):
                continue
            if line.startswith("["):
                in_deps = "dependencies" in line
                continue
            if not in_deps:
                continue
            m_simple = simple_dep_re.match(line)
            if m_simple:
                dep_name, ver = m_simple.group(1), m_simple.group(2)
                if not ver.startswith("="):
                    errors.append(
                        f"{cargo_file.relative_to(ROOT)}:{line_no} dependency '{dep_name}' version '{ver}' must be pinned with '='."
                    )
            for m_ver in version_re.finditer(line):
                ver = m_ver.group(1)
                if "path" in line and ver == "0.1.0":
                    continue
                if not ver.startswith("="):
                    errors.append(
                        f"{cargo_file.relative_to(ROOT)}:{line_no} dependency version '{ver}' must be pinned with '='."
                    )
    return errors


def is_documented_delivery(rel_path: str) -> bool:
    """A path that was out of scope for T1.00 is allowed once a later task report claims it."""
    reports_dir = ROOT / "docs/reports"
    if not reports_dir.exists():
        return False
    name = pathlib.Path(rel_path).name
    for report in sorted(reports_dir.glob("T*.md")):
        if report.name in {"T1.00.md"}:
            continue
        text = report.read_text(encoding="utf-8")
        if rel_path in text or name in text:
            return True
    return False


def is_path_claimed_by_delivery(rel_path: str) -> bool:
    """A deferred path is allowed once a later task report names that exact path."""
    reports_dir = ROOT / "docs/reports"
    if not reports_dir.exists():
        return False
    pattern = re.compile(r"(?<![\w/.-])" + re.escape(rel_path) + r"(?![\w.-])")
    for report in sorted(reports_dir.glob("T*.md")):
        if report.name in {"T1.00.md"}:
            continue
        if pattern.search(report.read_text(encoding="utf-8")):
            return True
    return False


def check_deferred_and_overscope_paths() -> list[str]:
    errors: list[str] = []
    for rel in DEFERRED_PATHS_S1:
        if (ROOT / rel).exists() and not is_path_claimed_by_delivery(rel):
            errors.append(
                f"Deferred directory '{rel}' exists before its owning slice delivered it, "
                "violating Section 19.4 deferral rule."
            )
    for rel in FORBIDDEN_OVERSCOPE_PATHS_T1_00:
        if (ROOT / rel).exists() and not is_documented_delivery(rel):
            errors.append(
                f"Over-scope path '{rel}' exists in T1.00 and is not claimed by any later task report."
            )
    for rel in FORBIDDEN_DUPLICATE_DOCS:
        if (ROOT / rel).exists():
            errors.append(
                f"Duplicate documentation file '{rel}' exists; only canonical paths are permitted."
            )
    return errors


def check_tasks_generated() -> list[str]:
    errors: list[str] = []
    tasks_dir = ROOT / "docs/tasks"
    handoff_path = ROOT / "docs/spec/19-handoff.md"
    if not tasks_dir.exists():
        return ["Missing docs/tasks directory (T0.01)."]
    if not handoff_path.exists():
        return ["Missing docs/spec/19-handoff.md; cannot verify generated task coverage."]

    # Validate the canonical generated set from Section 19, while permitting
    # separately numbered owner tasks (for example T0.00 and T0.05).
    handoff = handoff_path.read_text(encoding="utf-8")
    canonical_ids = {"T0.01"}
    task_id_pattern = re.compile(r"^\|\s*(T[1-6]\.\d{2})\s*\|", re.MULTILINE)
    canonical_ids.update(task_id_pattern.findall(handoff))

    expected_count = 110
    if len(canonical_ids) != expected_count:
        errors.append(
            f"Section 19 must define {expected_count} canonical task IDs (T0.01 + S1..S6); found {len(canonical_ids)}."
        )

    missing = sorted(task_id for task_id in canonical_ids if not (tasks_dir / f"{task_id}.md").exists())
    if missing:
        errors.append(
            "Missing canonical generated task files in docs/tasks/: " + ", ".join(missing)
        )

    if not (tasks_dir / "T1.00.md").exists():
        errors.append("Missing docs/tasks/T1.00.md.")
    return errors


def check_23_tools_manifest() -> list[str]:
    errors: list[str] = []
    manifest_path = ROOT / ".agents/tools-manifest.json"
    if not manifest_path.exists():
        return ["Missing .agents/tools-manifest.json."]
    data = json.loads(manifest_path.read_text(encoding="utf-8"))
    tools = data.get("tools", [])
    if len(tools) != 23:
        errors.append(
            f"Expected 23 tools in .agents/tools-manifest.json, found {len(tools)}."
        )
    for tool in tools:
        status = tool.get("status")
        if status not in VALID_TOOL_STATUSES:
            errors.append(
                f"Tool #{tool.get('id')} ({tool.get('name')}) has invalid status '{status}'."
            )
        active_path = tool.get("active_path")
        if status == "configured_in_t1_00":
            if not active_path or not (ROOT / active_path).exists():
                errors.append(
                    f"Tool #{tool.get('id')} ({tool.get('name')}) is marked configured_in_t1_00 but '{active_path}' is missing."
                )
        elif status == "deferred_to_task_or_slice":
            if not tool.get("target_task"):
                errors.append(
                    f"Tool #{tool.get('id')} ({tool.get('name')}) is deferred but missing 'target_task'."
                )
            if active_path and not (ROOT / active_path).exists():
                errors.append(
                    f"Tool #{tool.get('id')} ({tool.get('name')}) staged path '{active_path}' does not exist."
                )
    return errors


def check_no_todos_in_code() -> list[str]:
    errors: list[str] = []
    todo_re = re.compile(r"\b(TODO|FIXME|HACK)\b")
    for folder in ("crates", "packages", "apps"):
        base = ROOT / folder
        if not base.exists():
            continue
        for ext in ("*.rs", "*.ts", "*.tsx"):
            for fpath in base.rglob(ext):
                parts = fpath.relative_to(ROOT).parts
                if any(part in ("node_modules", "dist", "target") for part in parts):
                    continue
                text = fpath.read_text(encoding="utf-8")
                if todo_re.search(text):
                    errors.append(
                        f"Forbidden TODO/FIXME marker in {fpath.relative_to(ROOT)} (AGENTS.md Rule 2)."
                    )
    return errors


def check_canonical_files() -> list[str]:
    errors: list[str] = []
    required = [
        "AGENTS.md",
        "OWNER_DECISIONS.md",
        "README.md",
        "CHANGELOG.md",
        "Makefile",
        "errors.toml",
        "recipe.schema.json",
        "packages/brand/brand.config.ts",
        "docs/DECISIONS.md",
        "docs/COMMANDS.md",
        "docs/LOGGING.md",
        "docs/SECURITY.md",
        "docs/THREAT_MODEL.md",
        "docs/capacity.md",
        "docs/spec-full.md",
        ".agents/skills/cloudflare-security/SKILL.md",
        "tests/security/strix-runner.yaml",
        ".github/workflows/ci.yml",
    ]
    for rel in required:
        if not (ROOT / rel).exists():
            errors.append(f"Missing required canonical file: {rel}")
    return errors


def main() -> int:
    all_errors: list[str] = []
    all_errors.extend(check_npmrc())
    all_errors.extend(check_package_json_pinning())
    all_errors.extend(check_cargo_pinning())
    all_errors.extend(check_deferred_and_overscope_paths())
    all_errors.extend(check_tasks_generated())
    all_errors.extend(check_23_tools_manifest())
    all_errors.extend(check_no_todos_in_code())
    all_errors.extend(check_canonical_files())

    if all_errors:
        for err in all_errors:
            print(f"ERROR [verify-security]: {err}", file=sys.stderr)
        return 1

    print(
        "[verify-security] OK: T0.01 (110 canonical tasks from Section 19; supplementary owner tasks are allowed), T1.00 scope discipline, canonical docs, 23-tool status manifest, exact version pinning, and Section 19.4 deferral rules verified."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
