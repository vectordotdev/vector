#!/usr/bin/env python3
"""Probe a Vector scaling experiment and emit one JSON result on stdout.

Usage: KUBECONFIG=/path/to/kubeconfig python3 scripts/probe.py [stable|measure|hpa]
Requires Python >= 3.12, kubectl, and grpcurl — nothing else; all cluster reads
go through `kubectl ... -o json`. Namespace, consumer, ingress-nginx, and the
producer must already be deployed. All diagnostics go to stderr; stdout carries
exactly one JSON object.
"""

import argparse
import json
import re
import signal
import subprocess
import sys
import tempfile
import time
from contextlib import ExitStack, contextmanager
from urllib.parse import quote

NAMESPACE = "vector-perf"
SELECTOR = "app.kubernetes.io/name=vector"


def log(message):
    print(f"==> {message}", file=sys.stderr, flush=True)


def kubectl(*args):
    # Every cluster read is one kubectl call parsed as structured JSON.
    return json.loads(
        subprocess.run(
            ["kubectl", "-n", NAMESPACE, *args, "-o", "json"],
            check=True,
            capture_output=True,
            text=True,
        ).stdout
    )


def kubectl_raw(path):
    # The metrics API is not a typed kubectl resource; --raw fetches its JSON
    # directly (and is mutually exclusive with -o).
    return json.loads(
        subprocess.run(
            ["kubectl", "get", "--raw", path],
            check=True,
            capture_output=True,
            text=True,
        ).stdout
    )


def command(*args):
    return subprocess.run(args, check=True, capture_output=True, text=True).stdout


def millicores(quantity):
    # Parse the CPU quantity formats kubectl emits (e.g. "899500000n", "500u", "1").
    match = re.fullmatch(r"(\d+(?:\.\d+)?)([numk]?)", quantity)
    if not match:
        raise ValueError(f"Unsupported CPU quantity: {quantity}")
    factor = {"n": 1e-6, "u": 1e-3, "m": 1.0, "": 1000.0, "k": 1e6}[match.group(2)]
    return float(match.group(1)) * factor


@contextmanager
def port_forward(pod, port):
    # Each scope owns exactly one process, including on failure or interruption.
    with tempfile.TemporaryFile(mode="w+") as output:
        process = subprocess.Popen(
            ["kubectl", "port-forward", "-n", NAMESPACE, f"pod/{pod}", f"{port}:8686"],
            stdout=output,
            stderr=subprocess.STDOUT,
        )
        try:
            deadline = time.monotonic() + 10
            while process.poll() is None and time.monotonic() < deadline:
                health = subprocess.run(
                    [
                        "grpcurl",
                        "-plaintext",
                        "-max-time",
                        "1",
                        f"localhost:{port}",
                        "grpc.health.v1.Health/Check",
                    ],
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                    check=False,
                )
                if health.returncode == 0:
                    yield port
                    return
                time.sleep(0.5)
            output.seek(0)
            raise RuntimeError(
                f"Port-forward to pod/{pod} did not become healthy: {output.read()}"
            )
        finally:
            if process.poll() is None:
                process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()


def snapshot(port):
    data = json.loads(
        command(
            "grpcurl",
            "-plaintext",
            "-d",
            "{}",
            f"localhost:{port}",
            "vector.observability.v1.ObservabilityService/GetComponents",
        )
    )
    for component in data.get("components", []):
        if component.get("componentId") == "in":
            metrics = component.get("metrics", {})
            return int(metrics.get("receivedBytesTotal", 0)), int(
                metrics.get("receivedEventsTotal", 0)
            )
    raise RuntimeError("No 'in' component found in the observability response")


def restart_counts(pods):
    return {
        pod["metadata"]["uid"]: tuple(
            c["restartCount"] for c in pod["status"].get("containerStatuses") or []
        )
        for pod in pods
    }


def measure_pods(pods):
    if not pods:
        raise RuntimeError("No running Vector pods to measure")
    before_restarts = restart_counts(pods)
    with ExitStack() as stack:
        ports = [
            stack.enter_context(port_forward(pod["metadata"]["name"], 18700 + i))
            for i, pod in enumerate(pods)
        ]
        before = [snapshot(port) for port in ports]
        time.sleep(30)
        after = [snapshot(port) for port in ports]
    # A container restart mid-window resets the process-local counters while
    # the pod (and its port-forward) stay alive: subtracting the pre-restart
    # values would then report negative or understated throughput.
    after_restarts = restart_counts(kubectl("get", "pods", "-l", SELECTOR)["items"])
    if after_restarts != before_restarts:
        raise RuntimeError(
            "A Vector container restarted during the measurement window "
            "(counters reset): " + f"{before_restarts} -> {after_restarts}"
        )
    # Keep the guide's two-snapshot, 30-second measurement method.
    byte_delta = sum(end[0] - start[0] for start, end in zip(before, after))
    event_delta = sum(end[1] - start[1] for start, end in zip(before, after))
    return {
        "Throughput": f"{byte_delta / 30 / 1048576:.2f} MiB/s",
        "Events/s": f"{event_delta / 30:.0f}",
    }


