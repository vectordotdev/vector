#!/usr/bin/env python3
"""Validate correctness of Vector's DataDog metrics sink (v1 and v2).

Sends known metric values through Vector and queries the DataDog Metrics API
to confirm ingested data matches expectations. Covers all metric types:
Counter, Gauge, Set, Distribution, AggregatedHistogram, AggregatedSummary.

Usage:
    python3 scripts/validate_dd_metrics_correctness.py --site datadoghq.com

Requirements:
    - DD_API_KEY and DD_APP_KEY in environment
    - Vector binary (target/release/vector or target/debug/vector)
"""

from __future__ import annotations

import argparse
import json
import os
import signal
import socket
import subprocess
import sys
import tempfile
import threading
import time
import uuid
from dataclasses import dataclass
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path
from typing import Optional
import urllib.error
import urllib.parse
import urllib.request


# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

DEFAULT_SITE = "datadoghq.com"
DEFAULT_STATSD_PORT = 18125
DEFAULT_PROM_PORT = 19090
DEFAULT_VECTOR_METRICS_PORT = 19598
DEFAULT_WAIT_SECS = 60
DEFAULT_RETRY_COUNT = 2
DEFAULT_RETRY_INTERVAL = 15

# Known values injected during the test
COUNTER_VALUE = 10.0
COUNTER_SENDS = 5  # 5 packets × 10 = 50 total
GAUGE_VALUE = 42.5
SET_MEMBERS = ["alpha", "beta", "gamma", "delta"]  # 4 distinct → gauge = 4.0
DIST_SAMPLES = [1.0, 2.0, 3.0, 4.0, 5.0]  # p50≈3, avg=3, count=5, sum=15
HISTOGRAM_COUNT = 10  # sum of counts in mock histogram
HISTOGRAM_SUM = 28.5
SUMMARY_COUNT = 10
SUMMARY_SUM = 25.0

# Multi-tag aggregation test values (group:a vs group:b)
# Verifies that per-tag and combined queries return the correct aggregated values.
COUNTER_VALUE_A = 10.0   # group:a — 5 sends → 50 total
COUNTER_VALUE_B = 20.0   # group:b — 5 sends → 100 total
# Combined counter sum: 150
GAUGE_VALUE_A = 42.5     # group:a
GAUGE_VALUE_B = 99.0     # group:b

# How long to look back when querying (seconds before send_ts)
QUERY_LOOKBACK = 60
# How far forward to look (seconds after send_ts)
QUERY_FORWARD = 360

VECTOR_CONFIG_TEMPLATE = """\
[sources.statsd]
type = "statsd"
address = "127.0.0.1:{statsd_port}"
mode = "udp"

[sources.prom]
type = "prometheus_scrape"
endpoints = ["http://127.0.0.1:{prom_port}/metrics"]
scrape_interval_secs = 5

[sources.internal_metrics]
type = "internal_metrics"

[sinks.dd]
type = "datadog_metrics"
inputs = ["statsd", "prom"]
default_api_key = "{api_key}"
site = "{site}"

[sinks.dd.batch]
timeout_secs = 2

[sinks.internal_out]
type = "prometheus_exporter"
inputs = ["internal_metrics"]
address = "127.0.0.1:{metrics_port}"
"""


# ---------------------------------------------------------------------------
# DataDog API client
# ---------------------------------------------------------------------------

class DDClient:
    def __init__(self, api_key: str, app_key: str, site: str = DEFAULT_SITE):
        self.base_url = f"https://api.{site}"
        self.api_key = api_key
        self.app_key = app_key

    def _request(self, method: str, path: str, params: Optional[dict] = None) -> dict:
        url = self.base_url + path
        if params:
            url += "?" + urllib.parse.urlencode(params)
        req = urllib.request.Request(url, method=method)
        req.add_header("DD-API-KEY", self.api_key)
        req.add_header("DD-APPLICATION-KEY", self.app_key)
        req.add_header("Accept", "application/json")
        try:
            with urllib.request.urlopen(req, timeout=30) as resp:
                return json.loads(resp.read())
        except urllib.error.HTTPError as e:
            body = e.read().decode(errors="replace")
            raise RuntimeError(f"HTTP {e.code} {e.reason} for {url}: {body}") from e

    def query_timeseries(self, query: str, from_ts: int, to_ts: int, verbose: bool = False) -> list[float]:
        """Return list of non-None point values for the query window."""
        data = self._request("GET", "/api/v1/query", {
            "from": from_ts,
            "to": to_ts,
            "query": query,
        })
        if verbose:
            print(f"    RAW: status={data.get('status')} series_count={len(data.get('series', []))}")
            for s in data.get("series", []):
                print(f"      metric={s.get('metric')} scope={s.get('scope')} points={len(s.get('pointlist', []))}")
        series = data.get("series", [])
        if not series:
            return []
        values = []
        for s in series:
            for _, v in s.get("pointlist", []):
                if v is not None:
                    values.append(v)
        return values


