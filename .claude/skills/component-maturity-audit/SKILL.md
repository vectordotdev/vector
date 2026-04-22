---
name: component-maturity-audit
description: Produce a report evaluating Vector sources/transforms/sinks against the alpha/beta/stable rubric. Use when reviewing component maturity on a cadence (quarterly/per-release) or when vetting a single component for promotion or demotion.
disable-model-invocation: true
argument-hint: [--component <name>] [--kind sources|transforms|sinks] [--only-changes] [--include-stable]
allowed-tools: Bash(gh *), Bash(git log *), Bash(git diff *), Bash(git show *), Bash(cargo vdev int show), Bash(make *), Bash(.claude/skills/component-maturity-audit/scripts/*), Bash(grep *), Bash(rg *), Bash(find *), Bash(ls *), Bash(wc *), Bash(awk *), Bash(sort *), Bash(uniq *), Bash(jq *), Bash(mkdir *), Read, Write, Edit, Glob, Grep
---

Evaluate Vector component maturity (alpha/beta/stable) against a rubric and produce a report. Arguments: `$ARGUMENTS`

## Scope

**Stable components are out of scope by default.** Stable is the terminal tier and demoting is a rare, noisy event; spending audit effort on 80+ stable components each run wastes time and context. Skip them unless the user passes `--include-stable` or names a specific stable component via `--component <name>`. The default audit is effectively a promotion review of `alpha`, `unset`, `beta`, and `deprecated` entries.

## Output

Produce a markdown report at `docs/component-maturity/report-YYYY-MM-DD.md` grouped into three sections:

- **Promote** — components whose signals justify a higher maturity tier
- **Demote** — components showing regression signals for their current tier
- **Unchanged** — everything else (one-line per component)

Each Promote/Demote entry MUST include:

1. Component name, kind (source/transform/sink), current tier, proposed tier
2. Evidence: the specific signals that triggered the proposal (issue counts, test presence, fix density, etc.)
3. The exact file + field to change (e.g., `website/cue/reference/components/sinks/postgres.cue` — `development: "beta"` → `"stable"`)
4. Confidence: `high` (all criteria met/failed), `medium` (most), `low` (needs human judgment)

NEVER edit CUE files yourself. The report is the output; a human flips the field.

## Rubric

If `docs/specs/component_maturity.md` exists in the repo, read it and use it as the source of truth. Otherwise fall back to this default:

- **alpha** — compiles, unit tests present. No stability guarantees. Breaking changes allowed between minor releases.
- **beta** — meets alpha + (a) integration test exists under `scripts/integration/<name>/` or component is trivial enough to not need one (document why), (b) user-facing docs complete per `docs/specs/component.md`, (c) has shipped in at least one release, (d) no open correctness bugs older than 90 days, (e) event instrumentation passes `cargo vdev check events`.
- **stable** — meets beta + (a) has shipped in at least two minor releases, (b) no breaking config changes in last two minor releases, (c) fix-to-feature ratio over last 4 releases does not exceed 2:1, (d) no open correctness/data-loss bugs older than 180 days, (e) at least 3 months since last breaking change.
- **deprecated** — replacement is documented in component support notes; removal release is targeted.

**Regression (demotion) triggers:**
- `stable` → `beta`: P0/P1 correctness bug open >180 days, or breaking config change shipped in last minor release.
- `beta` → `alpha`: rubric fields in the CUE file are incomplete or the component has been silent (no commits, no changelog entries) for 4+ consecutive minor releases.

## Steps

### 1. Parse arguments

- `--component <name>`: audit only this one component (match against CUE filename stem).
- `--kind sources|transforms|sinks`: restrict to a single kind.
- `--only-changes`: final report suppresses the Unchanged section.
- No args: audit every component.

### 2. Enumerate components and current tiers

Run the collector script — it regenerates `website/data/docs.json` (via `make generate-component-docs` + `make -C website structured-data`) and emits a TSV of `<kind>\t<name>\t<tier>`:

```bash
.claude/skills/component-maturity-audit/scripts/collect-components.sh > /tmp/vmat-components.tsv
```

Use `--no-build` on repeat runs within a session to skip regeneration (the JSON is ~13 MB and regeneration takes minutes). Use `--json` if you want the structured form for further `jq` work.

Filter the TSV per `--component` / `--kind` args before proceeding. The `tier` column is the authoritative current value; any component missing `development` is reported as `unset` and should be treated as alpha by the rubric.

The CUE file for each component (needed later to cite the exact edit target in the report) lives at `website/cue/reference/components/<kind>/<name>.cue`.

### 3. Collect signals (bulk, one pass)

Gather all signals into an in-memory table before any LLM-style reasoning. Keep bash output compact — do not dump raw issue bodies into context.

Per-component signals to collect:

**GitHub issues** (via `gh`):
```bash
# Open issues for a component (label name matches the kind: "source: kafka", "sink: kafka", etc.)
gh issue list --repo vectordotdev/vector --label "<kind-singular>: <name>" --state open --limit 200 \
  --json number,title,createdAt,labels,updatedAt \
  > /tmp/vmat-issues-<kind>-<name>.json

# Derive: total_open, oldest_age_days, p0_p1_count (labels containing "priority: high" or "type: bug")
```

Do NOT fetch issues for every component upfront if unrestricted — that's 120+ API calls. Only fetch issues for components whose tier-shift is plausible from cheaper signals (integration-test presence, recent commits, changelog density). For a full audit without `--component`, batch the `gh` calls and cache to `/tmp/vmat-issues-*.json` so re-runs in the same day are cheap.

**Integration-test presence**:
```bash
cargo vdev int show | tail -n +3 | awk '{print $1}' > /tmp/vmat-int-tests.txt
# then grep each component name against this list
```

**Recent shipping history** — walk the last 4 release CUE files:
```bash
ls -1 website/cue/reference/releases/*.cue | sort -V | tail -4
```
Count changelog entries per component by matching the component name in the `description` of each entry, classified by fragment type (`fix`, `feature`, `enhancement`, `breaking`). A beta component with zero entries across 4 releases is a silence signal.

**Git activity**:
```bash
git log --since="1 year ago" --oneline -- src/<kind>/<name>/ | wc -l
git log -1 --format=%cI -- src/<kind>/<name>/
```
Note: some components live in single files (e.g. `src/sinks/<name>.rs`) rather than directories; try both paths and merge.

**Unreleased changelog fragments**:
```bash
grep -l "<name>" changelog.d/*.{fix,feature,enhancement,breaking,deprecation,security}.md 2>/dev/null
```

### 4. Apply the rubric

For each component, compare its current tier against the rubric using the collected signals. Classify as `promote`, `demote`, or `unchanged`, and assign confidence:

- `high`: every criterion for the proposed tier has a definitive yes/no from the signals.
- `medium`: one criterion requires human judgment (e.g., "docs complete" — you have heuristics but not certainty).
- `low`: you are flagging a candidate but the rubric is ambiguous here; human should review.

When `--component` is given, be thorough: read the component's CUE file and RS source to sanity-check the signals. When running across all components, stay in bulk-signal mode and only deep-dive on proposed promotions/demotions.

### 5. Write the report

Write to `docs/component-maturity/report-YYYY-MM-DD.md` (create the directory if missing). Use this structure:

```markdown
# Component Maturity Audit — YYYY-MM-DD

Rubric source: `<docs/specs/component_maturity.md or "skill default">`
Components audited: N
Proposed promotions: N · Proposed demotions: N · Unchanged: N

## Promote

### <kind>/<name>: <current> → <proposed> (confidence: <level>)

- **File:** `website/cue/reference/components/<kind>/<name>.cue`
- **Change:** `development: "<current>"` → `"<proposed>"`
- **Evidence:**
  - <signal 1>
  - <signal 2>

## Demote

<same structure>

## Unchanged

<one line per component: `- <kind>/<name> (<tier>)`>  (omit section if --only-changes)

## Signal coverage

- Integration tests matched: <count>
- gh issue fetches: <count> (<cached_count> from cache)
- Components with no git activity in last year: <count>
```

### 6. Summarize for the user

After writing the file, print in the chat:
- Path to the report
- One-line summary: `N promotions, M demotions proposed`
- Up to 5 highest-confidence proposed changes, as a preview

Do NOT commit the report or stage it. The user decides what to do with it.

## Constraints

- This skill is read-only for the repo. It writes one new markdown file under `docs/component-maturity/`. It never edits CUE files, never commits, never pushes, never opens issues or PRs.
- If `gh` is not authenticated, skip issue-based signals and note this in the report's "Signal coverage" section — do not abort.
- If running a full audit, cap `gh issue list` calls at 200 components total; if the list is longer, prefer candidate filtering via cheaper signals first.
- The report must be reproducible: any signal cited must be something a human can re-run the exact command for. Include the commands or issue numbers inline.