def measure_stable_pods(probe, attempts=3):
    """Measure without racing an HPA scale-down: a pod deleted mid-window
    aborts the measurement (its port-forward dies) instead of silently
    undercounting, and the pod set is re-read between attempts. Returns the
    measurement, the pod count it was taken against, and whether any retry
    was needed (a rescale or restart mid-window means the caller must not
    trust pre-measurement equilibrium evidence)."""
    for attempt in range(1, attempts + 1):
        pods = probe.pods(running=True)
        names = [pod["metadata"]["name"] for pod in pods]
        try:
            result = measure_pods(pods)
        except subprocess.CalledProcessError as error:
            if attempt == attempts:
                raise RuntimeError(
                    f"Measurement failed {attempts} times (pods {names}): "
                    f"{error.stderr or error.stdout}"
                ) from error
            log(
                f"Measurement attempt {attempt}/{attempts} on {len(pods)} pod(s) "
                f"failed (likely an HPA rescale mid-window); retrying..."
            )
            time.sleep(15)
            continue
        except RuntimeError as error:
            if attempt == attempts:
                raise
            log(f"Measurement attempt {attempt}/{attempts} failed ({error}); retrying...")
            time.sleep(15)
            continue
        if probe.pods(running=True) != pods:
            if attempt == attempts:
                raise RuntimeError(
                    f"Pod set changed during measurement ({len(pods)} pods); "
                    "the HPA has not settled"
                )
            log(
                f"Pod set changed during measurement attempt {attempt}/{attempts}; "
                "retrying..."
            )
            time.sleep(15)
            continue
        return result, len(pods), attempt > 1
    raise RuntimeError("unreachable")


