#!/usr/bin/env python3
"""Benchmark Datadog metrics sink series v1 vs v2 with real intake.

This script:
1) starts Vector with a temporary config (statsd -> datadog_metrics),
2) drives load via scripts/generate_statsd_load.py,
3) scrapes internal metrics from prometheus_exporter,
4) samples Vector CPU/RSS, and
5) prints per-run and median comparisons for v1/v2.
"""

import argparse
import os
import re
import signal
import statistics
import subprocess
import sys
import tempfile
import textwrap
import time
import urllib.error
import urllib.request
from dataclasses import dataclass
from pathlib import Path
from typing import Dict, Iterable, List, Optional, Tuple


METRIC_LINE_RE = re.compile(
    r"^([a-zA-Z_:][a-zA-Z0-9_:]*)(\{[^}]*\})?\s+"
    r"([-+]?[0-9]*\.?[0-9]+(?:[eE][-+]?[0-9]+)?)(?:\s+[-+]?[0-9]+)?$"
)
LABEL_RE = re.compile(r'([a-zA-Z_][a-zA-Z0-9_]*)="((?:\\.|[^"\\])*)"')

DEFAULT_LOAD_SCRIPT = "scripts/generate_statsd_load.py"
DEFAULT_DD_SITE = "datadoghq.com"
DEFAULT_SAMPLE_INTERVAL_SECONDS = 2.0
DEFAULT_STATSD_HOST = "127.0.0.1"
DEFAULT_STATSD_PORT = 8125
DEFAULT_METRICS_PORT = 9598
DEFAULT_METRIC = "bench.datadog.metrics.sink.performance.counter"
DEFAULT_TAGS = (
    # Cloud / infrastructure — 22 tags, long resource IDs
    "env:production-us-east-1,availability_zone:us-east-1a,"
    "cloud_provider:amazon-web-services,cloud_platform:aws_ec2_on_demand,"
    "cloud_availability_zone:us-east-1a,"
    "cloud_account_id:123456789012,cloud_resource_id:i-0a1b2c3d4e5f67890,"
    "instance_type:m5.2xlarge,image_id:ami-0abcdef1234567890,"
    "vpc_id:vpc-0a1b2c3d4e5f67890,subnet_id:subnet-0a1b2c3d4e5f67890,"
    "security_group_id:sg-0a1b2c3d4e5f67890,aws_private_ip:10.0.1.42,"
    "aws_autoscaling_group:vector-agents-prod-us-east-1a,"
    "aws_launch_template_id:lt-0a1b2c3d4e5f67890,"
    "aws_iam_instance_profile:vector-agent-prod-us-east-1,"
    "aws_iam_role:arn-aws-iam-123456789012-role-vector-agent-prod,"
    "aws_spot_instance_request_id:sir-0a1b2c3d4e5f67890,"
    "aws_reservation_id:r-0a1b2c3d4e5f678901,"
    # Kubernetes workload — pod/node UIDs add length
    "cluster:prod-metrics-01,kube_node:ip-10-0-1-42.ec2.internal,"
    "kube_node_uid:b2c3d4e5-f6a7-8901-bcde-f12345678901,"
    "kube_namespace:platform-observability,kube_pod_name:vector-agent-7d9f8b,"
    "kube_pod_uid:a1b2c3d4-e5f6-7890-abcd-ef1234567890,"
    "kube_container_name:vector-agent,"
    "kube_container_id:containerd-sha256-abc123def456789012345678901234567890,"
    "kube_deployment:vector-agent-stable,kube_daemonset:vector-agent,"
    "kube_replicaset:vector-agent-7d9f8b,kube_service:vector-agent-headless,"
    "kube_service_account:vector-agent-service-account,"
    "kube_priority_class:high-priority-nonpreempting,"
    "kube_scheduler:default-scheduler,"
    "kube_label_app:vector-agent,kube_label_chart:vector-agent-0.54.0,"
    "kube_label_release:prod-us-east-1,kube_label_heritage:Helm,"
    "kube_label_managed_by:flux-helm-operator,"
    "kube_label_gitops_repo:platform-config-repo,"
    "kube_label_gitops_path:clusters/prod/us-east-1/vector,"
    "kube_label_app_kubernetes_io_name:vector-agent,"
    "kube_label_app_kubernetes_io_version:0.54.0,"
    "kube_label_topology_kubernetes_io_region:us-east-1,"
    "kube_label_topology_kubernetes_io_zone:us-east-1a,"
    "kube_annotation_prometheus_io_scrape:true,"
    "kube_annotation_prometheus_io_port:9598,"
    "kube_annotation_datadog_ad_check_names:vector-agent,"
    # Application / service — 20 tags
    "service:vector-metrics-pipeline,version:0.54.0-build.1234,"
    "team:platform-observability,squad:platform-engineering,"
    "org:engineering-infrastructure,business_unit:infrastructure-platform,"
    "cost_center:platform-observability-prod,product:observability-platform,"
    "component:datadog-metrics-sink-v2,pipeline:metrics-ingestion-pipeline,"
    "tier:backend-data-processing,runtime:rust-1.88-stable,"
    "os:ubuntu-22.04-lts-linux,"
    "app:datadog-vector-agent,deployment:stable-production-rollout,"
    "stage:production-stable,data_classification:internal,"
    # Telemetry / observability — 10 tags
    "telemetry_sdk_name:opentelemetry-rust-sdk,telemetry_sdk_language:rust,"
    "telemetry_sdk_version:0.24.0,telemetry_distro_name:datadog-vector-0.54,"
    "otel_scope_name:datadog_metrics_sink_v2,otel_scope_version:0.54.0,"
    "network_destination_domain:agent.datadoghq.com,"
    "network_protocol:https-1.1,network_peer_address:13.248.148.240,"
    # Business / compliance — 8 tags
    "account_id:123456789012,project:metrics-ingestion-platform,"
    "workload:vector-agent-daemonset,feature_flag_new_pipeline:enabled,"
    "feature_flag_compression_v2:disabled,feature_flag_otlp_ingest:enabled,"
    "compliance_framework:soc2-type2-certified,"
    "environment_tier:production-stable-tier1"
)
DEFAULT_CARDINALITY_TAG_KEY = "host"
DEFAULT_CARDINALITY_TAG_COUNT = 1000
DEFAULT_HIGH_CARDINALITY_TAGS_COUNT = 15
DEFAULT_HIGH_CARDINALITY_VALUE_BYTES = 8
DEFAULT_BATCH_TIMEOUT_SECONDS = 2.0
DEFAULT_SINK_ID = "dd_metrics"
DEFAULT_RUN_ORDER = ("v2", "v1")


