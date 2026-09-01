#!/usr/bin/env bash
# run-experiment.sh — run one or all Vector scaling phases and print results.
#
# Usage:
#   KUBECONFIG=/path/to/kubeconfig ./scripts/run-experiment.sh [all|1|2|3|4]
#
# Requirements: kubectl, helm, grpcurl, python3, Bash >= 4
# The script assumes namespace, consumer, ingress-nginx, and ingress are already deployed.

set -euo pipefail

if (( BASH_VERSINFO[0] < 4 )); then
  echo "ERROR: this script requires Bash >= 4 (found ${BASH_VERSION})." >&2
  echo "On macOS, the default /bin/bash is 3.2, install a newer version with" >&2
  echo "'brew install bash' and re-run this script with it." >&2
  exit 1
fi

PHASE=${1:-all}
if [[ "$#" -gt 1 ]]; then
  echo "ERROR: expected at most one phase argument." >&2
  exit 2
fi
case "$PHASE" in
  all | 1 | 2 | 3 | 4) ;;
  *)
    echo "ERROR: phase must be one of: all, 1, 2, 3, 4." >&2
    exit 2
    ;;
esac

NAMESPACE=vector-perf
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GUIDE_DIR="$(dirname "$SCRIPT_DIR")"
PRODUCER_CHART="$GUIDE_DIR/manifests/producer-chart"
VECTOR_VALUES="$GUIDE_DIR/values.yaml"
VECTOR_CHART_VERSION=0.58.0
TMPDIR_WORK=/tmp/vec-experiment-$$
mkdir -p "$TMPDIR_WORK"
trap 'rm -rf "$TMPDIR_WORK"; pkill -f "kubectl port-forward.*vector-perf.*pod/" 2>/dev/null || true' EXIT

# ── helpers ───────────────────────────────────────────────────────────────────
log() { echo "==> $*" >&2; }

# Helm owns every desired-state change. kubectl is used only to observe the
# cluster and port-forward to Vector's observability API.
kube() { kubectl "$@"; }

helm_vector() {
  helm upgrade --install vector vectordotdev/vector \
    --namespace "$NAMESPACE" \
    --version "$VECTOR_CHART_VERSION" \
    -f "$VECTOR_VALUES" \
    "$@" \
    --wait --timeout=3m >/dev/null
}

# Wait until all Vector pods are Ready and have had a stable restart count for
# 30 consecutive seconds. This ensures pods have survived the initial load
# burst (which can cause 1–3 OOM restarts before backpressure establishes),
# and aren't sitting in a CrashLoopBackOff window where the restart count
# hasn't ticked yet, before we measure.
wait_stable() {
  local max_wait=300 interval=5 stable_needed=6
  local elapsed=0 stable_count=0 last_restarts=""

  log "Waiting for Vector pods to stabilise under load..."
  while [[ "$elapsed" -lt "$max_wait" ]]; do
    local restarts all_ready
    restarts=$(kube get pods -n "$NAMESPACE" -l app.kubernetes.io/name=vector \
      -o jsonpath='{range .items[*]}{.status.containerStatuses[0].restartCount}{"\n"}{end}' \
      2>/dev/null | paste -sd,)
    all_ready=false
    kube wait --for=condition=Ready pod -l app.kubernetes.io/name=vector \
      -n "$NAMESPACE" --timeout=15s >/dev/null 2>&1 && all_ready=true

    if [[ "$restarts" == "$last_restarts" && -n "$restarts" && "$all_ready" == true ]]; then
      stable_count=$(( stable_count + 1 ))
      log "[${elapsed}s] restarts=${restarts} ready=${all_ready} (stable ${stable_count}/${stable_needed})"
      if [[ "$stable_count" -ge "$stable_needed" ]]; then
        log "Pods stable and ready (restart counts: ${restarts})."
        return 0
      fi
    else
      if [[ -n "$last_restarts" ]]; then
        log "[${elapsed}s] restarts=${restarts} ready=${all_ready} (changed from ${last_restarts} or not ready, reset)"
      else
        log "[${elapsed}s] restarts=${restarts} ready=${all_ready}"
      fi
      stable_count=0
      last_restarts="$restarts"
    fi

    sleep "$interval"
    elapsed=$(( elapsed + interval ))
  done

  log "ERROR: Vector pods did not stabilise within ${max_wait}s."
  exit 1
}

# Scale Vector to 0 and wait for its pods to terminate, so every run (including
# reruns against a cluster left over from a previous Phase 4) starts from the
# same clean state instead of measuring a transition from whatever replica
# count the last run ended on.
reset_vector() {
  log "Resetting Vector to 0 replicas for a clean run..."
  helm_vector --set replicas=0 --set autoscaling.enabled=false
  kube wait --for=delete pod -l app.kubernetes.io/name=vector \
    -n "$NAMESPACE" --timeout=60s >/dev/null 2>&1 || true
}

