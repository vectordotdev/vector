---
last_modified_on: "2020-11-19"
$schema: ".schema.json"
title: "New `prometheus_remote_write` sources and sinks"
description: "Export metrics out of Prometheus."
author_github: "https://github.com/binarylogic"
pr_numbers: [4856, 5144]
release: "0.11.0"
hide_on_release_notes: false
tags: ["type: announcement", "domain: metrics", "domain: sources"]
---

We're big fans of Prometheus at Timber, and as an extension of our Kubernetes
integration we wanted to better understand how Vector could assist Prometheus
oeprators. As noted in the [Kubernetes highlight][kubernetes_highlight], it is
our intent to be the only tool needed to collect and process _all_ Kubernetes
observability data, and working with Prometheus is core to our
metrics strategy. As a result, 0.11.0 includes two new components that assist
with a variety of Prometheus use cases:

1. [`prometheus_remote_write` source][prometheus_remote_write_source]
2. [`prometheus_remote_write` sink][prometheus_remote_write_sink]

## Get Started

### Backing up Prometheus data

Using the new [`prometheus_remote_write` source][prometheus_remote_write_source],
Prometheus operators can route a stream of Prometheus data to the archiving
solution of their choice. For this use case we recommend object stores for their
cheap and durable qualtiies:

```toml
[sources.prometheus]
  type = "prometheus_remote_write"

[transforms.convert]
  type = "metric_to_log"
  inputs = ["prometheus"]

[sinks.backup]
  type = "aws_s3"
  inputs = ["convert"]
```

Swap `aws_s3` with `gcp_cloud_storage` or other object stores.

### Long-term, highly-available Prometheus setups

For large setups it's common to couple Prometheus with another solution designed
for long term storage and querying. This reduces the pressure on Prometheus for
short-term, real-time data without sacrificing retention. Databases like
[M3 (Chronosphere)][], [Victoria metrics][],

### Using Prometheus as a centralized export proxy

Finally, because Prometheus is so tightly integrated into various ecosystems,
namely Kubernetes, it is often used a central proxy for exporting metrics data.
Using the new [`prometheus_remote_write` source][prometheus_remote_write_source],
make it possible to export metrics to other solutions, using them e

# Exporting Prometheus data to long term storage

Many Prometheus users reach for



Here at Vector we're big fans of Prometheus. It's an excellent monitoring tool
that has become a staple in the Kubernetes ecosystem. And after interviewing
a nume

We're big fans of Prometheus at Vector. It is a fantastic monitoring tool that
has become the defactor standard in Kubernetes.



 it's desirable to
export your metrics for long term storage to reduce cost and increase perofmrnace
for near-term metrics data.
