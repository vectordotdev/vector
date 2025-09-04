#!/usr/bin/env python3

import argparse
import json
import random
import time
import uuid

import requests

SEVERITIES = ["DEBUG", "INFO", "WARN", "ERROR"]
PATHS = ["/", "/login", "/api/data", "/metrics", "/health"]

def generate_log(endpoint: str, count: int) -> dict:
    now_nanos = time.time_ns()
    timestamp = time.strftime("%Y-%m-%dT%H:%M:%S%z")
    severity = random.choice(SEVERITIES)
    log_id = str(uuid.uuid4())[:8]

    log_data = {
        "resourceLogs": [
            {
                "resource": {
                    "attributes": [
                        {"key": "service.name", "value": {"stringValue": "opentelemetry-logs"}}
                    ]
                },
                "scopeLogs": [
                    {
                        "scope": {"name": "log-generator"},
                        "logRecords": [
                            {
                                "timeUnixNano": now_nanos,
                                "severityText": severity,
                                "body": {"stringValue": f"[{log_id}] {severity} log {count} at {timestamp}"},
                                "attributes": [
                                    {"key": "count", "value": {"intValue": count}}
                                ]
                            }
                        ]
                    }
                ]
            }
        ]
    }

    try:
        response = requests.post(
            endpoint,
            data=json.dumps(log_data),
            headers={"Content-Type": "application/json"},
            timeout=2
        )
        if response.status_code == 200:
            return {
                "success": True,
                "message": f"Log {count} sent successfully",
                "log_id": log_id,
                "status_code": response.status_code
            }
        else:
            return {
                "success": False,
                "message": f"HTTP {response.status_code}: {response.text.strip() or '[empty]'}",
                "log_id": log_id,
                "status code": response.status_code,
            }

    except requests.exceptions.RequestException as e:
        return {
            "success": False,
            "message": f"RequestException: {str(e)}",
            "log_id": log_id,
        }


def non_negative_float(value):
    f = float(value)
    if f < 0:
        raise argparse.ArgumentTypeError(f"Interval must be non-negative, got {value}")
    return f


def main():
    parser = argparse.ArgumentParser(description="Generate OTLP logs periodically.")
    parser.add_argument(
        "--interval",
        type=non_negative_float,
        help="Seconds between log sends (non-negative, optional)"
    )
    parser.add_argument("-n", type=int, default=0, help="Total logs to send (0 or negative = infinite)")
    parser.add_argument("--host", type=str, default="otel-collector-source", help="Host for the OTLP collector")
    parser.add_argument("--port", type=int, default=4318, help="Port for OTLP HTTP logs")
    parser.add_argument("--path", type=str, default="/v1/logs", help="OTLP HTTP logs path")

    args = parser.parse_args()
    endpoint = f"http://{args.host}:{args.port}{args.path}"

    print(f"Starting log generator → {endpoint}")

    count = 0
    sent = 0
    failed = 0

    while True:
        result = generate_log(endpoint, count)
        count += 1
        if result["success"]:
            print(f"✅ Sent log {count} (ID: {result['log_id']})")
            sent += 1
        else:
            print(f"❌ Failed log {count} (ID: {result['log_id']}): {result['message']}")
            failed += 1

        if 0 < args.n <= count:
            break

        if args.interval is not None:
            time.sleep(args.interval)

    print(f"\n📊 Finished: Sent={sent}, Failed={failed}")


if __name__ == "__main__":
    main()