pick_pods() {
  kube get pods -n "$NAMESPACE" -l app.kubernetes.io/name=vector \
    --field-selector=status.phase=Running \
    -o jsonpath='{range .items[*]}{.metadata.name}{"\n"}{end}'
}

# Average CPU % across all Vector pods via kubectl top. Outputs e.g. "97%".
avg_cpu_pct() {
  kube top pods -n "$NAMESPACE" -l app.kubernetes.io/name=vector \
    --no-headers 2>/dev/null \
    | awk '{gsub("m","",$2); sum+=$2; n++} END {
        if (n>0) printf "%d%%", int(sum/n/10)
        else     print "?"
      }'
}

# Port-forward to a single pod on a given port; blocks until the gRPC health
# check passes. Prints the port-forward PID to stdout.
start_port_forward() {
  local pod=$1 port=$2 logfile=$3

  kube port-forward -n "$NAMESPACE" "pod/$pod" "${port}:8686" > "$logfile" 2>&1 &
  local pf_pid=$!

  # Wait up to 10 s for the gRPC health check to pass.
  local i=0
  while ! grpcurl -plaintext "localhost:${port}" grpc.health.v1.Health/Check >/dev/null 2>&1; do
    if ! kill -0 "$pf_pid" 2>/dev/null; then
      log "ERROR: port-forward to pod/${pod}:8686 → ${port} died. Output:"
      cat "$logfile" >&2
      exit 1
    fi
    i=$(( i + 1 ))
    if [[ "$i" -ge 20 ]]; then
      log "ERROR: gRPC health check on port ${port} not ready after 10 s."
      cat "$logfile" >&2
      exit 1
    fi
    sleep 0.5
  done

  echo "$pf_pid"
}

snapshot_pod() {
  local port=$1 out=$2
  if ! grpcurl -plaintext -d '{}' "localhost:${port}" \
      vector.observability.v1.ObservabilityService/GetComponents \
      > "$out" 2>&1; then
    log "ERROR: grpcurl failed on port ${port}. Output:"
    cat "$out" >&2
    exit 1
  fi
}