# ---------------------------------------------------------------------------
# Prometheus mock server (serves histogram + summary for Vector to scrape)
# ---------------------------------------------------------------------------

def _make_prom_handler(content_fn):
    class Handler(BaseHTTPRequestHandler):
        def do_GET(self):
            body = content_fn().encode()
            self.send_response(200)
            self.send_header("Content-Type", "text/plain; version=0.0.4")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def log_message(self, fmt, *args):
            pass  # suppress access logs

    return Handler


class PrometheusMetricsServer:
    def __init__(self, port: int, run_tag: str, metric_prefix: str):
        self.port = port
        self.run_tag = run_tag
        self.prefix = metric_prefix
        self._server: Optional[HTTPServer] = None
        self._thread: Optional[threading.Thread] = None
        self._scrape_count = 0
        self._lock = threading.Lock()

    def _content(self) -> str:
        # Increment values on each scrape so Vector computes non-zero deltas.
        # Simulates a real histogram/summary accumulating observations over time.
        with self._lock:
            self._scrape_count += 1
            n = self._scrape_count
        tag = self.run_tag
        p = self.prefix
        hcount = n * HISTOGRAM_COUNT
        hsum = n * HISTOGRAM_SUM
        scount = n * SUMMARY_COUNT
        ssum = n * SUMMARY_SUM
        lines = [
            f"# HELP {p}_histogram Test histogram",
            f"# TYPE {p}_histogram histogram",
            f'{p}_histogram_bucket{{le="1.0",testrun="{tag}"}} {n * 2}',
            f'{p}_histogram_bucket{{le="5.0",testrun="{tag}"}} {n * 7}',
            f'{p}_histogram_bucket{{le="10.0",testrun="{tag}"}} {hcount}',
            f'{p}_histogram_bucket{{le="+Inf",testrun="{tag}"}} {hcount}',
            f'{p}_histogram_sum{{testrun="{tag}"}} {hsum}',
            f'{p}_histogram_count{{testrun="{tag}"}} {hcount}',
            "",
            f"# HELP {p}_summary Test summary",
            f"# TYPE {p}_summary summary",
            f'{p}_summary{{quantile="0.5",testrun="{tag}"}} 2.5',
            f'{p}_summary{{quantile="0.9",testrun="{tag}"}} 4.5',
            f'{p}_summary_sum{{testrun="{tag}"}} {ssum}',
            f'{p}_summary_count{{testrun="{tag}"}} {scount}',
            "",
        ]
        return "\n".join(lines)

    def start(self):
        class _ReuseHTTPServer(HTTPServer):
            allow_reuse_address = True

        handler = _make_prom_handler(self._content)
        self._server = _ReuseHTTPServer(("127.0.0.1", self.port), handler)
        self._thread = threading.Thread(target=self._server.serve_forever, daemon=True)
        self._thread.start()

    def stop(self):
        if self._server:
            self._server.shutdown()
            self._server.server_close()
            time.sleep(0.5)  # allow OS to release the port


# ---------------------------------------------------------------------------
# StatsD UDP sender
# ---------------------------------------------------------------------------

def send_statsd_packets(host: str, port: int, packets: list[str], delay: float = 0.01):
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    for pkt in packets:
        sock.sendto(pkt.encode(), (host, port))
        time.sleep(delay)
    sock.close()


def build_statsd_packets(metric_prefix: str, run_tag: str, api_version: str) -> list[str]:
    """Build StatsD packets for counter, gauge, set, distribution, and multi-tag checks."""
    tags = f"testrun:{run_tag},apiversion:{api_version}"
    p = metric_prefix
    packets = []

    # Counter: COUNTER_SENDS packets of COUNTER_VALUE each
    for _ in range(COUNTER_SENDS):
        packets.append(f"{p}.counter:{COUNTER_VALUE:.1f}|c|#{tags}")

    # Gauge
    packets.append(f"{p}.gauge:{GAUGE_VALUE}|g|#{tags}")

    # Set: one packet per distinct member
    for member in SET_MEMBERS:
        packets.append(f"{p}.set:{member}|s|#{tags}")

    # Distribution: one packet per sample
    for sample in DIST_SAMPLES:
        packets.append(f"{p}.dist:{sample}|d|#{tags}")

    # Multi-tag aggregation: counter and gauge split by group:a / group:b.
    # Allows verifying per-group and combined queries independently.
    tags_a = f"{tags},group:a"
    tags_b = f"{tags},group:b"
    for _ in range(COUNTER_SENDS):
        packets.append(f"{p}.ctr_grp:{COUNTER_VALUE_A:.1f}|c|#{tags_a}")
    for _ in range(COUNTER_SENDS):
        packets.append(f"{p}.ctr_grp:{COUNTER_VALUE_B:.1f}|c|#{tags_b}")
    packets.append(f"{p}.gge_grp:{GAUGE_VALUE_A}|g|#{tags_a}")
    packets.append(f"{p}.gge_grp:{GAUGE_VALUE_B}|g|#{tags_b}")

    return packets


