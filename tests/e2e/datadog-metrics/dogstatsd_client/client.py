from datadog import initialize, statsd
import time
import os
import random

STATSD_HOST = os.getenv('STATSD_HOST')

print(f"initializing for {STATSD_HOST}")

options = {
    'statsd_host':STATSD_HOST,
    'statsd_port':8125
}

initialize(**options)

# Give the Agent time to actually spin up.
# The container may return "ready" but the
# Agent process is still booting.
time.sleep(10)

hist_data = [
    9, 5, 0, 2, 16, 17, 8, 16, 10, 13,
    15, 3, 9, 13, 11, 17, 5, 18, 14, 9,
    4, 16, 9, 17, 4, 11, 7, 14, 8, 12,
    10, 9, 11, 3, 18, 12, 17, 12, 3, 19,
    9, 11, 19, 9, 15, 2, 7, 10, 4, 14
]

dist_data = [
    18, 5, 19, 0, 13, 12, 5, 12, 10, 4,
    1, 5, 7, 1, 14, 16, 20, 0, 8, 2, 4,
    20, 8, 4, 20, 6, 20, 3, 10, 11, 12,
    15, 2, 12, 5, 19, 19, 5, 9, 6, 18,
    19, 11, 6, 17, 5, 0, 1, 17, 17
]

for i in range(50):
    print("rate")
    statsd.increment('foo_metric.rate', tags=['a_tag:1'])

    print("gauge")
    statsd.gauge('foo_metric.gauge', i, tags=["a_tag:2"])

    print("set")
    statsd.set('foo_metric.set', i, tags=["a_tag:3"])

    print("histogram")
    statsd.histogram('foo_metric.histogram', hist_data[i], tags=["a_tag:4"])

    print("distribution")
    statsd.distribution('foo_metric.distribution', dist_data[i], tags=["a_tag:5"])

    statsd.flush()
    time.sleep(0.01)

