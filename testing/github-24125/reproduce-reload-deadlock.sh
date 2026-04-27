#!/usr/bin/env bash
# Faithful repro of vectordotdev/vector#24125: Vector fails to reload sink with failed events.
#
# Production scenario being recreated:
#   - One source (demo_logs) routed via a route transform to two http sinks.
#   - One sink ("good") receives 200; the other ("bad") begins returning 429 after a delay,
#     simulating a downstream that throttles/refuses requests. 429 is in the http sink's
#     default retriable set, so events back up in the bad sink rather than being dropped.
#   - After the bad sink has been failing for a while, the operator edits a benign field
#     (encoding.except_fields) on both sinks and SIGHUPs Vector.
#   - The reload stalls indefinitely. All sinks stop sending. Only restarting Vector recovers.
#
# A separate metrics pipeline (internal_metrics -> prometheus_exporter) is included so we can
# poll per-component event counters; we also tap router.all to observe live event flow.
#
# At four checkpoints we probe the system (tap + metric deltas) and classify the state as
# GREEN (normal) or RED (malfunction). A colorized summary prints at the end.
#   T1 — initial startup
#   T2 — after the fouled sink starts returning 429
#   T3 — after the first SIGHUP (no config change, baseline reload)
#   T4 — after the config change and SIGHUP (the bug-trigger)
#
# Files in this directory:
#   receiver.py            tiny HTTP listener used as both receivers
#   base-config.yaml       static parts of the Vector config (sources, transforms, metrics sink)
#   http-sinks.yaml.tmpl   templated http sinks (rendered per-run)
#
# Usage:
#   ./testing/github-24125/reproduce-reload-deadlock.sh <image>
#
# Exit codes:
#   0 — bug reproduced (post-edit reload did not complete within the floor)
#   1 — reload completed (image is unaffected/patched)
#   2 — inconclusive (setup failed or baseline reload didn't complete)

set -euo pipefail