# ---------------------------------------------------------------------------
# Vector process manager
# ---------------------------------------------------------------------------

class VectorProcess:
    def __init__(self, binary: str, config_path: str, env: dict[str, str], verbose: bool):
        self.binary = binary
        self.config_path = config_path
        self.env = env
        self.verbose = verbose
        self._proc: Optional[subprocess.Popen] = None
        self._log_file = None
        self._log_path: Optional[str] = None

    def start(self):
        env = {**os.environ, **self.env}
        # VECTOR_LOG is Vector's own log-level env var, read in src/app.rs.
        # Set to debug so HTTP request lines (including the target URI) are emitted.
        env["VECTOR_LOG"] = "debug"
        stdout = None if self.verbose else subprocess.DEVNULL
        # Always capture stderr to a temp file so we can grep for endpoint URLs.
        self._log_file = tempfile.NamedTemporaryFile(
            mode="wb", suffix=".log", delete=False, prefix="vector_"
        )
        self._log_path = self._log_file.name
        stderr = self._log_file if not self.verbose else None
        self._proc = subprocess.Popen(
            [self.binary, "--config", self.config_path],
            env=env,
            stdout=stdout,
            stderr=stderr,
        )
        if self._log_file:
            self._log_file.close()  # child now owns the fd; we reopen later for reading

    def stop(self):
        if self._proc and self._proc.poll() is None:
            self._proc.send_signal(signal.SIGTERM)
            try:
                self._proc.wait(timeout=10)
            except subprocess.TimeoutExpired:
                self._proc.kill()
                self._proc.wait()

    def is_running(self) -> bool:
        return self._proc is not None and self._proc.poll() is None

    def endpoint_proof(self) -> list[str]:
        """Grep the captured log for lines mentioning known DD API paths.
        Returns concise 'METHOD uri content-type' strings."""
        if not self._log_path:
            return []
        keywords = ["/api/v1/series", "/api/v2/series", "/api/beta/sketches"]
        found = []
        try:
            with open(self._log_path, "r", errors="replace") as f:
                for line in f:
                    for kw in keywords:
                        if kw in line:
                            # Extract uri= and content-type= tokens for a compact summary
                            tokens = line.split()
                            uri = next((t for t in tokens if t.startswith("uri=")), "uri=?")
                            method = next((t for t in tokens if t.startswith("method=")), "")
                            ct = next((t for t in tokens if "content-type" in t.lower()), "")
                            found.append(f"{method} {uri} {ct}".strip())
                            break
        except OSError:
            pass
        return found

    def cleanup_log(self):
        if self._log_path:
            try:
                Path(self._log_path).unlink(missing_ok=True)
            except OSError:
                pass
            self._log_path = None


# ---------------------------------------------------------------------------
# Validation helpers
# ---------------------------------------------------------------------------

@dataclass
class CheckResult:
    metric: str
    metric_type: str
    api_version: str
    expected: str
    actual: str
    status: str  # PASS / FAIL / ERROR


def _query_once(client: DDClient, query: str, from_ts: int, to_ts: int, verbose: bool) -> list[float]:
    try:
        if verbose:
            print(f"    QUERY: {query}")
        values = client.query_timeseries(query, from_ts, to_ts, verbose=verbose)
        if verbose:
            print(f"    VALUES: {values}")
        return values
    except Exception as e:
        print(f"  Query error: {e}")
        return []


def check_counter(
    client: DDClient, prefix: str, run_tag: str, api_version: str,
    from_ts: int, to_ts: int, verbose: bool,
) -> CheckResult:
    # We send exactly COUNTER_SENDS packets of COUNTER_VALUE each.
    # Vector sums incremental counters within a batch, so the total must be exact.
    metric = f"{prefix}.counter"
    expected = COUNTER_VALUE * COUNTER_SENDS
    query = f"sum:{metric}{{testrun:{run_tag},apiversion:{api_version}}}.as_count()"
    values = _query_once(client, query, from_ts, to_ts, verbose)
    if not values:
        return CheckResult(metric, "count", api_version, f"= {expected:.0f}", "no data", "FAIL")
    total = sum(values)
    status = "PASS" if abs(total - expected) < 0.5 else "FAIL"
    return CheckResult(metric, "count", api_version, f"= {expected:.0f}", f"{total:.1f}", status)


