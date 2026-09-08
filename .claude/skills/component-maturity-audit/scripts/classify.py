#!/usr/bin/env python3
"""Apply the maturity rubric to collected signals and propose promotions/demotions.

Reads:
  /tmp/vmat-signals.json
  /tmp/vmat-issues-summary.tsv  (optional; issue signals skipped if missing)

Writes:
  /tmp/vmat-classification.json

Rubric (from the skill default — edit if docs/specs/component_maturity.md exists):

  alpha:
    - compiles + unit tests present. No stability guarantees.

  beta (requires alpha + all of):
    a) integration test present OR trivial enough that one isn't needed (document why)
    b) user-facing docs complete
    c) shipped in ≥1 release
    d) no open correctness bugs >90 days
    e) event instrumentation passes `cargo vdev check events`

  stable (requires beta + all of):
    a) shipped in ≥2 minor releases
    b) no breaking config changes in last 2 minor releases
    c) fix-to-feature ratio over last 4 releases ≤ 2:1
    d) no open correctness/data-loss bugs >180 days
    e) ≥3 months since last breaking change

  demotion triggers:
    stable→beta: P0/P1 correctness bug open >180d, OR breaking config change
                 shipped in last minor release
    beta→alpha:  rubric fields in CUE file incomplete, OR component silent
                 (no commits, no changelog) for 4+ consecutive minor releases
"""

import csv
import json
import sys
from dataclasses import dataclass, field
from datetime import datetime
from pathlib import Path


SIGNALS = Path("/tmp/vmat-signals.json")
ISSUES_SUMMARY = Path("/tmp/vmat-issues-summary.tsv")
OUT = Path("/tmp/vmat-classification.json")


@dataclass
class Proposal:
    kind: str
    name: str
    current: str
    proposed: str
    action: str  # promote / demote / unchanged
    confidence: str  # high / medium / low
    evidence: list[str] = field(default_factory=list)
    failures: list[str] = field(default_factory=list)


def load_issues() -> dict[tuple[str, str], dict]:
    out: dict[tuple[str, str], dict] = {}
    if not ISSUES_SUMMARY.exists():
        return out
    with ISSUES_SUMMARY.open() as f:
        reader = csv.DictReader(f, delimiter="\t")
        for row in reader:
            key = (row["kind"], row["name"])
            out[key] = {
                "total_open": int(row["total_open"]),
                "oldest_age_days": int(row["oldest_age_days"]),
                "p0_p1_bugs": int(row["p0_p1_bugs"]),
                "oldest_p0_p1_age": int(row["oldest_p0_p1_age"]),
            }
    return out