if [ $# -lt 1 ]; then
  echo "Usage: $0 <image>" >&2
  echo "  e.g. $0 timberio/vector:0.50.0-debian" >&2
  exit 64
fi

IMAGE="$1"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RECEIVER_PY="$SCRIPT_DIR/receiver.py"
BASE_YAML="$SCRIPT_DIR/base-config.yaml"
SINKS_TMPL="$SCRIPT_DIR/http-sinks.yaml.tmpl"

for f in "$RECEIVER_PY" "$BASE_YAML" "$SINKS_TMPL"; do
  if [ ! -f "$f" ]; then
    echo "ERROR: missing required file: $f" >&2
    exit 2
  fi
done

NETWORK="vector-repro-24125-net"
VECTOR_C="vector-repro-24125"
GOOD_C="receiver-good-24125"
BAD_C="receiver-bad-24125"
WORKDIR="$(mktemp -d)"
FAIL_AFTER_SEC="${FAIL_AFTER_SEC:-10}"
SOAK_BEFORE_RELOAD_SEC="${SOAK_BEFORE_RELOAD_SEC:-15}"
BASELINE_RELOAD_MAX_SEC="${BASELINE_RELOAD_MAX_SEC:-120}"
# We hard-set Vector's graceful shutdown limit (VECTOR_GRACEFUL_SHUTDOWN_LIMIT_SECS)
# to 20s so the test isn't subject to upstream default drift. The post-edit reload
# floor is that value + 10s — well above any healthy reload time. Note: as of this
# writing the graceful shutdown limit only applies to SIGINT/SIGTERM, not to per-
# reload sink drain (which has no timeout), so this floor is a heuristic, not a
# strict upper bound implied by Vector's code. See src/topology/running.rs.
GRACEFUL_SHUTDOWN_LIMIT_SEC="${GRACEFUL_SHUTDOWN_LIMIT_SEC:-20}"
POST_EDIT_RELOAD_FLOOR_SEC="${POST_EDIT_RELOAD_FLOOR_SEC:-30}"
# These ports are baked into vector-base.yaml; changing them here also requires editing the
# static config. They're variables only so the docker port mappings stay in one place.
API_PORT=8686
PROM_PORT=9598
SCRAPE_INTERVAL_SEC=5

if [ -t 1 ]; then
  RED=$'\033[0;31m'; GREEN=$'\033[0;32m'; BOLD=$'\033[1m'; RESET=$'\033[0m'
else
  RED=''; GREEN=''; BOLD=''; RESET=''
fi

RESULTS=()  # each entry: STATUS|ID|LABEL|NOTE|DETAIL

cleanup() {
  for c in "$VECTOR_C" "$GOOD_C" "$BAD_C"; do
    docker rm -f "$c" >/dev/null 2>&1 || true
  done
  docker network rm "$NETWORK" >/dev/null 2>&1 || true
  rm -rf "$WORKDIR"
}
trap cleanup EXIT

# Render the http-sinks template with the current host names and except_fields list.
# $1 = comma-separated except_fields (e.g. "garbage_initial,garbage_post_edit")
render_sinks() {
  python3 - "$SINKS_TMPL" "$GOOD_C" "$BAD_C" "$1" <<'PYEOF'
import sys
tmpl_path, good_host, bad_host, fields_csv = sys.argv[1:5]
fields = [f.strip() for f in fields_csv.split(',') if f.strip()]
except_yaml = '\n'.join(f'        - {f}' for f in fields)
with open(tmpl_path) as f:
    out = f.read()
out = out.replace('@@GOOD_HOST@@', good_host)
out = out.replace('@@BAD_HOST@@', bad_host)
out = out.replace('@@EXCEPT_FIELDS@@', except_yaml)
sys.stdout.write(out)
PYEOF
}

write_config() {
  # $1 = comma-separated except_fields
  cat "$BASE_YAML" > "$WORKDIR/vector.yaml"
  render_sinks "$1" >> "$WORKDIR/vector.yaml"
}

count_log_marker() {
  docker logs "$VECTOR_C" 2>&1 | grep -c "New configuration loaded successfully" || true
}

wait_for_reload_completion() {
  # $1 = baseline count of "New configuration loaded successfully"
  # $2 = max wait seconds
  # echoes elapsed seconds on success, returns 1 on timeout
  local baseline="$1" max="$2"
  local start now elapsed cur
  start=$(date +%s)
  while :; do
    cur=$(count_log_marker)
    if [ "$cur" -gt "$baseline" ]; then
      now=$(date +%s); elapsed=$((now - start))
      echo "$elapsed"
      return 0
    fi
    now=$(date +%s); elapsed=$((now - start))
    if [ "$elapsed" -ge "$max" ]; then
      return 1
    fi
    sleep 1
  done
}

scrape_metric() {
  # Prometheus exposition lines look like:
  #   metric_name{labels} value [timestamp_ms]
  # Field $2 is always the value; $NF would be the timestamp when present.
  local metric="$1" component="$2"
  local val
  val=$(curl -fs --max-time 3 "http://localhost:${PROM_PORT}/metrics" 2>/dev/null \
    | grep -E "^${metric}\{[^}]*component_id=\"${component}\"" \
    | head -1 \
    | awk '{print int($2)}') || true
  echo "${val:-0}"
}

scrape_inbound() {
  local component="$1" v
  v=$(scrape_metric vector_component_received_events_total "$component")
  if [ "$v" -gt 0 ]; then echo "$v"; return; fi
  v=$(scrape_metric vector_events_in_total "$component")
  echo "${v:-0}"
}

scrape_outbound() {
  local component="$1" v
  v=$(scrape_metric vector_component_sent_events_total "$component")
  if [ "$v" -gt 0 ]; then echo "$v"; return; fi
  v=$(scrape_metric vector_events_out_total "$component")
  echo "${v:-0}"
}

# probe_state probe_id label expectation
#   expectation=healthy → GREEN if events flowing AND sink_good in/out delta > 0
#   expectation=either  → GREEN if any flow detected; RED only if fully stalled (use for T4)
probe_state() {
  local probe_id="$1" label="$2" expectation="$3"
  echo
  echo "${BOLD}==> [${probe_id}] PROBE: ${label}${RESET}"

  local good_in_b bad_in_b good_out_b bad_out_b
  good_in_b=$(scrape_inbound sink_good)
  bad_in_b=$(scrape_inbound sink_bad)
  good_out_b=$(scrape_outbound sink_good)
  bad_out_b=$(scrape_outbound sink_bad)
  echo "    waiting $((SCRAPE_INTERVAL_SEC + 1))s for next metric scrape..."
  sleep $((SCRAPE_INTERVAL_SEC + 1))

  local good_in_a bad_in_a good_out_a bad_out_a
  good_in_a=$(scrape_inbound sink_good)
  bad_in_a=$(scrape_inbound sink_bad)
  good_out_a=$(scrape_outbound sink_good)
  bad_out_a=$(scrape_outbound sink_bad)

  local d_good_in=$((good_in_a - good_in_b))
  local d_bad_in=$((bad_in_a - bad_in_b))
  local d_good_out=$((good_out_a - good_out_b))
  local d_bad_out=$((bad_out_a - bad_out_b))

  local taps tap_out
  echo "    tapping router.all for 3s..."
  tap_out=$(docker exec "$VECTOR_C" timeout 3 vector tap --quiet router.all 2>/dev/null || true)
  taps=$(printf '%s\n' "$tap_out" | grep -c '^{' || true)

  echo "    tap events:    ${taps}"
  echo "    sink_good in:  +${d_good_in}, out: +${d_good_out}"
  echo "    sink_bad  in:  +${d_bad_in}, out: +${d_bad_out}"

  local detail="taps=${taps} good(in/out)=+${d_good_in}/+${d_good_out} bad(in/out)=+${d_bad_in}/+${d_bad_out}"
  local status note
  case "$expectation" in
    healthy)
      if [ "$taps" -gt 0 ] && [ "$d_good_in" -gt 0 ] && [ "$d_good_out" -gt 0 ]; then
        status=GREEN; note="events flowing; sink_good healthy"
      else
        status=RED;   note="expected healthy flow but found stall"
      fi
      ;;
    either)
      if [ "$taps" -gt 0 ] && [ "$d_good_in" -gt 0 ] && [ "$d_good_out" -gt 0 ]; then
        status=GREEN; note="events flowing post-reload; topology recovered"
      else
        status=RED;   note="topology deadlocked after reload (bug reproduced)"
      fi
      ;;
  esac
  echo "    => ${status}: ${note}"
  RESULTS+=("${status}|${probe_id}|${label}|${note}|${detail}")
}

