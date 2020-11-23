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

We're big fans of Prometheus at Vector, and as an extension of our Kubernetes
integration we wanted to better understand how Vector could assist Prometheus
users. It is our desire to be the only tool needed to collect and process _all_
Kubernetes observability data, and working with Prometheus is core to our
metrics strategy. As a result we built two new components that assist with a
variety of Prometheus use cases:

1. [`prometheus_remote_write` source][prometheus_remote_write_source]
2. [`prometheus_remote_write` sink][prometheus_remote_write_sink]

## Use cases

After talking with a number of users we uncovered three very common
use cases that fit squarely into Vector's domain:

1. Exporting Prometheus data for long term archiving.
2. Exporting Prometheus data for long term querying.
3. Using Prometheus as a central proxy to export metrics to different solution.

##

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