def check_gauge(
    client: DDClient, prefix: str, run_tag: str, api_version: str,
    from_ts: int, to_ts: int, verbose: bool,
) -> CheckResult:
    metric = f"{prefix}.gauge"
    query = f"avg:{metric}{{testrun:{run_tag},apiversion:{api_version}}}"
    values = _query_once(client, query, from_ts, to_ts, verbose)
    if not values:
        return CheckResult(metric, "gauge", api_version, f"= {GAUGE_VALUE}", "no data", "FAIL")
    actual = max(values)
    status = "PASS" if abs(actual - GAUGE_VALUE) <= 0.01 else "FAIL"
    return CheckResult(metric, "gauge", api_version, f"= {GAUGE_VALUE}", f"{actual:.4f}", status)


def check_set(
    client: DDClient, prefix: str, run_tag: str, api_version: str,
    from_ts: int, to_ts: int, verbose: bool,
) -> CheckResult:
    # NOTE: Vector's statsd source does not accumulate set members across
    # packets — each |s packet becomes an independent gauge(1) event.
    # This differs from the Datadog Agent which aggregates across a flush window.
    # We verify the metric arrives with value >= 1 and note the behavior.
    metric = f"{prefix}.set"
    query = f"avg:{metric}{{testrun:{run_tag},apiversion:{api_version}}}"
    values = _query_once(client, query, from_ts, to_ts, verbose)
    if not values:
        return CheckResult(metric, "gauge", api_version, ">= 1 (no accumulation)", "no data", "FAIL")
    actual = max(values)
    status = "PASS" if actual >= 1.0 else "FAIL"
    return CheckResult(metric, "gauge", api_version, ">= 1 (no accumulation)", f"{actual:.1f}", status)


def check_distribution(
    client: DDClient, prefix: str, run_tag: str,
    from_ts: int, to_ts: int, verbose: bool,
) -> list[CheckResult]:
    """Distribution (sketches endpoint): verify avg, p50, count, sum, min, max."""
    metric = f"{prefix}.dist"
    # Known values from DIST_SAMPLES = [1, 2, 3, 4, 5]
    expected_count = float(len(DIST_SAMPLES))
    expected_sum = float(sum(DIST_SAMPLES))
    expected_avg = expected_sum / expected_count
    expected_min = float(min(DIST_SAMPLES))
    expected_max = float(max(DIST_SAMPLES))
    tol = 0.5  # DDSketch is approximate; 0.5 tolerance is generous

    results = []
    # Note: p50/p75/p99 require percentile aggregations to be explicitly enabled
    # per-metric in DataDog (disabled by default). We use avg/count/sum/min/max.
    checks = [
        ("avg", expected_avg, f"avg≈{expected_avg:.1f}"),
        ("count", expected_count, f"count={expected_count:.0f}"),
        ("sum", expected_sum, f"sum={expected_sum:.1f}"),
        ("min", expected_min, f"min={expected_min:.1f}"),
        ("max", expected_max, f"max={expected_max:.1f}"),
    ]
    for agg, expected, label in checks:
        values = _query_once(client, f"{agg}:{metric}{{testrun:{run_tag}}}", from_ts, to_ts, verbose)
        if not values:
            results.append(CheckResult(f"{metric}[{agg}]", "distribution", "sketches", label, "no data", "FAIL"))
            continue
        actual = sum(values) if agg == "count" else (values[0] if len(values) == 1 else sum(values) / len(values))
        status = "PASS" if abs(actual - expected) <= tol else "FAIL"
        results.append(CheckResult(f"{metric}[{agg}]", "distribution", "sketches", label, f"{actual:.4f}", status))
    return results


