#!/usr/bin/env python3
"""Render the final audit report from the classification + signals JSON.

Writes docs/component-maturity/report-YYYY-MM-DD.md.

Usage:
  write-report.py [--only-changes]
"""

import argparse
import json
from datetime import date
from pathlib import Path


SIGNALS = Path("/tmp/vmat-signals.json")
CLASSIFICATION = Path("/tmp/vmat-classification.json")
ISSUES_SUMMARY = Path("/tmp/vmat-issues-summary.tsv")
COMPONENTS_TSV = Path("/tmp/vmat-components.tsv")
INT_TESTS_TXT = Path("/tmp/vmat-int-tests.txt")
RUBRIC_PATH = Path("docs/specs/component_maturity.md")


def load_issues():
    import csv
    out = {}
    if not ISSUES_SUMMARY.exists():
        return out
    with ISSUES_SUMMARY.open() as f:
        reader = csv.DictReader(f, delimiter="\t")
        for row in reader:
            out[(row["kind"], row["name"])] = {
                "total_open": int(row["total_open"]),
                "oldest_age_days": int(row["oldest_age_days"]),
                "p0_p1_bugs": int(row["p0_p1_bugs"]),
                "oldest_p0_p1_age": int(row["oldest_p0_p1_age"]),
            }
    return out


def load_all_components() -> list[tuple[str, str, str]]:
    out = []
    for line in COMPONENTS_TSV.read_text().splitlines():
        parts = line.split("\t")
        if len(parts) >= 3:
            out.append((parts[0], parts[1], parts[2]))
    return out


