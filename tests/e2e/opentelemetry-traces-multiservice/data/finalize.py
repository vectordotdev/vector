"""Freeze both Collector captures before Rust validation starts."""

from concurrent.futures import ThreadPoolExecutor
from contextlib import closing
import http.client
import json
import os
from pathlib import Path
import socket
import time
from urllib.parse import urlencode

# Allow more than Vector's default 60-second graceful shutdown limit.
VECTOR_SHUTDOWN_TIMEOUT_SECONDS = 120
# Bound the wait for the output Collector to write all expected spans.
CAPTURE_DRAIN_TIMEOUT_SECONDS = 120


class DockerConnection(http.client.HTTPConnection):
    def connect(self):
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.settimeout(self.timeout)
        self.sock.connect("/var/run/docker.sock")


def docker_request(method, path, timeout=40):
    with closing(DockerConnection("localhost", timeout=timeout)) as connection:
        connection.request(method, path)
        response = connection.getresponse()
        body = response.read()
        if not 200 <= response.status < 300:
            raise RuntimeError(f"Docker {method} {path}: {response.status} {body.decode()}")
        return json.loads(body) if body else None


def stop_containers(network, label, timeout=30):
    filters = json.dumps({"network": [network], "label": [label]})
    containers = docker_request("GET", "/containers/json?" + urlencode({"filters": filters}))
    if not containers:
        raise RuntimeError(f"No running demo containers matching {label}")

    # Stop containers together; allow the API request to outlast Docker's grace period.
    with ThreadPoolExecutor(max_workers=len(containers)) as executor:
        list(executor.map(
            lambda container: docker_request(
                "POST", f"/containers/{container['Id']}/stop?t={timeout}", timeout=timeout + 10,
            ),
            containers,
        ))
    return containers


def span_count(path):
    # Only count for draining here. Rust decodes and validates the complete captures.
    return sum(
        len(scope.get("spans", []))
        for line in path.read_text().splitlines() if line.strip()
        for resource in json.loads(line).get("resourceSpans", [])
        for scope in resource.get("scopeSpans", [])
    )


def finalize(directory, network):
    if not (directory / "workload-complete").exists():
        raise RuntimeError("Demo workload did not complete")

    # Allow asynchronous Kafka consumers to process the final checkout.
    time.sleep(5)
    stop_containers(network, "vector.otel-traces-multiservice.producer=true")
    stop_containers(network, "com.docker.compose.service=otel-collector")
    expected = span_count(directory / "input.jsonl")
    # Keep the destination alive until Vector has flushed and exited, including any
    # late duplicate batches that the Rust comparison must be able to detect.
    for container in stop_containers(network, "com.docker.compose.service=vector", timeout=VECTOR_SHUTDOWN_TIMEOUT_SECONDS):
        state = docker_request("GET", f"/containers/{container['Id']}/json")["State"]
        if state["ExitCode"] != 0 or state["OOMKilled"]:
            raise RuntimeError(f"Vector did not shut down cleanly: {state}")

    deadline = time.monotonic() + CAPTURE_DRAIN_TIMEOUT_SECONDS
    received = 0
    while received < expected:
        try:
            received = span_count(directory / "output.jsonl")
        except (FileNotFoundError, json.JSONDecodeError):
            # The file exporter may still be creating or writing the capture.
            pass
        if received >= expected:
            break
        if time.monotonic() >= deadline:
            (directory / "drain-timeout").touch()
            break
        time.sleep(0.5)

    stop_containers(network, "com.docker.compose.service=otel-collector-sink")
    # A drain timeout is reported by Rust alongside its normal comparison diagnostics.
    (directory / "capture-complete").touch()
    print(f"Captures finalized: {expected} input spans, {received} output spans", flush=True)


if __name__ == "__main__":
    directory = Path("/output/opentelemetry-traces-multiservice") / os.environ["CONFIG_INGRESS_EXPORTER"]
    finalize(directory, os.environ["VECTOR_NETWORK"])