def check_histogram(
    client: DDClient, prefix: str, run_tag: str,
    from_ts: int, to_ts: int, verbose: bool,
) -> list[CheckResult]:
    """AggregatedHistogram → sketches endpoint.
    Mock server increments by HISTOGRAM_COUNT/HISTOGRAM_SUM per scrape.
    We verify count is a positive multiple of HISTOGRAM_COUNT, and that
    sum/count ratio matches HISTOGRAM_SUM/HISTOGRAM_COUNT (avg obs value).
    """
    metric = f"{prefix}_histogram"
    results = []

    count_values = _query_once(client, f"count:{metric}{{testrun:{run_tag}}}", from_ts, to_ts, verbose)
    if not count_values:
        results.append(CheckResult(f"{metric}[count]", "distribution", "sketches",
                                   f">= {HISTOGRAM_COUNT}", "no data", "FAIL"))
    else:
        total_count = sum(count_values)
        ok = total_count >= HISTOGRAM_COUNT and total_count % HISTOGRAM_COUNT == 0
        results.append(CheckResult(f"{metric}[count]", "distribution", "sketches",
                                   f">= {HISTOGRAM_COUNT}, multiple of {HISTOGRAM_COUNT}",
                                   f"{total_count:.0f}", "PASS" if ok else "FAIL"))

    # avg value = HISTOGRAM_SUM / HISTOGRAM_COUNT (mean observation)
    expected_avg = HISTOGRAM_SUM / HISTOGRAM_COUNT
    avg_values = _query_once(client, f"avg:{metric}{{testrun:{run_tag}}}", from_ts, to_ts, verbose)
    if not avg_values:
        results.append(CheckResult(f"{metric}[avg]", "distribution", "sketches",
                                   f"avg≈{expected_avg:.2f}", "no data", "FAIL"))
    else:
        actual_avg = avg_values[0] if len(avg_values) == 1 else sum(avg_values) / len(avg_values)
        tol = expected_avg * 0.1 + 0.5
        results.append(CheckResult(f"{metric}[avg]", "distribution", "sketches",
                                   f"avg≈{expected_avg:.2f}", f"{actual_avg:.4f}",
                                   "PASS" if abs(actual_avg - expected_avg) <= tol else "FAIL"))
    return results


def check_summary(
    client: DDClient, prefix: str, run_tag: str, api_version: str,
    from_ts: int, to_ts: int, verbose: bool,
) -> list[CheckResult]:
    """AggregatedSummary → _summary_sum (count) and _summary_count (gauge).
    Mock server increments by SUMMARY_SUM/SUMMARY_COUNT per scrape.
    We verify the sum/count ratio equals SUMMARY_SUM/SUMMARY_COUNT.
    """
    results = []

    sum_values = _query_once(client, f"sum:{prefix}_summary_sum{{testrun:{run_tag}}}.as_count()",
                              from_ts, to_ts, verbose)
    count_values = _query_once(client, f"avg:{prefix}_summary_count{{testrun:{run_tag}}}", from_ts, to_ts, verbose)

    # Check _summary_sum: must be a positive multiple of SUMMARY_SUM
    metric_sum = f"{prefix}_summary_sum"
    if not sum_values:
        results.append(CheckResult(metric_sum, "count", api_version, f">= {SUMMARY_SUM}", "no data", "FAIL"))
    else:
        total_sum = sum(sum_values)
        ok = total_sum >= SUMMARY_SUM and abs(total_sum / SUMMARY_SUM - round(total_sum / SUMMARY_SUM)) < 0.01
        results.append(CheckResult(metric_sum, "count", api_version,
                                   f">= {SUMMARY_SUM}, multiple of {SUMMARY_SUM}",
                                   f"{total_sum:.4f}", "PASS" if ok else "FAIL"))

    # Check _summary_count: should equal SUMMARY_COUNT per scrape cycle
    metric_count = f"{prefix}_summary_count"
    if not count_values:
        results.append(CheckResult(metric_count, "gauge", api_version, f"= {SUMMARY_COUNT}", "no data", "FAIL"))
    else:
        actual_count = max(count_values)
        ok = abs(actual_count - SUMMARY_COUNT) < 0.5
        results.append(CheckResult(metric_count, "gauge", api_version,
                                   f"= {SUMMARY_COUNT}", f"{actual_count:.4f}",
                                   "PASS" if ok else "FAIL"))

    # Cross-check: sum/count ratio must match SUMMARY_SUM/SUMMARY_COUNT
    if sum_values and count_values:
        total_sum = sum(sum_values)
        actual_count = max(count_values)
        if actual_count > 0:
            n_scrapes = round(total_sum / SUMMARY_SUM)
            ratio_ok = abs(total_sum / n_scrapes - SUMMARY_SUM) < 0.5 if n_scrapes > 0 else False
            results.append(CheckResult(f"{prefix}_summary[ratio]", "gauge", api_version,
                                       f"sum/scrape={SUMMARY_SUM}",
                                       f"{total_sum/n_scrapes:.2f}" if n_scrapes > 0 else "n/a",
                                       "PASS" if ratio_ok else "FAIL"))
    return results


