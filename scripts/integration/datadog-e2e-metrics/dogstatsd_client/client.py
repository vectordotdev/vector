from datadog import initialize, statsd
import time
import os

STATSD_HOST = os.getenv('STATSD_HOST')

print("initializing for {STATSD_HOST}")

options = {
    'statsd_host':STATSD_HOST,
    'statsd_port':8125
}

initialize(**options)

# time.sleep(5)

for i in range(50):
    print("rate")
    statsd.increment('foo_metric.rate', tags=['a_tag:1'])

    print("guage")
    statsd.gauge('foo_metric.gauge', i, tags=["a_tag:2"])

    print("set")
    statsd.set('foo_metric.set', i, tags=["a_tag:3"])

    print("histogram")
    statsd.histogram('foo_metric.histogram', random.randint(0, 20), tags=["a_tag:4"])

    print("distribution")
    statsd.distribution('foo_metric.distribution', random.randint(0, 20), tags=["a_tag:5"])

    statsd.flush()
    time.sleep(0.01)