def render_entry(prop, sig, issue):
    kind = prop["kind"]
    name = prop["name"]
    cue = f"website/cue/reference/components/{kind}/{name}.cue"
    lines = [
        f"### {kind}/{name}: {prop['current']} → {prop['proposed']} (confidence: {prop['confidence']})",
        "",
        f"- **File:** `{cue}`",
        f"- **Change:** `development: \"{prop['current']}\"` → `\"{prop['proposed']}\"`",
        "- **Evidence:**",
    ]

    shipped = sig["shipped_minors_count"]
    minors = ", ".join(sig["shipped_minors"][-6:]) or "none in recent history"
    lines.append(f"  - shipped in {shipped} minor releases (recent: {minors})")

    r4 = sig["recent4_by_type"]
    lines.append(
        f"  - last 4 minors: {sig['recent4_entries']} changelog entries "
        f"(feat={r4['feat']}, enhance={r4['enhancement']}, fix={r4['fix']})"
    )

    if sig["breaking_recent4"]:
        lines.append(f"  - breaking in last 4 minors: {', '.join(sig['breaking_recent4'])}")
    else:
        lines.append("  - breaking in last 4 minors: none detected")

    if sig["has_int_test"]:
        lines.append(f"  - integration test match: `{', '.join(sig['int_test_match'])}` (under `scripts/integration/`)")
    else:
        if kind == "transforms":
            lines.append("  - integration test: not required (transform is pure-Rust)")
        else:
            lines.append("  - integration test: **missing** under `scripts/integration/`")

    lines.append(
        f"  - git activity: {sig['git_commits_1yr']} commits past year, "
        f"last commit {sig['git_last_commit'] or 'unknown'} "
        f"(paths: {', '.join(sig['git_paths']) or 'none'})"
    )

    if issue is not None:
        lines.append(
            f"  - open issues: {issue['total_open']} total, oldest {issue['oldest_age_days']}d; "
            f"confirmed/priority bugs: {issue['p0_p1_bugs']}, oldest {issue['oldest_p0_p1_age']}d "
            f"(`gh issue list --repo vectordotdev/vector --label '{kind.rstrip('s')}: {name}' --state open`)"
        )
    else:
        lines.append("  - open issues: not fetched")

    if sig["unreleased_fragments"]:
        frags = ", ".join(f"{Path(p).name}" for p, _t in sig["unreleased_fragments"])
        lines.append(f"  - unreleased changelog fragments: {frags}")

    # Rubric-check bullet list
    if prop["action"] == "demote":
        lines.append(
            "  - demotion trigger: silent for 4+ consecutive minors "
            "(no changelog entries) and low git activity — rubric says "
            "`beta → alpha` when component has no commits/changelog for "
            "4+ consecutive minor releases"
        )
    elif prop["failures"]:
        lines.append(f"  - failed criteria: {', '.join(prop['failures'])}")
    else:
        lines.append("  - rubric: all promotion criteria satisfied")

    # Pulled evidence from classify.py
    if prop["evidence"]:
        pass  # The headline summary above captures most; avoid duplication

    lines.append("")
    return lines


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--only-changes", action="store_true")
    args = ap.parse_args()

    sigs = {(s["kind"], s["name"]): s for s in json.loads(SIGNALS.read_text())}
    props = json.loads(CLASSIFICATION.read_text())
    issues = load_issues()
    all_components = load_all_components()

    rubric_src = str(RUBRIC_PATH) if RUBRIC_PATH.exists() else "skill default (no `docs/specs/component_maturity.md` present)"

    today = date.today().isoformat()
    promote = [p for p in props if p["action"] == "promote"]
    demote = [p for p in props if p["action"] == "demote"]
    # Unchanged includes every component in the full registry, not just those evaluated.
    # Per user guidance, stable components are not audited — they appear in Unchanged as-is.
    evaluated_keys = {(p["kind"], p["name"]) for p in props}

    out_dir = Path("docs/component-maturity")
    out_dir.mkdir(parents=True, exist_ok=True)
    out_path = out_dir / f"report-{today}.md"

    lines: list[str] = []
    lines.append(f"# Component Maturity Audit — {today}")
    lines.append("")
    lines.append(f"Rubric source: `{rubric_src}`")
    lines.append(f"Components in registry: {len(all_components)}")
    lines.append(f"Components audited: {len(props)} (stable and deprecated skipped per request)")
    lines.append(f"Proposed promotions: {len(promote)} · Proposed demotions: {len(demote)} · Unchanged: {len(all_components) - len(promote) - len(demote)}")
    lines.append("")
    lines.append("> **Note:** This report is read-only output. A human reviewer flips the CUE `development:` field after reviewing each proposal. Do not merge promotions/demotions mechanically.")
    lines.append("")

    # Promote
    lines.append("## Promote")
    lines.append("")
    if not promote:
        lines.append("_No promotions proposed._")
        lines.append("")
    else:
        # Sort: high confidence first, then by kind/name
        conf_order = {"high": 0, "medium": 1, "low": 2}
        promote.sort(key=lambda p: (conf_order[p["confidence"]], p["kind"], p["name"]))
        for p in promote:
            sig = sigs[(p["kind"], p["name"])]
            issue = issues.get((p["kind"], p["name"]))
            lines.extend(render_entry(p, sig, issue))

    # Demote
    lines.append("## Demote")
    lines.append("")
    if not demote:
        lines.append("_No demotions proposed._")
        lines.append("")
    else:
        demote.sort(key=lambda p: (p["kind"], p["name"]))
        for p in demote:
            sig = sigs[(p["kind"], p["name"])]
            issue = issues.get((p["kind"], p["name"]))
            lines.extend(render_entry(p, sig, issue))

    # Unchanged
    if not args.only_changes:
        lines.append("## Unchanged")
        lines.append("")
        lines.append("Components that either passed without change or were skipped (stable/deprecated).")
        lines.append("")
        # List every component, marking source of "unchanged"
        changed = {(p["kind"], p["name"]) for p in (promote + demote)}
        audited = evaluated_keys
        for kind, name, tier in sorted(all_components):
            key = (kind, name)
            if key in changed:
                continue
            if key in audited:
                # Audited but unchanged — beta failed to promote, or alpha failed to promote
                suffix = " (audited)"
            else:
                suffix = " (skipped)"
            lines.append(f"- {kind}/{name} ({tier}){suffix}")
        lines.append("")

    # Signal coverage
    lines.append("## Signal coverage")
    lines.append("")
    int_tests_count = len(INT_TESTS_TXT.read_text().split()) if INT_TESTS_TXT.exists() else 0
    fetched = sum(1 for k in sigs if (k in issues))
    lines.append(f"- Integration test suites enumerated via `cargo vdev int show`: {int_tests_count}")
    lines.append(f"- `gh issue list` calls: {len(issues)} (cached in `/tmp/vmat-issues-*.json`)")
    no_git = sum(1 for s in sigs.values() if s["git_commits_1yr"] == 0)
    lines.append(f"- Components with no git activity in last year: {no_git}")
    lines.append("- Release changelog scanned: last 4 minors from `website/data/docs.json`")
    lines.append("- Breaking-change detection: `.breaking == true` OR chore entries matching `removed/renamed/replaced/migrated/must (now|be explicitly)/no longer/breaking change`")
    lines.append("")
    lines.append("### Reproduce")
    lines.append("")
    lines.append("```sh")
    lines.append(".claude/skills/component-maturity-audit/scripts/collect-components.sh --no-build > /tmp/vmat-components.tsv")
    lines.append("cargo vdev int show | tail -n +3 | awk '{print $1}' | sort -u > /tmp/vmat-int-tests.txt")
    lines.append("python3 .claude/skills/component-maturity-audit/scripts/collect-signals.py")
    lines.append(".claude/skills/component-maturity-audit/scripts/fetch-issues.sh          # reuses /tmp cache")
    lines.append("python3 .claude/skills/component-maturity-audit/scripts/classify.py")
    lines.append("python3 .claude/skills/component-maturity-audit/scripts/write-report.py")
    lines.append("```")
    lines.append("")

    out_path.write_text("\n".join(lines))
    print(f"Wrote {out_path}")
    print(f"Promotions: {len(promote)}, Demotions: {len(demote)}")


if __name__ == "__main__":
    main()