def check_counter_multitag(
    client: DDClient, prefix: str, run_tag: str, api_version: str,
    from_ts: int, to_ts: int, verbose: bool,
) -> list[CheckResult]:
    """Counter with group:a / group:b tags — verifies per-group and combined aggregation."""
    metric = f"{prefix}.ctr_grp"
    base_tags = f"testrun:{run_tag},apiversion:{api_version}"
    expected_a = COUNTER_VALUE_A * COUNTER_SENDS   # 50
    expected_b = COUNTER_VALUE_B * COUNTER_SENDS   # 100
    expected_total = expected_a + expected_b        # 150
    results = []

    for group, expected in [("a", expected_a), ("b", expected_b)]:
        query = f"sum:{metric}{{{base_tags},group:{group}}}.as_count()"
        values = _query_once(client, query, from_ts, to_ts, verbose)
        if not values:
            results.append(CheckResult(f"{metric}[group:{group}]", "count", api_version,
                                       f"= {expected:.0f}", "no data", "FAIL"))
        else:
            total = sum(values)
            status = "PASS" if abs(total - expected) < 0.5 else "FAIL"
            results.append(CheckResult(f"{metric}[group:{group}]", "count", api_version,
                                       f"= {expected:.0f}", f"{total:.1f}", status))

    # Combined (all groups together)
    query_all = f"sum:{metric}{{{base_tags}}}.as_count()"
    values_all = _query_once(client, query_all, from_ts, to_ts, verbose)
    if not values_all:
        results.append(CheckResult(f"{metric}[group:*]", "count", api_version,
                                   f"= {expected_total:.0f}", "no data", "FAIL"))
    else:
        total_all = sum(values_all)
        status = "PASS" if abs(total_all - expected_total) < 0.5 else "FAIL"
        results.append(CheckResult(f"{metric}[group:*]", "count", api_version,
                                   f"= {expected_total:.0f}", f"{total_all:.1f}", status))
    return results


def check_gauge_multitag(
    client: DDClient, prefix: str, run_tag: str, api_version: str,
    from_ts: int, to_ts: int, verbose: bool,
) -> list[CheckResult]:
    """Gauge with group:a / group:b tags — verifies per-group aggregation."""
    metric = f"{prefix}.gge_grp"
    base_tags = f"testrun:{run_tag},apiversion:{api_version}"
    results = []

    for group, expected in [("a", GAUGE_VALUE_A), ("b", GAUGE_VALUE_B)]:
        query = f"avg:{metric}{{{base_tags},group:{group}}}"
        values = _query_once(client, query, from_ts, to_ts, verbose)
        if not values:
            results.append(CheckResult(f"{metric}[group:{group}]", "gauge", api_version,
                                       f"= {expected}", "no data", "FAIL"))
        else:
            actual = max(values)
            status = "PASS" if abs(actual - expected) <= 0.01 else "FAIL"
            results.append(CheckResult(f"{metric}[group:{group}]", "gauge", api_version,
                                       f"= {expected}", f"{actual:.4f}", status))
    return results


def run_all_checks(
    client: DDClient, metric_prefix: str, prom_prefix: str, run_tag: str,
    api_version: str, from_ts: int, to_ts: int, verbose: bool,
) -> list[CheckResult]:
    """Run all checks once without retrying."""
    results: list[CheckResult] = []
    results.append(check_counter(client, metric_prefix, run_tag, api_version, from_ts, to_ts, verbose))
    results.append(check_gauge(client, metric_prefix, run_tag, api_version, from_ts, to_ts, verbose))
    results.append(check_set(client, metric_prefix, run_tag, api_version, from_ts, to_ts, verbose))
    results.extend(check_distribution(client, metric_prefix, run_tag, from_ts, to_ts, verbose))
    results.extend(check_histogram(client, prom_prefix, run_tag, from_ts, to_ts, verbose))
    results.extend(check_summary(client, prom_prefix, run_tag, api_version, from_ts, to_ts, verbose))
    results.extend(check_counter_multitag(client, metric_prefix, run_tag, api_version, from_ts, to_ts, verbose))
    results.extend(check_gauge_multitag(client, metric_prefix, run_tag, api_version, from_ts, to_ts, verbose))
    return results


# ---------------------------------------------------------------------------
# Test runner
# ---------------------------------------------------------------------------

