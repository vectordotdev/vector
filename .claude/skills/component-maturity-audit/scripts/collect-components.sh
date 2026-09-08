#!/usr/bin/env bash
# Collect every Vector component and its current maturity tier.
#
# Regenerates website/data/docs.json via the website structured-data pipeline
# and emits a TSV to stdout: <kind>\t<name>\t<tier>
#
# Usage:
#   collect-components.sh [--no-build]   # skip regeneration, read existing JSON
#   collect-components.sh --json         # emit JSON array instead of TSV
#
# Must be run from the repo root.

set -euo pipefail

BUILD=1
FORMAT=tsv

for arg in "$@"; do
  case "$arg" in
    --no-build) BUILD=0 ;;
    --json) FORMAT=json ;;
    -h|--help)
      sed -n '2,12p' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *) echo "unknown argument: $arg" >&2; exit 2 ;;
  esac
done

if [[ ! -f Makefile || ! -d website ]]; then
  echo "error: run from the repo root (Makefile + website/ must exist)" >&2
  exit 1
fi

if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq is required" >&2
  exit 1
fi

JSON=website/data/docs.json

if [[ "$BUILD" -eq 1 ]]; then
  echo "regenerating $JSON ..." >&2
  make generate-component-docs >&2
  make -C website structured-data >&2
fi

if [[ ! -f "$JSON" ]]; then
  echo "error: $JSON missing. Re-run without --no-build." >&2
  exit 1
fi

# Emit one row per component across sources, transforms, sinks.
# Tier is .classes.development; missing tiers are reported as "unset"
# (the rubric treats unset as alpha).
if [[ "$FORMAT" == "json" ]]; then
  jq '
    [
      (.components | to_entries[]) as $kind
      | ($kind.value | to_entries[]) as $comp
      | {
          kind: $kind.key,
          name: $comp.key,
          tier: ($comp.value.classes.development // "unset")
        }
    ]
  ' "$JSON"
else
  jq -r '
    .components
    | to_entries[]
    | .key as $kind
    | .value | to_entries[]
    | [$kind, .key, (.value.classes.development // "unset")]
    | @tsv
  ' "$JSON" | sort
fi
