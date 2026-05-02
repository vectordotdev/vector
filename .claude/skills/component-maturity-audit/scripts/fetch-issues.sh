#!/usr/bin/env bash
# Fetch open issues for every non-stable component and cache them to /tmp.
#
# Uses label "<kind-singular>: <name>" (e.g., "sink: amqp", "source: kafka", "transform: dedupe").
# Caches to /tmp/vmat-issues-<kind>-<name>.json so repeat runs are cheap.
# Pass --force to overwrite cache.
#
# Input:  /tmp/vmat-components.tsv
# Output: /tmp/vmat-issues-<kind>-<name>.json for each non-stable component,
#         plus /tmp/vmat-issues-summary.tsv

set -euo pipefail

FORCE=0
for arg in "$@"; do
  case "$arg" in
    --force) FORCE=1 ;;
    *) echo "unknown arg: $arg" >&2; exit 2 ;;
  esac
done

if ! command -v gh >/dev/null 2>&1; then
  echo "error: gh CLI required" >&2
  exit 1
fi
if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq required" >&2
  exit 1
fi

TSV=/tmp/vmat-components.tsv
SUMMARY=/tmp/vmat-issues-summary.tsv
echo -e "kind\tname\ttotal_open\toldest_age_days\tp0_p1_bugs\toldest_p0_p1_age" > "$SUMMARY"

NOW=$(date +%s)

fetch_one() {
  local kind="$1" name="$2"
  local singular="${kind%s}"
  local out="/tmp/vmat-issues-${kind}-${name}.json"
  if [[ "$FORCE" -eq 0 && -f "$out" ]]; then
    return 0
  fi
  gh issue list --repo vectordotdev/vector \
    --label "${singular}: ${name}" \
    --state open --limit 200 \
    --json number,title,createdAt,updatedAt,labels \
    > "$out" 2>/dev/null || echo "[]" > "$out"
}

# Fetch in parallel batches (8 at a time) to stay under GH rate limits
pids=()
while IFS=$'\t' read -r kind name tier; do
  if [[ "$tier" == "stable" || "$tier" == "deprecated" ]]; then
    continue
  fi
  fetch_one "$kind" "$name" &
  pids+=($!)
  if (( ${#pids[@]} >= 8 )); then
    wait "${pids[@]}" 2>/dev/null || true
    pids=()
  fi
done < "$TSV"
wait "${pids[@]}" 2>/dev/null || true

# Build summary
while IFS=$'\t' read -r kind name tier; do
  if [[ "$tier" == "stable" || "$tier" == "deprecated" ]]; then
    continue
  fi
  in="/tmp/vmat-issues-${kind}-${name}.json"
  if [[ ! -f "$in" ]]; then
    echo -e "${kind}\t${name}\t0\t0\t0\t0" >> "$SUMMARY"
    continue
  fi
  total=$(jq 'length' "$in")
  oldest=$(jq -r 'map(.createdAt) | sort | first // ""' "$in")
  if [[ -n "$oldest" ]]; then
    oldest_s=$(date -d "$oldest" +%s 2>/dev/null || echo "$NOW")
    oldest_days=$(( (NOW - oldest_s) / 86400 ))
  else
    oldest_days=0
  fi

  # Count issues tagged as bug with priority high/critical OR just a confirmed bug
  p01=$(jq '[ .[] | select(
    ( [.labels[].name] | any(. == "type: bug") )
    and ( [.labels[].name] | any(. == "priority: high" or . == "priority: critical" or . == "meta: confirmed") )
  ) ] | length' "$in")

  oldest_p01=$(jq -r '[ .[] | select(
    ( [.labels[].name] | any(. == "type: bug") )
    and ( [.labels[].name] | any(. == "priority: high" or . == "priority: critical" or . == "meta: confirmed") )
  ) | .createdAt ] | sort | first // ""' "$in")
  if [[ -n "$oldest_p01" ]]; then
    oldest_p01_s=$(date -d "$oldest_p01" +%s 2>/dev/null || echo "$NOW")
    oldest_p01_days=$(( (NOW - oldest_p01_s) / 86400 ))
  else
    oldest_p01_days=0
  fi

  echo -e "${kind}\t${name}\t${total}\t${oldest_days}\t${p01}\t${oldest_p01_days}" >> "$SUMMARY"
done < "$TSV"

echo "wrote $SUMMARY" >&2
column -ts $'\t' "$SUMMARY"