def classify(sig: dict, issue: dict | None) -> Proposal:
    kind = sig["kind"]
    name = sig["name"]
    current = sig["tier"]
    r4_total = sig["recent4_entries"]
    r4 = sig["recent4_by_type"]
    feat_plus_enh = r4["feat"] + r4["enhancement"]
    fix = r4["fix"]
    breaking_recent = sig["breaking_recent4"]
    shipped_minors = sig["shipped_minors_count"]
    has_int = sig["has_int_test"]
    commits_1yr = sig["git_commits_1yr"]
    last_commit = sig["git_last_commit"]
    unreleased = sig["unreleased_fragments"]

    p = Proposal(kind=kind, name=name, current=current, proposed=current, action="unchanged", confidence="high")

    # --- Detect silence (for beta→alpha demotion) ---
    silent = (r4_total == 0) and (shipped_minors <= 1) and (not unreleased)
    low_activity = commits_1yr < 5

    # --- Evaluate promotion beta → stable ---
    if current == "beta":
        criteria = {}
        criteria["shipped_≥2_minors"] = shipped_minors >= 2
        criteria["no_breaking_in_recent4"] = len(breaking_recent) == 0
        # fix-to-feature ratio ≤ 2:1 (treat 0/0 as "pass" since silent)
        if feat_plus_enh == 0 and fix == 0:
            criteria["fix_to_feat_ratio_ok"] = True  # no data, neutral
            fix_ratio_note = "no changelog activity — neutral"
        else:
            # Avoid div by zero; if no features+enhancements, any fixes fail
            ratio_ok = (fix <= 2 * feat_plus_enh) if feat_plus_enh > 0 else (fix == 0)
            criteria["fix_to_feat_ratio_ok"] = ratio_ok
            fix_ratio_note = f"{fix} fix / {feat_plus_enh} feat+enh in last 4 minors"
        # beta precondition: int test exists OR component is trivial (transforms)
        criteria["int_test_present"] = has_int or kind == "transforms"

        # Issue-based criteria
        p01_ok = True
        p01_note = "no issue data"
        if issue is not None:
            # stable (d): no open correctness/data-loss bugs > 180 days
            p01_ok = not (issue["oldest_p0_p1_age"] > 180)
            p01_note = (
                f"{issue['p0_p1_bugs']} confirmed/priority bugs, "
                f"oldest {issue['oldest_p0_p1_age']}d"
            )
        criteria["no_p01_bugs_gt_180d"] = p01_ok

        # Additional beta precondition d: no open correctness bugs >90 days
        # We don't have issue priority granularity; rely on p0_p1 check.

        all_pass = all(criteria.values())
        evidence = []
        for k, v in criteria.items():
            evidence.append(f"{'PASS' if v else 'FAIL'} {k}")
        evidence.append(f"shipping: {shipped_minors} minors, first_release={sig['first_release']}")
        evidence.append(f"recent4_entries={r4_total} ({fix_ratio_note})")
        evidence.append(f"breaking_recent4={breaking_recent or 'none'}")
        evidence.append(f"int_test_match={sig['int_test_match'] or 'none'}")
        evidence.append(f"git: {commits_1yr} commits past yr, last={last_commit}")
        if issue is not None:
            evidence.append(f"issues: {issue['total_open']} open, oldest {issue['oldest_age_days']}d, {p01_note}")
        else:
            evidence.append("issues: not fetched")

        # Check for silent→alpha demotion
        if silent and low_activity and last_commit and "2025" in (last_commit or ""):
            p.action = "demote"
            p.proposed = "alpha"
            p.confidence = "medium"
            p.evidence = evidence + ["silent: no changelog entries in last 4 minors, <5 commits/year"]
            return p

        if all_pass:
            p.action = "promote"
            p.proposed = "stable"
            # Confidence: how many criteria passed with strong signal
            strong = shipped_minors >= 3 and has_int and (issue is not None) and p01_ok
            p.confidence = "high" if strong else ("medium" if shipped_minors >= 2 else "low")
            p.evidence = evidence
        else:
            p.action = "unchanged"
            p.proposed = "beta"
            p.confidence = "high"
            p.evidence = evidence
            p.failures = [k for k, v in criteria.items() if not v]

    elif current == "alpha" or current == "unset":
        # Evaluate alpha → beta
        criteria = {}
        criteria["shipped_≥1_minor"] = shipped_minors >= 1
        criteria["int_test_present"] = has_int
        if issue is not None:
            criteria["no_p01_bugs_gt_90d"] = not (issue["oldest_p0_p1_age"] > 90)
        else:
            criteria["no_p01_bugs_gt_90d"] = True  # neutral

        evidence = [f"{'PASS' if v else 'FAIL'} {k}" for k, v in criteria.items()]
        evidence.append(f"shipped in {shipped_minors} minors")
        if issue is not None:
            evidence.append(f"issues: {issue['total_open']} open, {issue['p0_p1_bugs']} confirmed bugs")

        if all(criteria.values()):
            p.action = "promote"
            p.proposed = "beta"
            p.confidence = "medium"  # doc completeness, events check not verified
            p.evidence = evidence
        else:
            p.action = "unchanged"
            p.proposed = current
            p.confidence = "high"
            p.evidence = evidence

    else:
        # stable / deprecated — skip (per user request)
        p.action = "unchanged"
        p.proposed = current
        p.confidence = "high"
        p.evidence = ["skipped: stable/deprecated components not audited"]

    return p


def main():
    sigs = json.loads(SIGNALS.read_text())
    issues = load_issues()

    proposals = []
    for sig in sigs:
        issue = issues.get((sig["kind"], sig["name"]))
        p = classify(sig, issue)
        proposals.append({
            "kind": p.kind,
            "name": p.name,
            "current": p.current,
            "proposed": p.proposed,
            "action": p.action,
            "confidence": p.confidence,
            "evidence": p.evidence,
            "failures": p.failures,
        })

    OUT.write_text(json.dumps(proposals, indent=2))
    promote = [x for x in proposals if x["action"] == "promote"]
    demote = [x for x in proposals if x["action"] == "demote"]
    unchanged = [x for x in proposals if x["action"] == "unchanged"]
    print(f"Wrote {OUT}")
    print(f"promote={len(promote)} demote={len(demote)} unchanged={len(unchanged)}")
    print("\n=== PROMOTE ===")
    for p in promote:
        print(f"  {p['kind']}/{p['name']}: {p['current']}→{p['proposed']} ({p['confidence']})")
    print("\n=== DEMOTE ===")
    for p in demote:
        print(f"  {p['kind']}/{p['name']}: {p['current']}→{p['proposed']} ({p['confidence']})")


if __name__ == "__main__":
    main()