@dataclass
class RunResult:
    mode: str
    repeat: int
    generator_actual_rate: float
    generated_total: float
    sent_eps: float
    sent_total: float
    compressed_bytes_sent: float
    compressed_bytes_per_sec: float
    avg_cpu_percent: float
    avg_rss_mb: float
    peak_rss_mb: float
    http_requests_sent_eps: float
    delivery_ratio: float


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Benchmark Datadog metrics sink v1 vs v2."
    )
    parser.add_argument(
        "--vector-bin",
        default="vector",
        help="Path to Vector binary (default: vector in PATH).",
    )
    parser.add_argument(
        "--repeats", type=int, default=3, help="Repeats per mode (default: 3)."
    )
    parser.add_argument(
        "--warmup-seconds",
        type=int,
        default=60,
        help="Warmup seconds before measurement load.",
    )
    parser.add_argument(
        "--measure-seconds",
        type=int,
        default=300,
        help="Measurement duration in seconds.",
    )
    parser.add_argument(
        "--rate", type=int, default=20000, help="Points/sec for load generator."
    )
    parser.add_argument(
        "--metric-count",
        type=int,
        default=100,
        help="Number of metric names to cycle through.",
    )
    parser.add_argument(
        "--batch-max-bytes-v1",
        type=int,
        default=None,
        help="Datadog metrics sink batch max_bytes (bytes) for the v1 series endpoint.",
    )
    parser.add_argument(
        "--batch-max-bytes-v2",
        type=int,
        default=None,
        help="Datadog metrics sink batch max_bytes (bytes) for the v2 series endpoint.",
    )
    parser.add_argument(
        "--vector-log",
        default="info",
        help="VECTOR_LOG level/filter for Vector (e.g. info, debug, vector::sinks::datadog::metrics=debug).",
    )
    parser.add_argument(
        "--vector-log-file",
        default="",
        help="Optional file path to store Vector logs during benchmark.",
    )
    return parser.parse_args()