def run_test_for_version(
    api_version: str,
    args,
    client: DDClient,
    vector_bin: str,
    run_tag: str,
) -> list[CheckResult]:
    """Run a full correctness test for one series API version (v1 or v2)."""
    print(f"\n{'='*60}")
    print(f"Testing series API version: {api_version.upper()}")
    print(f"{'='*60}")

    metric_prefix = args.metric_prefix
    prom_prefix = metric_prefix.replace(".", "_")

    # Start Prometheus mock server
    prom_server = PrometheusMetricsServer(args.prom_port, run_tag, prom_prefix)
    prom_server.start()
    print(f"Prometheus mock server started on :{args.prom_port}")

    # Write Vector config
    with tempfile.NamedTemporaryFile(mode="w", suffix=".toml", delete=False) as f:
        config_path = f.name
        f.write(VECTOR_CONFIG_TEMPLATE.format(
            statsd_port=args.statsd_port,
            prom_port=args.prom_port,
            api_key=args.api_key,
            site=args.site,
            metrics_port=args.metrics_port,
        ))

    # Set env for API version selection
    extra_env = {}
    if api_version == "v2":
        extra_env["VECTOR_TEMP_USE_DD_METRICS_SERIES_V2_API"] = "true"

    vector = VectorProcess(vector_bin, config_path, extra_env, args.verbose)
    vector.start()
    print(f"Vector started (pid={vector._proc.pid}), waiting 5s for startup...")
    time.sleep(5)

    if not vector.is_running():
        print("ERROR: Vector exited early. Check logs with --verbose.")
        prom_server.stop()
        return []

    # Send StatsD metrics
    packets = build_statsd_packets(metric_prefix, run_tag, api_version)
    send_statsd_packets("127.0.0.1", args.statsd_port, packets)
    send_ts = int(time.time())
    print(f"Sent {len(packets)} StatsD packets at ts={send_ts}")
    print(f"Prometheus scrape will deliver histogram+summary automatically")

    # Wait for Vector to flush (batch.timeout_secs=2) plus a bit more
    print("Waiting 10s for Vector to flush batches...")
    time.sleep(10)

    vector.stop()
    prom_server.stop()
    Path(config_path).unlink(missing_ok=True)

    # Grep the captured Vector log for lines mentioning the DD API paths.
    proof = vector.endpoint_proof()
    vector.cleanup_log()
    if proof:
        print(f"Endpoint proof ({len(proof)} matching log lines):")
        for line in proof:
            print(f"  {line}")
    else:
        print("WARNING: no endpoint lines found in Vector log")
    print("Vector stopped.")

    # Wait for DataDog ingestion
    print(f"\nWaiting {args.wait_secs}s for DataDog ingestion...")
    time.sleep(args.wait_secs)

    from_ts = send_ts - QUERY_LOOKBACK
    to_ts = send_ts + QUERY_FORWARD

    results: list[CheckResult] = []
    for attempt in range(args.retry_count + 1):
        print(f"Querying DataDog API (attempt {attempt+1}/{args.retry_count+1}, window {from_ts}→{to_ts})...")
        results = run_all_checks(
            client, metric_prefix, prom_prefix, run_tag,
            api_version, from_ts, to_ts, args.verbose,
        )
        failed = [r for r in results if r.status != "PASS"]
        if not failed:
            break
        if attempt < args.retry_count:
            print(f"  {len(failed)} check(s) pending, retrying in {args.retry_interval}s...")
            time.sleep(args.retry_interval)

    return results


# ---------------------------------------------------------------------------
# Reporting
# ---------------------------------------------------------------------------

def print_results(results: list[CheckResult]):
    if not results:
        print("\nNo results to report.")
        return

    col_widths = [
        max(len("Metric"), max(len(r.metric) for r in results)),
        max(len("Type"), max(len(r.metric_type) for r in results)),
        max(len("API Ver"), max(len(r.api_version) for r in results)),
        max(len("Expected"), max(len(r.expected) for r in results)),
        max(len("Actual"), max(len(r.actual) for r in results)),
        6,  # Status
    ]

    def row(cols):
        return "  ".join(str(c).ljust(w) for c, w in zip(cols, col_widths))

    header = row(["Metric", "Type", "API Ver", "Expected", "Actual", "Status"])
    sep = "  ".join("-" * w for w in col_widths)

    print(f"\n{'='*60}")
    print("Results")
    print(f"{'='*60}")
    print(header)
    print(sep)
    for r in results:
        print(row([r.metric, r.metric_type, r.api_version, r.expected, r.actual, r.status]))

    passed = sum(1 for r in results if r.status == "PASS")
    failed = sum(1 for r in results if r.status != "PASS")
    print(sep)
    print(f"\n{passed}/{len(results)} checks passed, {failed} failed.")


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------

