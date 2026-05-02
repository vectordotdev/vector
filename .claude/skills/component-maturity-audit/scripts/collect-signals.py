#!/usr/bin/env python3
"""Collect maturity signals for Vector components.

Reads:
  /tmp/vmat-components.tsv  (produced by collect-components.sh)
  /tmp/vmat-int-tests.txt   (produced from `cargo vdev int show`)
  website/data/docs.json    (produced by collect-components.sh via make)

Writes:
  /tmp/vmat-signals.json    structured data for downstream classification

Also prints a compact table to stdout.

Usage (from repo root):
  python3 .claude/skills/component-maturity-audit/scripts/collect-signals.py [--tier beta|alpha|all]

By default, only audits components with tier != "stable" and != "deprecated".
"""

import argparse
import glob
import json
import re
import subprocess
from pathlib import Path


DOCS_PATH = Path("website/data/docs.json")
COMPONENTS_TSV = Path("/tmp/vmat-components.tsv")
INT_TESTS_TXT = Path("/tmp/vmat-int-tests.txt")
OUT_PATH = Path("/tmp/vmat-signals.json")


# Some components live under a shared parent directory or with an unusual filename.
# Map: (kind, name) -> list of candidate paths (under src/).
PATH_OVERRIDES: dict[tuple[str, str], list[str]] = {
    ("sinks", "gcp_chronicle_unstructured"): ["src/sinks/gcp_chronicle/"],
    ("sinks", "gcp_stackdriver_logs"): ["src/sinks/gcp/stackdriver/logs/", "src/sinks/gcp/stackdriver/"],
    ("sinks", "gcp_stackdriver_metrics"): ["src/sinks/gcp/stackdriver/metrics/", "src/sinks/gcp/stackdriver/"],
    ("sinks", "gcp_pubsub"): ["src/sinks/gcp/pubsub.rs", "src/sinks/gcp/pubsub/"],
    ("sinks", "gcp_cloud_storage"): ["src/sinks/gcp/cloud_storage.rs", "src/sinks/gcp/cloud_storage/"],
    ("sinks", "greptimedb_logs"): ["src/sinks/greptimedb/logs/", "src/sinks/greptimedb/"],
    ("sinks", "greptimedb_metrics"): ["src/sinks/greptimedb/metrics/", "src/sinks/greptimedb/"],
    ("sinks", "datadog_events"): ["src/sinks/datadog/events/"],
    ("sinks", "datadog_logs"): ["src/sinks/datadog/logs/"],
    ("sinks", "datadog_metrics"): ["src/sinks/datadog/metrics/"],
    ("sinks", "datadog_traces"): ["src/sinks/datadog/traces/"],
    ("sinks", "prometheus_exporter"): ["src/sinks/prometheus/exporter/", "src/sinks/prometheus/"],
    ("sinks", "prometheus_remote_write"): ["src/sinks/prometheus/remote_write/", "src/sinks/prometheus/"],
    ("sources", "prometheus_scrape"): ["src/sources/prometheus/scrape.rs", "src/sources/prometheus/"],
    ("sources", "prometheus_pushgateway"): ["src/sources/prometheus/pushgateway.rs", "src/sources/prometheus/"],
    ("sources", "prometheus_remote_write"): ["src/sources/prometheus/remote_write.rs", "src/sources/prometheus/"],
    ("sources", "aws_kinesis_firehose"): ["src/sources/aws_kinesis_firehose/"],
}


def candidate_paths(kind: str, name: str) -> list[str]:
    if (kind, name) in PATH_OVERRIDES:
        return PATH_OVERRIDES[(kind, name)]
    return [
        f"src/{kind}/{name}/",
        f"src/{kind}/{name}.rs",
    ]