def require_file(path: str, name: str) -> None:
    if not Path(path).exists():
        raise FileNotFoundError(f"{name} not found at {path}")


def write_vector_config(path: Path, args: argparse.Namespace, max_bytes: Optional[int], mode: str = "v2") -> None:
    max_bytes_line = f"\n              max_bytes: {max_bytes}" if max_bytes else ""
    config = textwrap.dedent(
        f"""
        data_dir: "{path.parent / "vector-data"}"

        sources:
          statsd_in:
            type: "statsd"
            mode: "tcp"
            address: "{DEFAULT_STATSD_HOST}:{DEFAULT_STATSD_PORT}"
          internal_metrics:
            type: "internal_metrics"

        sinks:
          metrics_exporter:
            type: "prometheus_exporter"
            inputs: ["internal_metrics"]
            address: "0.0.0.0:{DEFAULT_METRICS_PORT}"
          {DEFAULT_SINK_ID}:
            type: "datadog_metrics"
            inputs: ["statsd_in"]
            default_api_key: "${{DD_API_KEY}}"
            site: "${{DD_SITE}}"
            series_api_version: "{mode}"
            batch:
              timeout_secs: {DEFAULT_BATCH_TIMEOUT_SECONDS}{max_bytes_line}
        """
    ).strip()
    path.write_text(config + "\n", encoding="utf-8")


def wait_for_metrics(url: str, timeout_seconds: float = 30.0) -> None:
    deadline = time.time() + timeout_seconds
    while time.time() < deadline:
        try:
            with urllib.request.urlopen(url, timeout=2) as resp:
                if resp.status == 200:
                    return
        except urllib.error.URLError:
            time.sleep(0.25)
    raise RuntimeError(f"Timed out waiting for metrics endpoint: {url}")


def parse_labels(labels_blob: str) -> Dict[str, str]:
    if not labels_blob:
        return {}
    content = labels_blob.strip()[1:-1]
    if not content:
        return {}
    labels: Dict[str, str] = {}
    for key, raw_value in LABEL_RE.findall(content):
        labels[key] = bytes(raw_value, "utf-8").decode("unicode_escape")
    return labels


def parse_prometheus_metrics(text: str) -> List[Tuple[str, Dict[str, str], float]]:
    rows: List[Tuple[str, Dict[str, str], float]] = []
    for raw in text.splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        match = METRIC_LINE_RE.match(line)
        if not match:
            continue
        name, labels_blob, value = match.groups()
        labels = parse_labels(labels_blob or "")
        try:
            rows.append((name, labels, float(value)))
        except ValueError:
            continue
    return rows


def extract_component_sum(
    rows: Iterable[Tuple[str, Dict[str, str], float]],
    metric_base_name: str,
    component_id: str,
) -> float:
    plain = metric_base_name
    prefixed = f"vector_{metric_base_name}"
    total = 0.0
    for name, labels, value in rows:
        if (name == plain or name == prefixed) and labels.get("component_id") == component_id:
            total += value
    return total


def fetch_metrics_snapshot(metrics_url: str, sink_id: str) -> Dict[str, float]:
    with urllib.request.urlopen(metrics_url, timeout=5) as resp:
        text = resp.read().decode("utf-8", errors="replace")
    rows = parse_prometheus_metrics(text)
    return {
        "sent": extract_component_sum(rows, "component_sent_events_total", sink_id),
        "http_requests": extract_component_sum(rows, "http_client_requests_sent_total", sink_id),
        "compressed_bytes": extract_component_sum(rows, "component_sent_bytes_total", sink_id),
    }


