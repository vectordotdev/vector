#!/usr/bin/env python3

import time
import requests
import json
import random


def generate_metric():
    otel_endpoint = "http://localhost:5318/v1/metrics"

    while True:
        now_nanos = int(time.time() * 1e9)

        metric_data = {
            "resourceMetrics": [
                {
                    "resource": {
                        "attributes": [
                            {"key": "service.name", "value": {"stringValue": "python-metric-generator"}}
                        ]
                    },
                    "scopeMetrics": [
                        {
                            "scope": {"name": "example-metrics"},
                            "metrics": [
                                {
                                    "name": "cpu.usage",
                                    "description": "Fake CPU usage",
                                    "unit": "%",
                                    "gauge": {
                                        "dataPoints": [
                                            {
                                                "attributes": [{"key": "host", "value": {"stringValue": "localhost"}}],
                                                "timeUnixNano": now_nanos,
                                                "asDouble": random.uniform(10, 90)
                                            }
                                        ]
                                    }
                                },
                                {
                                    "name": "requests.count",
                                    "description": "Fake request counter",
                                    "unit": "1",
                                    "sum": {
                                        "aggregationTemporality": "AGGREGATION_TEMPORALITY_CUMULATIVE",
                                        "isMonotonic": True,
                                        "dataPoints": [
                                            {
                                                "attributes": [{"key": "path", "value": {"stringValue": "/api"}}],
                                                "startTimeUnixNano": now_nanos - int(60 * 1e9),
                                                "timeUnixNano": now_nanos,
                                                "asInt": random.randint(100, 1000)
                                            }
                                        ]
                                    }
                                }
                            ]
                        }
                    ]
                }
            ]
        }

        try:
            resp = requests.post(
                otel_endpoint,
                data=json.dumps(metric_data),
                headers={"Content-Type": "application/json"}
            )
            if resp.status_code == 200:
                print("✅ Sent fake metrics")
            else:
                print(f"❌ Failed: {resp.status_code} - {resp.text}")
        except Exception as e:
            print(f"💥 Error sending metrics: {e}")

        time.sleep(5)


if __name__ == "__main__":
    generate_metric()
