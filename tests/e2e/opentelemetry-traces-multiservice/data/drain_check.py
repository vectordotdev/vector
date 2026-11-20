"""Wait for the output Collector to write every input span before it is stopped.

vdev runs this after Vector has stopped (see the `lifecycle` in config/test.yaml). A drain
timeout is reported by Rust alongside its normal comparison diagnostics instead of failing here.
"""

import json
import os
from pathlib import Path
import time

# Bound the wait for the output Collector to write all expected spans.
CAPTURE_DRAIN_TIMEOUT_SECONDS = 120


def span_count(path):
    # Only count for draining here. Rust decodes and validates the complete captures.
    return sum(
        len(scope.get("spans", []))
        for line in path.read_text().splitlines() if line.strip()
        for resource in json.loads(line).get("resourceSpans", [])
        for scope in resource.get("scopeSpans", [])
    )


def drain(directory):
    if not (directory / "workload-complete").exists():
        raise RuntimeError("Demo workload did not complete")

    expected = span_count(directory / "input.jsonl")
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
    print(f"Captures drained: {expected} input spans, {received} output spans", flush=True)


if __name__ == "__main__":
    drain(Path("/output/opentelemetry-traces-multiservice") / os.environ["CONFIG_INGRESS_EXPORTER"])
