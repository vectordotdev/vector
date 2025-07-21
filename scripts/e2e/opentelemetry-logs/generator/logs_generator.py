#!/usr/bin/env python3

import argparse
import time
import requests
import json
import random
import uuid


SEVERITIES = ["DEBUG", "INFO", "WARN", "ERROR"]
PATHS = ["/", "/login", "/api/data", "/metrics", "/health"]
SERVICES = ["auth-service", "web-frontend", "device-ingestor", "payment-api", "metrics-collector"]


def generate_log(endpoint: str, count: int) -> bool:
    now_nanos = int(time.time() * 1e9)
    timestamp = time.strftime("%Y-%m-%dT%H:%M:%S%z")
    severity = random.choice(SEVERITIES)
    path = random.choice(PATHS)
    service = random.choice(SERVICES)
    log_id = str(uuid.uuid4())[:8]

    log_data = {
        "resourceLogs": [
            {
                "resource": {
                    "attributes": [
                        {"key": "service.name", "value": {"stringValue": service}}
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
                                    {"key": "request.path", "value": {"stringValue": path}},
                                    {"key": "log.id", "value": {"stringValue": log_id}},
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
        if response.status_code != 200:
            return (
                f"HTTP {response.status_code}\n"
                f"Response body:\n{response.text.strip() or '[empty]'}\n"
                f"Endpoint: {endpoint}\n"
                # f"Payload:\n{json.dumps(log_data, indent=2)}"
            )
        return True

    except requests.exceptions.RequestException as e:
        return (
            f"⚠️ RequestException: {str(e)}\n"
            f"Endpoint: {endpoint}\n"
            # f"Payload:\n{json.dumps(log_data, indent=2)}"
        )

def main():
    parser = argparse.ArgumentParser(description="Generate OTLP logs periodically.")
    parser.add_argument("--interval", type=float, default=5.0, help="Seconds between log sends (default: 5)")
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
        count += 1
        result = generate_log(endpoint, count)
        if result is True:
            print(f"✅ Sent log {count}")
            sent += 1
        elif isinstance(result, str):
            print(f"❌ Failed log {count}: {result}")
            failed += 1
        else:
            print(f"❌ Failed to send log {count}")
            failed += 1

        if 0 < args.n <= count:
            break

        time.sleep(args.interval)

    print(f"\n📊 Finished: Sent={sent}, Failed={failed}")


if __name__ == "__main__":
    main()