def sample_process_cpu_rss(pid: int) -> Tuple[float, float]:
    proc = subprocess.run(
        ["ps", "-o", "%cpu=,rss=", "-p", str(pid)],
        check=True,
        text=True,
        capture_output=True,
    )
    line = proc.stdout.strip()
    if not line:
        return 0.0, 0.0
    parts = line.split()
    if len(parts) < 2:
        return 0.0, 0.0
    cpu = float(parts[0])
    rss_kb = float(parts[1])
    return cpu, rss_kb / 1024.0


def run_load_generator(
    args: argparse.Namespace,
    duration_seconds: int,
    run_label: str,
) -> float:
    cmd = [
        sys.executable,
        DEFAULT_LOAD_SCRIPT,
        "--host",
        DEFAULT_STATSD_HOST,
        "--port",
        str(DEFAULT_STATSD_PORT),
        "--rate",
        str(args.rate),
        "--duration",
        str(duration_seconds),
        "--metric",
        DEFAULT_METRIC,
        "--metric-count",
        str(args.metric_count),
        "--tags",
        f"{DEFAULT_TAGS},run:{run_label}",
        "--cardinality-tag-key",
        DEFAULT_CARDINALITY_TAG_KEY,
        "--cardinality-tag-count",
        str(DEFAULT_CARDINALITY_TAG_COUNT),
        "--incrementing-tag-start",
        "0",
        "--incrementing-tag-key",
        "seq",
        "--high-cardinality-tags-count",
        str(DEFAULT_HIGH_CARDINALITY_TAGS_COUNT),
        "--high-cardinality-deterministic",
        "--high-cardinality-value-bytes",
        str(DEFAULT_HIGH_CARDINALITY_VALUE_BYTES),
    ]

    proc = subprocess.run(cmd, check=True, text=True, capture_output=True)
    output = (proc.stdout + "\n" + proc.stderr).strip()
    match = re.search(r"actual rate=([0-9.]+)", output)
    if not match:
        raise RuntimeError(
            "Could not parse generator actual rate from output:\n"
            f"{output}"
        )
    return float(match.group(1))


def start_load_generator(
    args: argparse.Namespace,
    duration_seconds: int,
    run_label: str,
) -> subprocess.Popen:
    cmd = [
        sys.executable,
        DEFAULT_LOAD_SCRIPT,
        "--host",
        DEFAULT_STATSD_HOST,
        "--port",
        str(DEFAULT_STATSD_PORT),
        "--rate",
        str(args.rate),
        "--duration",
        str(duration_seconds),
        "--metric",
        DEFAULT_METRIC,
        "--metric-count",
        str(args.metric_count),
        "--tags",
        f"{DEFAULT_TAGS},run:{run_label}",
        "--cardinality-tag-key",
        DEFAULT_CARDINALITY_TAG_KEY,
        "--cardinality-tag-count",
        str(DEFAULT_CARDINALITY_TAG_COUNT),
        "--incrementing-tag-start",
        "0",
        "--incrementing-tag-key",
        "seq",
        "--high-cardinality-tags-count",
        str(DEFAULT_HIGH_CARDINALITY_TAGS_COUNT),
        "--high-cardinality-deterministic",
        "--high-cardinality-value-bytes",
        str(DEFAULT_HIGH_CARDINALITY_VALUE_BYTES),
    ]
    return subprocess.Popen(cmd, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)


def parse_actual_rate(output: str) -> float:
    match = re.search(r"actual rate=([0-9.]+)", output)
    if not match:
        raise RuntimeError(
            "Could not parse generator actual rate from output:\n"
            f"{output}"
        )
    return float(match.group(1))


def stop_process_gracefully(proc: subprocess.Popen) -> None:
    if proc.poll() is not None:
        return
    proc.send_signal(signal.SIGINT)
    try:
        proc.wait(timeout=15)
        return
    except subprocess.TimeoutExpired:
        pass
    proc.terminate()
    try:
        proc.wait(timeout=10)
        return
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait(timeout=5)