print_summary() {
  echo
  echo "============================================================"
  echo "${BOLD}SUMMARY${RESET}"
  echo "============================================================"
  local any_red=false
  for r in "${RESULTS[@]}"; do
    IFS='|' read -r status pid label note detail <<< "$r"
    local color
    if [ "$status" = "RED" ]; then any_red=true; color="$RED"; else color="$GREEN"; fi
    printf "%s[%s]%s %s — %s\n" "$color" "$status" "$RESET" "$pid" "$label"
    printf "       %s\n" "$note"
    printf "       %s\n" "$detail"
  done
  echo "------------------------------------------------------------"
  if $any_red; then
    printf "%sRESULT: MALFUNCTION DETECTED%s\n" "$RED" "$RESET"
  else
    printf "%sRESULT: NORMAL OPERATION%s\n" "$GREEN" "$RESET"
  fi
  echo "============================================================"
}

# ============================================================================

echo "==> image:                 $IMAGE"
echo "==> bad sink fails after:  ${FAIL_AFTER_SEC}s"
echo "==> soak before reload:    ${SOAK_BEFORE_RELOAD_SEC}s"
echo

echo "==> creating docker network"
docker network create "$NETWORK" >/dev/null

echo "==> starting receivers (good = always 200; bad = 429 after ${FAIL_AFTER_SEC}s)"
docker run -d --name "$GOOD_C" --network "$NETWORK" \
  -e PORT=8001 \
  -v "$RECEIVER_PY:/receiver.py:ro" \
  python:3.11-slim python /receiver.py >/dev/null
docker run -d --name "$BAD_C" --network "$NETWORK" \
  -e PORT=8001 -e FAIL_AFTER="$FAIL_AFTER_SEC" \
  -v "$RECEIVER_PY:/receiver.py:ro" \
  python:3.11-slim python /receiver.py >/dev/null

for c in "$GOOD_C" "$BAD_C"; do
  for _ in $(seq 1 30); do
    if docker logs "$c" 2>&1 | grep -q "receiver listening"; then break; fi
    sleep 0.5
  done
done