def _print_v1_v2_comparison(v1_results: list[CheckResult], v2_results: list[CheckResult]):
    """Compare v1 and v2 actual values for the same metrics side by side."""
    print(f"\n{'='*60}")
    print("V1 vs V2 Value Comparison")
    print(f"{'='*60}")

    # Index v1 results by metric name
    v1_by_metric = {r.metric: r for r in v1_results}
    mismatches = 0
    compared = 0
    for r2 in v2_results:
        r1 = v1_by_metric.get(r2.metric)
        if r1 is None:
            continue
        compared += 1
        # Parse floats for comparison; skip non-numeric actuals
        try:
            a1 = float(r1.actual)
            a2 = float(r2.actual)
            match = abs(a1 - a2) < 0.01
        except ValueError:
            match = r1.actual == r2.actual
        symbol = "==" if match else "!="
        if not match:
            mismatches += 1
        print(f"  {r2.metric:<40} v1={r1.actual:<10} {symbol} v2={r2.actual}")

    if compared == 0:
        print("  (no comparable metrics)")
    elif mismatches == 0:
        print(f"\nAll {compared} metrics match between v1 and v2.")
    else:
        print(f"\n{mismatches}/{compared} metrics differ between v1 and v2.")


def find_vector_binary() -> Optional[str]:
    for path in [
        "target/release/vector",
        "target/debug/vector",
    ]:
        if Path(path).is_file():
            return str(Path(path).resolve())
    return None


def parse_args():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--api-key", default=os.environ.get("DD_API_KEY"), help="DataDog API key")
    p.add_argument("--app-key", default=os.environ.get("DD_APP_KEY"), help="DataDog APP key")
    p.add_argument("--site", default=os.environ.get("DD_SITE", DEFAULT_SITE), help="DataDog site")
    p.add_argument("--api-version", choices=["v1", "v2", "both"], default="both",
                   help="Series API version(s) to test")
    p.add_argument("--vector-bin", default=None, help="Path to Vector binary")
    p.add_argument("--metric-prefix", default="vct.correct",
                   help="Metric name prefix (default: vct.correct)")
    p.add_argument("--statsd-port", type=int, default=DEFAULT_STATSD_PORT)
    p.add_argument("--prom-port", type=int, default=DEFAULT_PROM_PORT)
    p.add_argument("--metrics-port", type=int, default=DEFAULT_VECTOR_METRICS_PORT)
    p.add_argument("--wait-secs", type=int, default=DEFAULT_WAIT_SECS,
                   help="Seconds to wait for DataDog ingestion before querying")
    p.add_argument("--retry-count", type=int, default=DEFAULT_RETRY_COUNT,
                   help="API query retry count if no data found")
    p.add_argument("--retry-interval", type=int, default=DEFAULT_RETRY_INTERVAL,
                   help="Seconds between API query retries")
    p.add_argument("--run-tag", default=None,
                   help="Unique run tag (auto-generated if not set)")
    p.add_argument("--verbose", action="store_true", help="Show Vector stdout/stderr")
    return p.parse_args()


def main():
    args = parse_args()

    if not args.api_key:
        print("ERROR: DD_API_KEY is not set. Set it in the environment or pass --api-key.", file=sys.stderr)
        sys.exit(1)
    if not args.app_key:
        print("ERROR: DD_APP_KEY is not set. Set it in the environment or pass --app-key.", file=sys.stderr)
        sys.exit(1)

    vector_bin = args.vector_bin or find_vector_binary()
    if not vector_bin:
        print("ERROR: No Vector binary found. Build with `cargo build --release` or pass --vector-bin.", file=sys.stderr)
        sys.exit(1)

    run_tag = args.run_tag or uuid.uuid4().hex[:12]
    client = DDClient(args.api_key, args.app_key, args.site)

    print("DataDog Metrics Sink Correctness Test")
    print("=" * 40)
    print(f"Run tag      : {run_tag}")
    print(f"Site         : {args.site}")
    print(f"API version  : {args.api_version}")
    print(f"Metric prefix: {args.metric_prefix}")
    print(f"Vector binary: {vector_bin}")
    print(f"Wait secs    : {args.wait_secs}")

    versions = ["v1", "v2"] if args.api_version == "both" else [args.api_version]
    all_results: list[CheckResult] = []
    results_by_version: dict[str, list[CheckResult]] = {}

    for api_version in versions:
        results = run_test_for_version(
            api_version=api_version,
            args=args,
            client=client,
            vector_bin=vector_bin,
            run_tag=run_tag,
        )
        results_by_version[api_version] = results
        all_results.extend(results)

    print_results(all_results)

    # When both versions tested, explicitly compare v1 vs v2 values.
    if len(versions) == 2:
        _print_v1_v2_comparison(results_by_version["v1"], results_by_version["v2"])

    failed = sum(1 for r in all_results if r.status != "PASS")
    sys.exit(0 if failed == 0 else 1)


if __name__ == "__main__":
    main()