def run_single_benchmark(
    args: argparse.Namespace,
    config_path: Path,
    repeat: int,
    mode: str,
) -> RunResult:
    dd_api_key = os.getenv("DD_API_KEY", "")
    dd_site = os.getenv("DD_SITE", DEFAULT_DD_SITE)
    env = os.environ.copy()
    env["DD_API_KEY"] = dd_api_key
    env["DD_SITE"] = dd_site
    env["VECTOR_LOG"] = args.vector_log

    vector_cmd = [args.vector_bin, "--config", str(config_path)]
    vector_log_fp = None
    try:
        if args.vector_log_file:
            vector_log_fp = open(args.vector_log_file, "a", encoding="utf-8")
            vector_proc = subprocess.Popen(
                vector_cmd,
                env=env,
                stdout=vector_log_fp,
                stderr=vector_log_fp,
                text=True,
            )
        else:
            # Inherit stdio so debug logs are visible and never block on pipe buffers.
            vector_proc = subprocess.Popen(
                vector_cmd,
                env=env,
                stdout=None,
                stderr=None,
                text=True,
            )

        metrics_url = f"http://127.0.0.1:{DEFAULT_METRICS_PORT}/metrics"
        try:
            wait_for_metrics(metrics_url, timeout_seconds=40)
            run_id = f"{mode}-r{repeat}"
            print(f"[{run_id}] warmup {args.warmup_seconds}s ...", flush=True)
            run_load_generator(args, args.warmup_seconds, run_label=f"{mode}-warmup")

            before = fetch_metrics_snapshot(metrics_url, DEFAULT_SINK_ID)
            cpu_samples: List[float] = []
            rss_samples: List[float] = []
            print(f"[{run_id}] measure {args.measure_seconds}s ...", flush=True)
            load_proc = start_load_generator(args, args.measure_seconds, run_label=run_id)
            while load_proc.poll() is None:
                cpu, rss_mb = sample_process_cpu_rss(vector_proc.pid)
                cpu_samples.append(cpu)
                rss_samples.append(rss_mb)
                time.sleep(DEFAULT_SAMPLE_INTERVAL_SECONDS)
            stdout, stderr = load_proc.communicate(timeout=5)
            if load_proc.returncode != 0:
                raise RuntimeError(
                    "Load generator failed with non-zero exit status:\n"
                    f"{stdout}\n{stderr}"
                )
            gen_rate = parse_actual_rate((stdout + "\n" + stderr).strip())

            after = fetch_metrics_snapshot(metrics_url, DEFAULT_SINK_ID)

            duration = float(args.measure_seconds)
            sent_delta = after["sent"] - before["sent"]
            compressed_bytes_delta = after["compressed_bytes"] - before["compressed_bytes"]

            return RunResult(
                mode=mode,
                repeat=repeat,
                generator_actual_rate=gen_rate,
                generated_total=gen_rate * duration,
                sent_eps=sent_delta / duration,
                sent_total=sent_delta,
                compressed_bytes_sent=compressed_bytes_delta,
                compressed_bytes_per_sec=compressed_bytes_delta / duration,
                avg_cpu_percent=statistics.mean(cpu_samples) if cpu_samples else 0.0,
                avg_rss_mb=statistics.mean(rss_samples) if rss_samples else 0.0,
                peak_rss_mb=max(rss_samples) if rss_samples else 0.0,
                http_requests_sent_eps=(after["http_requests"] - before["http_requests"]) / duration,
                delivery_ratio=sent_delta / (gen_rate * duration) if gen_rate > 0 else 0.0,
            )
        finally:
            stop_process_gracefully(vector_proc)
    finally:
        if vector_log_fp is not None:
            vector_log_fp.close()


def median(values: List[float]) -> float:
    return statistics.median(values) if values else 0.0


def summarize_mode(results: List[RunResult]) -> Dict[str, float]:
    return {
        "generator_actual_rate": median([r.generator_actual_rate for r in results]),
        "generated_total": median([r.generated_total for r in results]),
        "sent_eps": median([r.sent_eps for r in results]),
        "sent_total": median([r.sent_total for r in results]),
        "compressed_bytes_sent": median([r.compressed_bytes_sent for r in results]),
        "compressed_bytes_per_sec": median([r.compressed_bytes_per_sec for r in results]),
        "avg_cpu_percent": median([r.avg_cpu_percent for r in results]),
        "avg_rss_mb": median([r.avg_rss_mb for r in results]),
        "peak_rss_mb": median([r.peak_rss_mb for r in results]),
        "http_requests_sent_eps": median([r.http_requests_sent_eps for r in results]),
        "delivery_ratio": median([r.delivery_ratio for r in results]),
    }