write_config "garbage_initial"
echo "==> starting Vector (graceful shutdown limit: ${GRACEFUL_SHUTDOWN_LIMIT_SEC}s)"
docker run -d --name "$VECTOR_C" --network "$NETWORK" \
  -p "${API_PORT}:${API_PORT}" \
  -p "${PROM_PORT}:${PROM_PORT}" \
  -e VECTOR_GRACEFUL_SHUTDOWN_LIMIT_SECS="$GRACEFUL_SHUTDOWN_LIMIT_SEC" \
  -v "$WORKDIR:/etc/vector" \
  "$IMAGE" >/dev/null

started=false
for _ in $(seq 1 60); do
  if docker logs "$VECTOR_C" 2>&1 | grep -q "Vector has started"; then
    started=true; break
  fi
  if ! docker ps --format '{{.Names}}' | grep -q "^${VECTOR_C}$"; then
    echo "ERROR: Vector container exited during startup" >&2
    docker logs "$VECTOR_C" 2>&1 | tail -40 >&2
    exit 2
  fi
  sleep 0.5
done
if [ "$started" != true ]; then
  echo "ERROR: timed out waiting for Vector to start" >&2
  docker logs "$VECTOR_C" 2>&1 | tail -40 >&2
  exit 2
fi

# Allow at least one scrape interval so prometheus has data to compare against.
sleep "$SCRAPE_INTERVAL_SEC"

if [ -n "${DEBUG_METRICS:-}" ]; then
  echo
  echo "==> [debug] sample of vector_*_total metrics for sinks:"
  curl -fs --max-time 3 "http://localhost:${PROM_PORT}/metrics" 2>/dev/null \
    | grep -E '^vector_(component_(sent|received)_events|events_(in|out))_total\{[^}]*component_id="(sink_good|sink_bad)"' \
    | sort | head -20 || true
fi

probe_state T1 "initial startup" healthy

echo
echo "==> sleeping ${FAIL_AFTER_SEC}s for bad sink to begin failing, then ${SOAK_BEFORE_RELOAD_SEC}s soak"
sleep "$FAIL_AFTER_SEC"
echo "==> bad sink should now be returning 429; soaking ${SOAK_BEFORE_RELOAD_SEC}s..."
sleep "$SOAK_BEFORE_RELOAD_SEC"

probe_state T2 "after fouled sink starts returning 429" healthy

echo
echo "==> [baseline] sending SIGHUP with no config change to time a healthy reload"
baseline_count=$(count_log_marker)
docker kill -s SIGHUP "$VECTOR_C" >/dev/null
if baseline_elapsed=$(wait_for_reload_completion "$baseline_count" "$BASELINE_RELOAD_MAX_SEC"); then
  echo "==> [baseline] reload completed in ${baseline_elapsed}s"
else
  echo "${RED}ERROR: baseline reload did not complete within ${BASELINE_RELOAD_MAX_SEC}s${RESET}" >&2
  echo "       (this can happen on already-broken Vector versions; consider it a strong signal too)" >&2
  docker logs "$VECTOR_C" 2>&1 | tail -20 >&2
  RESULTS+=("RED|T3|after baseline SIGHUP|baseline reload itself stalled|n/a")
  print_summary
  exit 2
fi

sleep 2
probe_state T3 "after baseline SIGHUP (no config change)" healthy

echo
echo "==> editing config: adding 'garbage_post_edit' to encoding.except_fields on both sinks"
write_config "garbage_initial,garbage_post_edit"

post_edit_max=$(awk -v b="$baseline_elapsed" -v floor="$POST_EDIT_RELOAD_FLOOR_SEC" \
  'BEGIN { v = int(b * 1.5 + 0.5); if (v < floor) v = floor; print v }')
echo "==> [post-edit] sending SIGHUP; will wait up to ${post_edit_max}s (max(1.5*baseline, ${POST_EDIT_RELOAD_FLOOR_SEC}s))"
post_count=$(count_log_marker)
docker kill -s SIGHUP "$VECTOR_C" >/dev/null

if post_elapsed=$(wait_for_reload_completion "$post_count" "$post_edit_max"); then
  echo "==> [post-edit] reload completed in ${post_elapsed}s — image appears patched"
  reload_outcome=patched
else
  echo "${RED}==> [post-edit] reload did not complete within ${post_edit_max}s — bug indicator${RESET}"
  reload_outcome=stalled
fi

sleep 2
probe_state T4 "after config-change SIGHUP" either

print_summary

if [ "$reload_outcome" = "stalled" ]; then
  exit 0
else
  exit 1
fi
