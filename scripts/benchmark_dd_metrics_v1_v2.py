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
    "env:perf,bench:dd_metrics,service:vector,region:us-east-1,team:observability,"
    "version:1.2.3-build.456,datacenter:us-east-1a,availability_zone:us-east-1a,"
    "cluster:prod-metrics-01,namespace:observability,deployment:stable,"
    "kube_node:ip-10-0-1-42.ec2.internal,kube_container_name:vector-agent,"
    "app:datadog-agent,tier:backend,component:sink,pipeline:metrics,"
    "account_id:123456789012,org:engineering,squad:platform,product:observability,"
    "runtime:rust,os:linux,arch:amd64,kernel:5.15.0-1034-aws,"
    "instance_type:m5.2xlarge,image_id:ami-0abcdef1234567890,vpc_id:vpc-0a1b2c3d"
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
    received_eps: float
    received_total: float
    errors_total_delta: float
    discarded_total_delta: float
    avg_cpu_percent: float
    avg_rss_mb: float
    peak_rss_mb: float
    http_requests_sent_eps: float
    delivery_ratio: float
    pipeline_accept_ratio: float
    loss_rate: float


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


def write_vector_config(path: Path, args: argparse.Namespace, max_bytes: Optional[int]) -> None:
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
) -> Tuple[float, str]:
    exact = 0.0
    fuzzy = 0.0
    dd_sink = 0.0
    plain = metric_base_name
    prefixed = f"vector_{metric_base_name}"
    target = component_id.lower()
    for name, labels, value in rows:
        if name != plain and name != prefixed:
            continue
        label_component_id = labels.get("component_id", "")
        # Primary match: exact component_id.
        if label_component_id == component_id:
            exact += value
            continue
        # Fuzzy match: component_id includes sink id (e.g. namespaced IDs).
        if target and target in label_component_id.lower():
            fuzzy += value
            continue
        # Fallback match: datadog_metrics sink counters by component metadata.
        if (
            labels.get("component_kind") == "sink"
            and labels.get("component_type") == "datadog_metrics"
        ):
            dd_sink += value
            continue
    if exact > 0:
        return exact, "component_id_exact"
    if fuzzy > 0:
        return fuzzy, "component_id_fuzzy"
    if dd_sink > 0:
        return dd_sink, "datadog_sink_tags"
    return 0.0, "no_match"


def fetch_metrics_snapshot(metrics_url: str, sink_id: str) -> Dict[str, float]:
    with urllib.request.urlopen(metrics_url, timeout=5) as resp:
        text = resp.read().decode("utf-8", errors="replace")
    rows = parse_prometheus_metrics(text)
    sent, sent_match = extract_component_sum(rows, "component_sent_events_total", sink_id)
    received, received_match = extract_component_sum(
        rows, "component_received_events_total", sink_id
    )
    errors, errors_match = extract_component_sum(rows, "component_errors_total", sink_id)
    discarded, discarded_match = extract_component_sum(
        rows, "component_discarded_events_total", sink_id
    )
    http_requests, _ = extract_component_sum(
        rows, "http_client_requests_sent_total", sink_id
    )
    return {
        "sent": sent,
        "received": received,
        "errors": errors,
        "discarded": discarded,
        "http_requests": http_requests,
        "sent_match": sent_match,
        "received_match": received_match,
        "errors_match": errors_match,
        "discarded_match": discarded_match,
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

    if mode == "v2":
        env["VECTOR_TEMP_USE_DD_METRICS_SERIES_V2_API"] = "1"
    else:
        env.pop("VECTOR_TEMP_USE_DD_METRICS_SERIES_V2_API", None)

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
            if (
                after.get("sent_match") != "component_id_exact"
                or after.get("received_match") != "component_id_exact"
            ):
                print(
                    f"[{run_id}] metric match strategy: "
                    f"sent={after.get('sent_match')} "
                    f"received={after.get('received_match')} "
                    f"errors={after.get('errors_match')} "
                    f"discarded={after.get('discarded_match')}",
                    flush=True,
                )

            duration = float(args.measure_seconds)
            sent_delta = max(0.0, after["sent"] - before["sent"])
            received_delta = max(0.0, after["received"] - before["received"])
            errors_delta = max(0.0, after["errors"] - before["errors"])
            discarded_delta = max(0.0, after["discarded"] - before["discarded"])

            return RunResult(
                mode=mode,
                repeat=repeat,
                generator_actual_rate=gen_rate,
                generated_total=gen_rate * duration,
                sent_eps=sent_delta / duration,
                sent_total=sent_delta,
                received_eps=received_delta / duration,
                received_total=received_delta,
                errors_total_delta=errors_delta,
                discarded_total_delta=discarded_delta,
                avg_cpu_percent=statistics.mean(cpu_samples) if cpu_samples else 0.0,
                avg_rss_mb=statistics.mean(rss_samples) if rss_samples else 0.0,
                peak_rss_mb=max(rss_samples) if rss_samples else 0.0,
                http_requests_sent_eps=(after["http_requests"] - before["http_requests"]) / duration,
                delivery_ratio=(sent_delta / duration) / gen_rate if gen_rate > 0 else 0.0,
                pipeline_accept_ratio=(
                    (received_delta / duration) / gen_rate if gen_rate > 0 else 0.0
                ),
                loss_rate=(discarded_delta / (gen_rate * duration))
                if gen_rate > 0 and duration > 0
                else 0.0,
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
        "received_eps": median([r.received_eps for r in results]),
        "received_total": median([r.received_total for r in results]),
        "errors_total_delta": median([r.errors_total_delta for r in results]),
        "discarded_total_delta": median([r.discarded_total_delta for r in results]),
        "avg_cpu_percent": median([r.avg_cpu_percent for r in results]),
        "avg_rss_mb": median([r.avg_rss_mb for r in results]),
        "peak_rss_mb": median([r.peak_rss_mb for r in results]),
        "http_requests_sent_eps": median([r.http_requests_sent_eps for r in results]),
        "delivery_ratio": median([r.delivery_ratio for r in results]),
        "pipeline_accept_ratio": median([r.pipeline_accept_ratio for r in results]),
        "loss_rate": median([r.loss_rate for r in results]),
    }


def print_results_table(results: List[RunResult]) -> None:
    print("\nPer-run results")
    print(
        "mode repeat gen_rate generated sent_eps sent_total recv_eps recv_total delivery accept loss errors discarded avg_cpu avg_rss_mb peak_rss_mb"
    )
    for r in results:
        print(
            f"{r.mode:>3} {r.repeat:>6} {r.generator_actual_rate:>8.1f} "
            f"{r.generated_total:>9.0f} {r.sent_eps:>8.1f} {r.sent_total:>10.0f} "
            f"{r.received_eps:>8.1f} {r.received_total:>10.0f} "
            f"{r.delivery_ratio:>8.3f} {r.pipeline_accept_ratio:>7.3f} {r.loss_rate:>6.4f} "
            f"{r.errors_total_delta:>6.1f} {r.discarded_total_delta:>9.1f} "
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
        "received_eps",
        "received_total",
        "errors_total_delta",
        "discarded_total_delta",
        "avg_cpu_percent",
        "avg_rss_mb",
        "peak_rss_mb",
        "http_requests_sent_eps",
        "delivery_ratio",
        "pipeline_accept_ratio",
        "loss_rate",
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
        write_vector_config(config_paths["v1"], args, args.batch_max_bytes_v1)
        write_vector_config(config_paths["v2"], args, args.batch_max_bytes_v2)

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
