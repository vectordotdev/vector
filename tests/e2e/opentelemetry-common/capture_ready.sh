#!/bin/sh
# Healthcheck for the output Collector in the OpenTelemetry E2E suites. Compose starts
# each suite's `*-ready` service only after this passes, so tests read complete captures.
#
# Usage: capture_ready.sh DIR CHECK...
#   count:FILE:KEY:MIN  DIR/FILE has at least MIN occurrences of the JSON key "KEY"
#   received:MIN        the latest component_received_events_total for source0 in
#                       DIR/vector-internal-metrics-sink.log is at least MIN
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
    received:*)
        min=${check#received:}
        found=$(grep '"name":"component_received_events_total"' "$dir/vector-internal-metrics-sink.log" 2>/dev/null |
            grep '"component_id":"source0"' | tail -n 1 | sed -E 's/.*"value":([0-9.]+).*/\1/')
        found=${found:-0}
        report="$report received=$found/$min"
        awk -v found="$found" -v min="$min" 'BEGIN { exit !(found >= min) }' || status=1
        ;;
    *)
        echo "unknown check: $check" >&2
        exit 2
        ;;
    esac
done

echo "${report# }"
exit "$status"