class Probe:
    def pods(self, running=False):
        args = ["get", "pods", "-l", SELECTOR]
        if running:
            args.append("--field-selector=status.phase=Running")
        return kubectl(*args)["items"]

    def wait_stable(self):
        # Ready alone is insufficient: survive the initial load/OOM burst with
        # unchanged restart counts for 30 seconds before starting the warmup.
        log("Waiting for Vector pods to stabilise under load...")
        start = stable_since = time.monotonic()
        previous = None
        while time.monotonic() - start < 300:
            pods = self.pods()
            restarts = restart_counts(pods)
            ready = bool(pods) and all(
                any(
                    c["type"] == "Ready" and c["status"] == "True"
                    for c in pod["status"].get("conditions") or []
                )
                and pod["status"].get("containerStatuses")
                for pod in pods
            )
            now = time.monotonic()
            if not ready or restarts != previous:
                stable_since = now
            previous = restarts if ready else None
            log(f"[{int(now - start)}s] restarts={restarts} ready={ready}")
            if ready and now - stable_since >= 30:
                log("Pods stable and ready.")
                return
            time.sleep(5)
        raise RuntimeError("Vector pods did not stabilise within 300s")

    def avg_cpu(self):
        # values.yaml requests and limits one CPU per Vector pod; the
        # selector keeps the producer and consumer out of the average.
        pods = kubectl_raw(
            f"/apis/metrics.k8s.io/v1beta1/namespaces/{NAMESPACE}/pods"
            f"?labelSelector={quote(SELECTOR)}"
        )["items"]
        cores = sum(
            millicores(c["usage"]["cpu"]) for pod in pods for c in pod["containers"]
        )
        return f"{int(cores / len(pods) / 10)}%" if pods else "?"

    def measure(self):
        pods = self.pods(running=True)
        return {
            **measure_pods(pods),
            "Avg CPU": self.avg_cpu(),
            "Pods": str(len(pods)),
        }

    def rescale_events(self):
        # Retain the guide's SuccessfulRescale event-record baseline rather
        # than inferring scaling actions from sampled replica-count changes.
        try:
            events = kubectl(
                "get",
                "events",
                "--field-selector=involvedObject.kind=HorizontalPodAutoscaler",
            )["items"]
            return sum(event["reason"] == "SuccessfulRescale" for event in events)
        except subprocess.CalledProcessError:
            return 0

    def hpa_status(self):
        try:
            status = kubectl("get", "hpa", "vector")["status"]
            replicas = status.get("currentReplicas")
            desired = status.get("desiredReplicas")
            metrics = status.get("currentMetrics") or []
            resource = metrics[0].get("resource") if metrics else None
            current = resource.get("current") if resource else None
            cpu = current.get("averageUtilization") if current else None
        except (subprocess.CalledProcessError, IndexError, KeyError):
            return None, None, None
        return replicas, desired, cpu

    def wait_equilibrium(self, start):
        """Observe the HPA until it settles; returns (replicas, cpu,
        elapsed) at equilibrium."""
        last_replicas, last_stable, stable_count = 1, 0, 0
        while True:
            elapsed = int(time.monotonic() - start)
            if elapsed >= 900:
                raise RuntimeError(
                    f"HPA did not reach equilibrium within 900s "
                    f"(last: {last_replicas} pods)"
                )
            replicas, desired, cpu = self.hpa_status()
            if replicas is None or cpu is None:
                log(f"[{elapsed}s] HPA metrics unavailable; retrying...")
                time.sleep(15)
                continue
            if replicas != last_replicas:
                log(f"[{elapsed}s] SCALE {last_replicas}→{replicas} cpu={cpu}%")
                last_replicas = replicas
            else:
                log(f"[{elapsed}s] replicas={replicas} cpu={cpu}%")
            if desired != replicas:
                # The HPA intends another rescale (e.g. waiting out the
                # scaleDown stabilization window): not equilibrium yet.
                # Reset the settled evidence so a later transient
                # desired==current sample can't declare equilibrium on the
                # back of pre-rescale samples.
                log(
                    f"[{elapsed}s] pending rescale desired={desired} current={replicas}; "
                    "waiting for the HPA to settle..."
                )
                stable_count = 0
                time.sleep(15)
                continue
            stable_count = stable_count + 1 if replicas == last_stable else 1
            last_stable = replicas
            # Pending pods must not make a scale-out look stuck at maxReplicas.
            if replicas == 8 and cpu > 77 and stable_count >= 3:
                deployment = kubectl("get", "deployment", "vector")
                available = deployment["status"].get("availableReplicas") or 0
                if available == 8:
                    raise RuntimeError(
                        f"HPA at maxReplicas=8 with {cpu}% CPU > 77% — "
                        "cannot scale further; the cluster may be undersized"
                    )
                log(
                    f"HPA at maxReplicas with only {available}/8 replicas Ready; waiting..."
                )
                time.sleep(15)
                continue
            # Discrete HPA rounding means equilibrium need not be inside the
            # nominal CPU tolerance band: require 60s of replica-count
            # stability and the HPA not planning a rescale (desiredReplicas,
            # which holds through the scaleDown stabilization window).
            if stable_count >= 5 and elapsed > 120:
                log(f"Equilibrium: {replicas} pods, {cpu}% CPU, {elapsed}s elapsed.")
                return replicas, cpu, elapsed
            time.sleep(15)

    def hpa(self, baseline=None):
        # The HPA is already enabled; observe it to equilibrium. The
        # baseline (rescale events before the HPA was enabled) and the
        # elapsed clock may come from the caller: the HPA can reconcile
        # during the Helm task that enables it, so capturing them only
        # here would misattribute that first rescale and start late.
        log("HPA: observing already-enabled HPA (timeout 900s)...")
        start = time.monotonic()
        if baseline is None:
            baseline = self.rescale_events()
        replicas, cpu, elapsed = self.wait_equilibrium(start)
        log("HPA: measuring equilibrium throughput...")
        measurement, pod_count, retried = measure_stable_pods(self)
        if retried:
            # A rescale or restart happened during the measurement window:
            # the pre-measurement equilibrium evidence is stale, so confirm
            # the HPA has settled on the new pod set before reporting.
            log("HPA: pod set changed during measurement; re-checking equilibrium...")
            replicas, cpu, elapsed = self.wait_equilibrium(time.monotonic())
        # Re-read the HPA after the measurement window: a rescale during the
        # retries means the pre-measurement values no longer describe the
        # pod set the throughput was taken against.
        _, _, cpu = self.hpa_status()
        return {
            **measurement,
            "Avg CPU": f"{cpu}%" if cpu is not None else "?",
            "Pods": str(pod_count),
            "Scale events": str(self.rescale_events() - baseline),
            "Equilibrium": f"{elapsed}s",
        }


def interrupt(signum, _frame):
    sys.exit(128 + signum)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("stable", "measure", "hpa"))
    parser.add_argument(
        "--baseline",
        type=int,
        help="rescale-event count captured before the HPA was enabled",
    )
    args = parser.parse_args()
    signal.signal(signal.SIGINT, interrupt)
    signal.signal(signal.SIGTERM, interrupt)
    try:
        probe = Probe()
        if args.action == "stable":
            probe.wait_stable()
            # Emits the pre-HPA baseline so the enabling Helm task's own
            # reconciliations are not misattributed by the later hpa run.
            result = {"scale_events_baseline": probe.rescale_events()}
        elif args.action == "measure":
            result = probe.measure()
        else:
            result = probe.hpa(baseline=args.baseline)
        print(json.dumps(result))
        return 0
    except subprocess.CalledProcessError as error:
        log(f"ERROR: {error}: {error.stderr or error.stdout}")
    except (RuntimeError, ValueError, OSError) as error:
        log(f"ERROR: {error}")
    return 1


if __name__ == "__main__":
    sys.exit(main())
