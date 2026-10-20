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

DRAIN_TIMEOUT_SECONDS = 120


class DockerConnection(http.client.HTTPConnection):
    def connect(self):
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.settimeout(self.timeout)
        self.sock.connect("/var/run/docker.sock")


def docker_request(method, path):
    with closing(DockerConnection("localhost", timeout=40)) as connection:
        connection.request(method, path)
        response = connection.getresponse()
        body = response.read()
        if not 200 <= response.status < 300:
            raise RuntimeError(f"Docker {method} {path}: {response.status} {body.decode()}")
        return json.loads(body) if body else None


def stop_containers(network, label):
    filters = json.dumps({"network": [network], "label": [label]})
    containers = docker_request("GET", "/containers/json?" + urlencode({"filters": filters}))
    if not containers:
        raise RuntimeError(f"No running demo containers matching {label}")

    # Stop producers together, preserving the existing SDK shutdown grace period.
    with ThreadPoolExecutor(max_workers=len(containers)) as executor:
        list(executor.map(
            lambda container: docker_request("POST", f"/containers/{container['Id']}/stop?t=30"),
            containers,
        ))


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
    deadline = time.monotonic() + DRAIN_TIMEOUT_SECONDS
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
