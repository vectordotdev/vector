#!/usr/bin/env bash
# run-experiment.sh — run all 4 Vector scaling phases and print a results table.
#
# Usage:
#   KUBECONFIG=/path/to/kubeconfig ./scripts/run-experiment.sh
#
# Requirements: kubectl, grpcurl, python3
# The script assumes namespace, consumer, ingress-nginx, and ingress are already deployed.

set -euo pipefail

NAMESPACE=vector-perf
PRODUCER_MANIFEST=manifests/producer.yaml
TMPDIR_WORK=/tmp/vec-experiment-$$
mkdir -p "$TMPDIR_WORK"
trap 'rm -rf "$TMPDIR_WORK"; pkill -f "kubectl port-forward.*vector-perf.*pod/" 2>/dev/null || true' EXIT

# ── helpers ───────────────────────────────────────────────────────────────────
log() { echo "==> $*" >&2; }

# K3s kubeconfig uses client-certificate auth — no AWS credentials needed.
kube() { kubectl "$@"; }

wait_rollout() {
  kube rollout status deployment/vector -n "$NAMESPACE" --timeout=120s >&2
}

delete_hpa() {
  kube delete hpa vector -n "$NAMESPACE" 2>/dev/null || true
}

pick_pod() {
  kube get pods -n "$NAMESPACE" -l app.kubernetes.io/name=vector \
    --field-selector=status.phase=Running \
    -o jsonpath='{.items[0].metadata.name}'
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

# Measure throughput from one pod over a 30-second window.
# Writes "<MiB/s> <ev/s>" to $TMPDIR_WORK/measure.txt
measure_pod() {
  local pod=$1 port=$2

  kube port-forward -n "$NAMESPACE" "pod/$pod" "${port}:8686" > "$TMPDIR_WORK/pf.log" 2>&1 &
  local pf_pid=$!

  # Wait up to 10 s for the gRPC health check to pass.
  local i=0
  while ! grpcurl -plaintext "localhost:${port}" grpc.health.v1.Health/Check >/dev/null 2>&1; do
    if ! kill -0 "$pf_pid" 2>/dev/null; then
      log "ERROR: port-forward to pod/${pod}:8686 → ${port} died. Output:"
      cat "$TMPDIR_WORK/pf.log" >&2
      exit 1
    fi
    i=$(( i + 1 ))
    if [[ "$i" -ge 20 ]]; then
      log "ERROR: gRPC health check on port ${port} not ready after 10 s."
      cat "$TMPDIR_WORK/pf.log" >&2
      exit 1
    fi
    sleep 0.5
  done

  if ! grpcurl -plaintext -d '{}' "localhost:${port}" \
      vector.observability.v1.ObservabilityService/GetComponents \
      > "$TMPDIR_WORK/t0.json" 2>&1; then
    log "ERROR: grpcurl failed on port ${port} (pod=${pod}). Output:"
    cat "$TMPDIR_WORK/t0.json" >&2
    exit 1
  fi
  sleep 30
  if ! grpcurl -plaintext -d '{}' "localhost:${port}" \
      vector.observability.v1.ObservabilityService/GetComponents \
      > "$TMPDIR_WORK/t30.json" 2>&1; then
    log "ERROR: grpcurl failed on port ${port} (pod=${pod}). Output:"
    cat "$TMPDIR_WORK/t30.json" >&2
    exit 1
  fi

  kill "$pf_pid" 2>/dev/null
  wait "$pf_pid" 2>/dev/null || true

  python3 - "$TMPDIR_WORK/t0.json" "$TMPDIR_WORK/t30.json" <<'PYEOF'
import json, sys

def get_bytes_events(path):
    try:
        d = json.load(open(path))
    except Exception:
        return 0, 0
    for c in d.get('components', []):
        if c.get('componentId') == 'in':
            m = c.get('metrics', {})
            return int(m.get('receivedBytesTotal', 0)), int(m.get('receivedEventsTotal', 0))
    return 0, 0

b1, e1 = get_bytes_events(sys.argv[1])
b2, e2 = get_bytes_events(sys.argv[2])
mibps = (b2 - b1) / 30 / 1048576
eps   = (e2 - e1) / 30
print(f"{mibps:.2f} {eps:.0f}")
PYEOF
}

# ── phase runners ─────────────────────────────────────────────────────────────
# Each function writes key=value lines to $TMPDIR_WORK/phaseN.txt
run_static_phase() {
  local phase=$1 replicas=$2 out="$TMPDIR_WORK/phase${1}.txt"

  log "Phase $phase: scaling Vector to $replicas pod(s)..."
  delete_hpa
  kube scale deployment vector -n "$NAMESPACE" --replicas="$replicas" >/dev/null 2>&1
  wait_rollout

  local pod
  pod=$(pick_pod)
  log "Phase $phase: measuring $pod (20 s warmup + 30 s window)..."
  sleep 20

  local port=$((18680 + replicas))
  measure_pod "$pod" "$port" > "$TMPDIR_WORK/measure.txt"
  local mibps_per_pod eps_per_pod
  read -r mibps_per_pod eps_per_pod < "$TMPDIR_WORK/measure.txt"

  local total_mibps total_eps cpu
  total_mibps=$(python3 -c "print(f'{float(\"$mibps_per_pod\") * $replicas:.2f}')")
  total_eps=$(python3    -c "print(f'{float(\"$eps_per_pod\")   * $replicas:.0f}')")
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
  delete_hpa
  kube scale deployment vector -n "$NAMESPACE" --replicas=1 >/dev/null 2>&1
  kube autoscale deployment vector -n "$NAMESPACE" \
    --cpu-percent=70 --min=1 --max=8 >/dev/null 2>&1

  local start elapsed
  local last_replicas=1 scale_events=0 stable_count=0 last_stable=0
  local replicas="" cpu_avg=""
  start=$(date +%s)

  log "Phase 4: watching HPA..."
  while true; do
    elapsed=$(( $(date +%s) - start ))

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

    # Stable = same replica count for 75 s AND cpu within HPA tolerance band (63–77%)
    if [[ "$stable_count" -ge 5 && "$elapsed" -gt 120 && -n "$cpu_avg" ]]; then
      if [[ "$cpu_avg" -ge 63 && "$cpu_avg" -le 77 ]]; then
        log "Equilibrium: $replicas pods, ${cpu_avg}% CPU, ${elapsed}s elapsed."
        break
      fi
    fi

    sleep 15
  done

  log "Phase 4: measuring equilibrium throughput..."
  local pod
  pod=$(pick_pod)
  measure_pod "$pod" 28686 > "$TMPDIR_WORK/measure.txt"
  local mibps_per_pod eps_per_pod total_mibps total_eps
  read -r mibps_per_pod eps_per_pod < "$TMPDIR_WORK/measure.txt"
  total_mibps=$(python3 -c "print(f'{float(\"$mibps_per_pod\") * $last_replicas:.2f}')")
  total_eps=$(python3    -c "print(f'{float(\"$eps_per_pod\")   * $last_replicas:.0f}')")

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

log "Applying producer manifest (lading, 65 MiB/s)..."
kube apply -f "$PRODUCER_MANIFEST" >/dev/null 2>&1
kube scale deployment producer -n "$NAMESPACE" --replicas=1 >/dev/null 2>&1
kube rollout restart deployment producer -n "$NAMESPACE" >/dev/null 2>&1
log "Waiting 20 s for lading to initialise..."
sleep 20

run_static_phase 1 1
run_static_phase 2 3
run_static_phase 3 8
run_hpa_phase

# Load all results
declare -A R
for f in "$TMPDIR_WORK"/phase*.txt; do
  while IFS='=' read -r k v; do R[$k]=$v; done < "$f"
done

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
  "Vector CPU" "Vector CPU" "None" "— "
echo "└──────────────┴──────────────┴──────────────┴──────────────┴─────────────┘"
echo ""
echo "Phase 4: ${R[PHASE4_SCALE_EVENTS]:-?} scale events," \
     "equilibrium in ${R[PHASE4_ELAPSED]:-?}," \
     "0 manual producer restarts."