def component_in_description(desc: str, name: str, singular: str) -> bool:
    """Return True when the description plausibly refers to this component."""
    esc = re.escape(name)
    # Canonical prefix: `name` source / `name` sink / `name` transform
    if re.search(rf'`{esc}`\s+{singular}s?\b', desc):
        return True
    # Grouped mentions: `name1` and `name2` sinks
    if re.search(rf'`{esc}`[^`]*?(?:and|,)\s*`[a-z0-9_]+`\s+{singular}s?\b', desc):
        return True
    if re.search(rf'`[a-z0-9_]+`\s*(?:and|,)\s*`{esc}`\s+{singular}s?\b', desc):
        return True
    return False


def load_beta_components(tier_filter: str) -> list[tuple[str, str, str]]:
    out = []
    for line in COMPONENTS_TSV.read_text().splitlines():
        parts = line.split("\t")
        if len(parts) < 3:
            continue
        kind, name, tier = parts[0], parts[1], parts[2]
        if tier_filter == "non-stable":
            if tier in ("stable", "deprecated"):
                continue
        elif tier_filter != "all" and tier != tier_filter:
            continue
        out.append((kind, name, tier))
    return out


def load_int_tests() -> set[str]:
    if not INT_TESTS_TXT.exists():
        return set()
    return {l.strip() for l in INT_TESTS_TXT.read_text().splitlines() if l.strip()}


def has_int_test(name: str, int_tests: set[str]) -> tuple[bool, list[str]]:
    cands = {
        name,
        name.replace("_", "-"),
        name.split("_")[0],
    }
    matches = [c for c in cands if c in int_tests]
    return bool(matches), matches


def git_activity(kind: str, name: str) -> dict:
    existing = [p for p in candidate_paths(kind, name) if Path(p).exists()]
    if not existing:
        return {"commits_1yr": 0, "last_commit": None, "paths": []}
    last = subprocess.run(
        ["git", "log", "-1", "--format=%cI", "--"] + existing,
        capture_output=True, text=True,
    ).stdout.strip()
    count = subprocess.run(
        ["git", "log", "--since=1 year ago", "--oneline", "--"] + existing,
        capture_output=True, text=True,
    ).stdout.strip()
    return {
        "commits_1yr": len([l for l in count.split("\n") if l.strip()]),
        "last_commit": last,
        "paths": existing,
    }