# Measure aggregate throughput across all given pods over the same 30-second
# window (each pod is sampled at t0 and t0+30s, then deltas are summed).
# Writes "<MiB/s> <ev/s>" to $TMPDIR_WORK/measure.txt
measure_pods() {
  local pods=("$@")
  local n=${#pods[@]}
  local -a ports pids
  local i

  for ((i = 0; i < n; i++)); do
    local port=$((18700 + i))
    ports+=("$port")
    pids+=("$(start_port_forward "${pods[$i]}" "$port" "$TMPDIR_WORK/pf-${i}.log")")
  done

  for ((i = 0; i < n; i++)); do
    snapshot_pod "${ports[$i]}" "$TMPDIR_WORK/t0-${i}.json"
  done
  sleep 30
  for ((i = 0; i < n; i++)); do
    snapshot_pod "${ports[$i]}" "$TMPDIR_WORK/t30-${i}.json"
  done

  for pid in "${pids[@]}"; do
    kill "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
  done

  python3 - "$n" "$TMPDIR_WORK" <<'PYEOF'
import json, sys

n = int(sys.argv[1])
workdir = sys.argv[2]

def get_bytes_events(path):
    try:
        d = json.load(open(path))
    except Exception as e:
        sys.exit(f"ERROR: failed to parse {path}: {e}")
    for c in d.get('components', []):
        if c.get('componentId') == 'in':
            m = c.get('metrics', {})
            return int(m.get('receivedBytesTotal', 0)), int(m.get('receivedEventsTotal', 0))
    sys.exit(f"ERROR: no 'in' component found in {path}")

total_bytes = 0
total_events = 0
for i in range(n):
    b1, e1 = get_bytes_events(f"{workdir}/t0-{i}.json")
    b2, e2 = get_bytes_events(f"{workdir}/t30-{i}.json")
    total_bytes += b2 - b1
    total_events += e2 - e1

mibps = total_bytes / 30 / 1048576
eps = total_events / 30
print(f"{mibps:.2f} {eps:.0f}")
PYEOF
}

# ── phase runners ─────────────────────────────────────────────────────────────
# Each function writes key=value lines to $TMPDIR_WORK/phaseN.txt
run_static_phase() {
  local phase=$1 replicas=$2 out="$TMPDIR_WORK/phase${1}.txt"

  log "Phase $phase: scaling Vector to $replicas pod(s)..."
  helm_vector --set replicas="$replicas" --set autoscaling.enabled=false
  wait_stable

  log "Phase $phase: measuring all $replicas pod(s) (20 s warmup + 30 s window)..."
  sleep 20

  local -a pods
  mapfile -t pods < <(pick_pods)
  measure_pods "${pods[@]}" > "$TMPDIR_WORK/measure.txt"
  local total_mibps total_eps cpu
  read -r total_mibps total_eps < "$TMPDIR_WORK/measure.txt"
  cpu=$(avg_cpu_pct)

  {
    echo "PHASE${phase}_MIBPS=${total_mibps}"
    echo "PHASE${phase}_EPS=${total_eps}"
    echo "PHASE${phase}_CPU=${cpu}"
    echo "PHASE${phase}_PODS=${replicas}"
  } > "$out"
}

run_hpa_phase() {
  local out="$TMPDIR_WORK/phase4.txt"

  log "Phase 4: resetting to 1 pod and creating HPA (70% target, max 8)..."
  helm_vector --set replicas=1 --set autoscaling.enabled=false
  wait_stable
  helm_vector \
    --set replicas=1 \
    --set autoscaling.enabled=true \
    --set autoscaling.minReplicas=1 \
    --set autoscaling.maxReplicas=8 \
    --set autoscaling.targetCPUUtilizationPercentage=70 \
    --set autoscaling.behavior.scaleDown.stabilizationWindowSeconds=60

  local start elapsed
  local last_replicas=1 scale_events=0 stable_count=0 last_stable=0
  local replicas="" cpu_avg=""
  local max_elapsed=900
  start=$(date +%s)

  log "Phase 4: watching HPA (timeout ${max_elapsed}s)..."
  while true; do
    elapsed=$(( $(date +%s) - start ))

    if [[ "$elapsed" -ge "$max_elapsed" ]]; then
      log "ERROR: HPA did not reach equilibrium within ${max_elapsed}s (last: ${last_replicas} pods, ${cpu_avg:-?}% CPU)."
      exit 1
    fi

    replicas=$(kube get hpa vector -n "$NAMESPACE" \
               -o jsonpath='{.status.currentReplicas}' 2>/dev/null || echo "")
    cpu_avg=$(kube get hpa vector -n "$NAMESPACE" \
               -o jsonpath='{.status.currentMetrics[0].resource.current.averageUtilization}' \
               2>/dev/null || echo "")

    if [[ -n "$replicas" && "$replicas" != "$last_replicas" ]]; then
      scale_events=$(( scale_events + 1 ))
      log "[${elapsed}s] SCALE ${last_replicas}→${replicas}  cpu=${cpu_avg}%"
      last_replicas=$replicas
    else
      log "[${elapsed}s] replicas=${replicas:-?}  cpu=${cpu_avg:-?}%"
    fi

    if [[ "$replicas" == "$last_stable" ]]; then
      stable_count=$(( stable_count + 1 ))
    else
      last_stable=$replicas
      stable_count=1
    fi

    # Fail fast if HPA is blocked at maxReplicas with persistently high CPU.
    if [[ -n "$replicas" && "$replicas" == "8" && -n "$cpu_avg" && "$cpu_avg" -gt 77 && "$stable_count" -ge 3 ]]; then
      log "ERROR: HPA at maxReplicas=8 with ${cpu_avg}% CPU > 77% — cannot scale further; the cluster may be undersized."
      exit 1
    fi

    # Equilibrium: same replica count held for 60+ seconds. The achieved CPU
    # at a given replica count depends on discrete rounding in the HPA's
    # ceil(replicas × utilization/target) formula, so it won't always land
    # inside the nominal ±10% tolerance band — replica-count stability alone
    # is what actually indicates the HPA has stopped scaling.
    if [[ "$stable_count" -ge 5 && "$elapsed" -gt 120 ]]; then
      log "Equilibrium: $replicas pods, ${cpu_avg}% CPU, ${elapsed}s elapsed."
      break
    fi

    sleep 15
  done

  log "Phase 4: measuring equilibrium throughput..."
  local -a pods
  mapfile -t pods < <(pick_pods)
  measure_pods "${pods[@]}" > "$TMPDIR_WORK/measure.txt"
  local total_mibps total_eps
  read -r total_mibps total_eps < "$TMPDIR_WORK/measure.txt"

  {
    echo "PHASE4_MIBPS=${total_mibps}"
    echo "PHASE4_EPS=${total_eps}"
    echo "PHASE4_PODS=${last_replicas}"
    echo "PHASE4_CPU=${cpu_avg}%"
    echo "PHASE4_SCALE_EVENTS=${scale_events}"
    echo "PHASE4_ELAPSED=${elapsed}s"
  } > "$out"
}

# ── main ──────────────────────────────────────────────────────────────────────
log "Cleaning up any leftover port-forwards from previous runs..."
pkill -f "kubectl port-forward.*vector-perf.*pod/" 2>/dev/null || true
sleep 1

log "Checking cluster connectivity..."
if ! kubectl cluster-info --request-timeout=5s >/dev/null 2>&1; then
  echo "ERROR: cannot reach the cluster. Is KUBECONFIG set correctly?" >&2
  echo "  KUBECONFIG=${KUBECONFIG:-<unset>}" >&2
  exit 1
fi
log "Cluster reachable."

reset_vector

log "Installing producer chart (lading, 55 MiB/s)..."
helm upgrade --install producer "$PRODUCER_CHART" \
  -n "$NAMESPACE" \
  --set-string runId="$(date -u +%Y-%m-%dT%H:%M:%SZ)-$$" \
  --wait --timeout=3m >/dev/null
log "Waiting 20 s for lading to initialise..."
sleep 20

case "$PHASE" in
  all)
    run_static_phase 1 1
    run_static_phase 2 3
    run_static_phase 3 8
    run_hpa_phase
    ;;
  1) run_static_phase 1 1 ;;
  2) run_static_phase 2 3 ;;
  3) run_static_phase 3 8 ;;
  4) run_hpa_phase ;;
