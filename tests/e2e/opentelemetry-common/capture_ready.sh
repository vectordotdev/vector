#!/bin/sh
# Healthcheck for the output Collector in the OpenTelemetry E2E suites. Compose starts
# each suite's `*-ready` service only after this passes, so tests read complete captures.
#
# Usage: capture_ready.sh DIR CHECK...
#   count:FILE:KEY:MIN  DIR/FILE has at least MIN occurrences of the JSON key "KEY"
#
# Checks are lower bounds: the Rust tests assert exact counts, so duplicates fail there
# instead of stalling this healthcheck.
set -u

dir=$1
shift
status=0
report=""

for check in "$@"; do
    case $check in
    count:*)
        spec=${check#count:}
        file=${spec%%:*}
        spec=${spec#*:}
        key=${spec%%:*}
        min=${spec#*:}
        found=$(grep -o "\"$key\"" "$dir/$file" 2>/dev/null | wc -l)
        report="$report $file=$found/$min"
        [ "$found" -ge "$min" ] || status=1
        ;;
    *)
        echo "unknown check: $check" >&2
        exit 2
        ;;
    esac
done

echo "${report# }"
exit "$status"
