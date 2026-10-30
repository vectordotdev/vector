"""Stop the pipeline in dependency order so both fakeintake captures are complete.

Each producer flushes its tracer when stopped, the Agent flushes to both endpoints when
stopped, and Vector drains its sink during graceful shutdown. Rust validation then reads
the still-running fakeintakes, so it can be repeated without rerunning the workload.
"""

from concurrent.futures import ThreadPoolExecutor
from contextlib import closing
import http.client
import json
import os
from pathlib import Path
import socket
from urllib.parse import urlencode

OUTPUT = Path("/output/datadog-traces")
# Allow more than Vector's default 60-second graceful shutdown limit.
VECTOR_STOP_TIMEOUT_SECONDS = 90
# Vector exits 0 even when graceful shutdown times out, so check its logs too.
VECTOR_SHUTDOWN_TIMEOUT_MESSAGE = b"Failed to gracefully shut down in time"


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
        return body


def stop_containers(network, label, timeout=30):
    filters = json.dumps({"network": [network], "label": [label]})
    containers = json.loads(docker_request("GET", "/containers/json?" + urlencode({"filters": filters})))
    if not containers:
        raise RuntimeError(f"No running containers matching {label}")

    # Allow the API request to outlast Docker's grace period.
    with ThreadPoolExecutor(max_workers=len(containers)) as executor:
        list(executor.map(
            lambda container: docker_request(
                "POST", f"/containers/{container['Id']}/stop?t={timeout}", timeout=timeout + 10,
            ),
            containers,
        ))

    for container in containers:
        state = json.loads(docker_request("GET", f"/containers/{container['Id']}/json"))["State"]
        if state["ExitCode"] != 0 or state["OOMKilled"]:
            raise RuntimeError(f"{container['Names'][0]} did not stop cleanly: {state}")
    return containers


def finalize(network):
    stop_containers(network, "vector.datadog-traces.producer=true")
    stop_containers(network, "com.docker.compose.service=datadog-agent")
    for container in stop_containers(
        network, "com.docker.compose.service=vector", timeout=VECTOR_STOP_TIMEOUT_SECONDS,
    ):
        logs = docker_request("GET", f"/containers/{container['Id']}/logs?stdout=1&stderr=1")
        if VECTOR_SHUTDOWN_TIMEOUT_MESSAGE in logs:
            raise RuntimeError("Vector did not drain its sink before shutting down")
    (OUTPUT / "capture-complete").touch()
    print("Captures finalized", flush=True)


if __name__ == "__main__":
    finalize(os.environ["VECTOR_NETWORK"])