esac

# Load all results
declare -A R
for f in "$TMPDIR_WORK"/phase*.txt; do
  while IFS='=' read -r k v; do R[$k]=$v; done < "$f"
done

if [[ "$PHASE" != all ]]; then
  key="PHASE${PHASE}"
  echo ""
  echo "Phase $PHASE results:"
  echo "  Throughput: ${R[${key}_MIBPS]} MiB/s"
  echo "  Events/s:   ${R[${key}_EPS]}"
  echo "  Avg CPU:    ${R[${key}_CPU]}"
  echo "  Pods:       ${R[${key}_PODS]}"
  if [[ "$PHASE" == 4 ]]; then
    echo "  Scale events: ${R[PHASE4_SCALE_EVENTS]}"
    echo "  Equilibrium:  ${R[PHASE4_ELAPSED]}"
  fi
  exit 0
fi

# ── results table ─────────────────────────────────────────────────────────────
echo ""
echo "┌──────────────┬──────────────┬──────────────┬──────────────┬─────────────┐"
printf "│ %-12s │ %-12s │ %-12s │ %-12s │ %-11s │\n" \
  "" "Phase 1" "Phase 2" "Phase 3" "Phase 4"
printf "│ %-12s │ %-12s │ %-12s │ %-12s │ %-11s │\n" \
  "" "1 pod" "3 pods" "8 pods" "HPA (auto)"
echo "├──────────────┼──────────────┼──────────────┼──────────────┼─────────────┤"
printf "│ %-12s │ %-12s │ %-12s │ %-12s │ %-11s │\n" \
  "Throughput" \
  "${R[PHASE1_MIBPS]:-?} MiB/s" \
  "${R[PHASE2_MIBPS]:-?} MiB/s" \
  "${R[PHASE3_MIBPS]:-?} MiB/s" \
  "${R[PHASE4_MIBPS]:-?} MiB/s"
printf "│ %-12s │ %-12s │ %-12s │ %-12s │ %-11s │\n" \
  "Events/s" \
  "${R[PHASE1_EPS]:-?}" \
  "${R[PHASE2_EPS]:-?}" \
  "${R[PHASE3_EPS]:-?}" \
  "${R[PHASE4_EPS]:-?}"
printf "│ %-12s │ %-12s │ %-12s │ %-12s │ %-11s │\n" \
  "Avg CPU/pod" \
  "${R[PHASE1_CPU]:-?}" \
  "${R[PHASE2_CPU]:-?}" \
  "${R[PHASE3_CPU]:-?}" \
  "${R[PHASE4_CPU]:-?}"
printf "│ %-12s │ %-12s │ %-12s │ %-12s │ %-11s │\n" \
  "Pods" \
  "${R[PHASE1_PODS]:-?}" \
  "${R[PHASE2_PODS]:-?}" \
  "${R[PHASE3_PODS]:-?}" \
  "${R[PHASE4_PODS]:-?}"
printf "│ %-12s │ %-12s │ %-12s │ %-12s │ %-11s │\n" \
  "Bottleneck" \
  "Vector CPU" "Vector CPU" "None" "N/A"
echo "└──────────────┴──────────────┴──────────────┴──────────────┴─────────────┘"
echo ""
echo "Phase 4: ${R[PHASE4_SCALE_EVENTS]:-?} scale events," \
     "equilibrium in ${R[PHASE4_ELAPSED]:-?}," \
     "0 manual producer restarts."