def print_results_table(results: List[RunResult]) -> None:
    print("\nPer-run results")
    print(
        "mode repeat gen_rate generated sent_eps sent_total comp_MB comp_MB/s delivery avg_cpu avg_rss_mb peak_rss_mb"
    )
    for r in results:
        print(
            f"{r.mode:>3} {r.repeat:>6} {r.generator_actual_rate:>8.1f} "
            f"{r.generated_total:>9.0f} {r.sent_eps:>8.1f} {r.sent_total:>10.0f} "
            f"{r.compressed_bytes_sent / 1e6:>7.1f} {r.compressed_bytes_per_sec / 1e6:>8.2f} "
            f"{r.delivery_ratio:>8.3f} "
            f"{r.avg_cpu_percent:>7.1f} {r.avg_rss_mb:>10.1f} {r.peak_rss_mb:>11.1f}"
        )


def print_median_comparison(v1: Dict[str, float], v2: Dict[str, float]) -> None:
    print("\nMedian summary")
    print("metric                    v1         v2      delta(v2-v1)")
    keys = [
        "generator_actual_rate",
        "generated_total",
        "sent_eps",
        "sent_total",
        "compressed_bytes_sent",
        "compressed_bytes_per_sec",
        "avg_cpu_percent",
        "avg_rss_mb",
        "peak_rss_mb",
        "http_requests_sent_eps",
        "delivery_ratio",
    ]
    for key in keys:
        v1v = v1.get(key, 0.0)
        v2v = v2.get(key, 0.0)
        delta = v2v - v1v
        if v1v != 0:
            delta_pct = f"{(delta / v1v) * 100:+.1f}%"
        else:
            delta_pct = "n/a"
        print(f"{key:24} {v1v:9.2f} {v2v:9.2f}  {delta:10.2f} ({delta_pct})")


def main() -> int:
    args = parse_args()
    if not os.getenv("DD_API_KEY"):
        raise ValueError("DD API key is required: set DD_API_KEY.")
    if args.repeats <= 0:
        raise ValueError("--repeats must be > 0")
    if args.warmup_seconds <= 0 or args.measure_seconds <= 0:
        raise ValueError("--warmup-seconds and --measure-seconds must be > 0")
    if args.batch_max_bytes_v1 is not None and args.batch_max_bytes_v1 <= 0:
        raise ValueError("--batch-max-bytes-v1 must be > 0")
    if args.batch_max_bytes_v2 is not None and args.batch_max_bytes_v2 <= 0:
        raise ValueError("--batch-max-bytes-v2 must be > 0")

    require_file(DEFAULT_LOAD_SCRIPT, "load script")

    all_results: List[RunResult] = []
    with tempfile.TemporaryDirectory(prefix="dd-metrics-bench-") as tmpdir:
        tmpdir_path = Path(tmpdir)
        config_paths = {
            "v1": tmpdir_path / "vector-bench-v1.yaml",
            "v2": tmpdir_path / "vector-bench-v2.yaml",
        }
        write_vector_config(config_paths["v1"], args, args.batch_max_bytes_v1, mode="v1")
        write_vector_config(config_paths["v2"], args, args.batch_max_bytes_v2, mode="v2")

        for repeat in range(1, args.repeats + 1):
            for mode in DEFAULT_RUN_ORDER:
                print(f"\n=== repeat {repeat}/{args.repeats} mode={mode} ===", flush=True)
                result = run_single_benchmark(args, config_paths[mode], repeat, mode)
                all_results.append(result)

    print_results_table(all_results)
    v1_results = [r for r in all_results if r.mode == "v1"]
    v2_results = [r for r in all_results if r.mode == "v2"]
    if v1_results and v2_results:
        print_median_comparison(
            summarize_mode(v1_results),
            summarize_mode(v2_results),
        )
    else:
        print("\nSkipped median v1/v2 comparison because one mode has no runs.")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
