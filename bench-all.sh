#!/usr/bin/env bash
set -uo pipefail

BENCH="/Users/luke.steensen/code/vector/bench.sh"
RESULTS_DIR="/Users/luke.steensen/code/vector/bench-results/full"
mkdir -p "$RESULTS_DIR"

VARIANTS=(
  flat
  flat-co-ks
  flat-co-ksc
  btree-co-ks
  btree-co-ksc
)

CASES=(
  datadog_agent_remap_blackhole
  datadog_agent_remap_blackhole_acks
  datadog_agent_remap_datadog_logs
  datadog_agent_remap_datadog_logs_acks
  file_100_to_blackhole
  file_to_blackhole
  fluent_elasticsearch
  http_elasticsearch
  http_text_to_http_json
  http_to_http_acks
  http_to_http_disk_buffer
  http_to_http_json
  http_to_http_noack
  http_to_s3
  otlp_grpc_to_blackhole
  otlp_http_to_blackhole
  socket_to_socket_blackhole
  splunk_hec_indexer_ack_blackhole
  splunk_hec_route_s3
  splunk_hec_to_splunk_hec_logs_acks
  splunk_hec_to_splunk_hec_logs_noack
  statsd_to_datadog_metrics
  syslog_humio_logs
  syslog_log2metric_humio_metrics
  syslog_log2metric_splunk_hec_metrics
  syslog_log2metric_tag_cardinality_limit_blackhole
  syslog_loki
  syslog_regex_logs2metric_ddmetrics
  syslog_splunk_hec_logs
)

total=$(( ${#VARIANTS[@]} * ${#CASES[@]} ))
n=0

for variant in "${VARIANTS[@]}"; do
  for case_name in "${CASES[@]}"; do
    n=$((n + 1))
    outfile="${RESULTS_DIR}/${case_name}--${variant}.log"

    # Skip if already completed
    if [[ -f "$outfile" ]] && grep -q "Δ mean" "$outfile" 2>/dev/null; then
      echo "[$n/$total] SKIP (already done): ${case_name} -- btree vs ${variant}"
      continue
    fi

    # Skip known-broken cases (need FUSE)
    if [[ "$case_name" == "file_100_to_blackhole" || "$case_name" == "file_to_blackhole" ]]; then
      echo "[$n/$total] SKIP (needs FUSE): ${case_name} -- btree vs ${variant}"
      continue
    fi

    echo "[$n/$total] Running: ${case_name} -- btree vs ${variant}"
    containers=()
    while IFS= read -r container; do
      containers+=("$container")
    done < <(docker ps -aq)
    if (( ${#containers[@]} > 0 )); then
      docker rm -f "${containers[@]}" 2>/dev/null || true
    fi
    if ! "${BENCH}" run "${case_name}" btree "${variant}" 2>&1 | tee "${outfile}"; then
      echo "[$n/$total] FAILED: ${case_name} -- btree vs ${variant}"
    fi
    echo ""
  done
done

echo "=== ALL DONE ==="