def unreleased_fragments(name: str) -> list[tuple[str, str]]:
    matches = []
    for ext in ("fix", "feature", "enhancement", "breaking", "deprecation", "security", "chore"):
        for fpath in glob.glob(f"changelog.d/*.{ext}.md"):
            try:
                txt = Path(fpath).read_text()
            except Exception:
                continue
            if re.search(rf'`{re.escape(name)}`', txt):
                matches.append((fpath, ext))
    return matches


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--tier", default="non-stable",
                    help="Filter: non-stable (default), beta, alpha, unset, or all")
    args = ap.parse_args()

    docs = json.loads(DOCS_PATH.read_text())

    all_releases = sorted(
        [v for v in docs["releases"].keys() if re.match(r'^\d+\.\d+\.\d+$', v)],
        key=lambda s: tuple(int(p) for p in s.split(".")),
    )
    minors = [v for v in all_releases if v.endswith(".0")]
    recent4 = minors[-4:]
    recent8 = minors[-8:]
    print(f"Recent 4 minors: {recent4}", flush=True)

    beta = load_beta_components(args.tier)
    print(f"Components to audit: {len(beta)}", flush=True)

    int_tests = load_int_tests()

    per_kind_counts: dict[tuple[str, str], dict[str, dict]] = {}
    breaking_flags: dict[tuple[str, str], list[str]] = {}
    first_release: dict[tuple[str, str], str] = {}

    # Phrases in a "chore" entry that indicate a breaking config change.
    breaking_phrases = re.compile(
        r'(has been removed|have been removed|removed in this release|renamed|replaced|migrated|is now required|must now|now must|must be explicitly|no longer|breaking change)',
        re.IGNORECASE,
    )

    for rver in all_releases:
        entries = docs["releases"][rver].get("changelog", []) or []
        for e in entries:
            desc = e.get("description", "") or ""
            etype = e.get("type", "")
            breaking = e.get("breaking", False)
            # Chore entries often encode breaking config changes without the flag set.
            if etype == "chore" and breaking_phrases.search(desc):
                breaking = True
            for kind, name, _tier in beta:
                singular = kind.rstrip("s")
                if not component_in_description(desc, name, singular):
                    continue
                key = (kind, name)
                bucket = per_kind_counts.setdefault(key, {}).setdefault(
                    rver, {"total": 0, "types": {}},
                )
                bucket["total"] += 1
                bucket["types"][etype] = bucket["types"].get(etype, 0) + 1
                if breaking:
                    breaking_flags.setdefault(key, []).append(rver)
                if key not in first_release:
                    first_release[key] = rver

    # "New X source/sink" mentions in release descriptions
    new_pat = re.compile(r'[Nn]ew\s+`([a-z0-9_]+)`\s+(source|sink|transform)')
    beta_set = {(k, n) for k, n, _ in beta}
    for rver in all_releases:
        desc = docs["releases"][rver].get("description", "") or ""
        for m in new_pat.finditer(desc):
            key = (m.group(2) + "s", m.group(1))
            if key in beta_set:
                cur = first_release.get(key)
                if cur is None or tuple(int(p) for p in rver.split(".")) < tuple(int(p) for p in cur.split(".")):
                    first_release[key] = rver

    results = []
    for kind, name, tier in beta:
        key = (kind, name)
        prc = per_kind_counts.get(key, {})
        all_shipped_minors = [r for r in minors if r in prc]

        total = feat = enh = fix = 0
        for r in recent4:
            rdata = prc.get(r, {"total": 0, "types": {}})
            total += rdata["total"]
            t = rdata["types"]
            feat += t.get("feat", 0)
            enh += t.get("enhancement", 0)
            fix += t.get("fix", 0)
        breaking_recent4 = [r for r in breaking_flags.get(key, []) if r in recent4]

        itest, itest_matches = has_int_test(name, int_tests)
        git = git_activity(kind, name)
        frags = unreleased_fragments(name)

        results.append({
            "kind": kind,
            "name": name,
            "tier": tier,
            "first_release": first_release.get(key),
            "shipped_minors_count": len(all_shipped_minors),
            "shipped_minors": all_shipped_minors,
            "recent4_entries": total,
            "recent4_by_type": {"feat": feat, "enhancement": enh, "fix": fix},
            "recent4_breakdown": {r: prc.get(r, {"total": 0})["total"] for r in recent4},
            "breaking_recent4": breaking_recent4,
            "has_int_test": itest,
            "int_test_match": itest_matches,
            "git_commits_1yr": git["commits_1yr"],
            "git_last_commit": git["last_commit"],
            "git_paths": git["paths"],
            "unreleased_fragments": [[p, t] for p, t in frags],
        })

    OUT_PATH.write_text(json.dumps(results, indent=2))
    print(f"\nWrote {OUT_PATH}", flush=True)
    print(f"\n{'kind':<11} {'name':<30} {'shipN':>5} {'last_commit':<11} {'1yr':>4} {'r4ent':>5} {'brk':>3} {'int':>3} {'frag':>4}")
    for r in results:
        lc = (r["git_last_commit"] or "")[:10]
        print(
            f"{r['kind']:<11} {r['name']:<30} {r['shipped_minors_count']:>5} {lc:<11} "
            f"{r['git_commits_1yr']:>4} {r['recent4_entries']:>5} "
            f"{len(r['breaking_recent4']):>3} {'Y' if r['has_int_test'] else 'n':>3} "
            f"{len(r['unreleased_fragments']):>4}"
        )


if __name__ == "__main__":
    main()
