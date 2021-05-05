---
title: Prometheus remote write
description: Collect metrics from [Prometheus](https://prometheus.io)
kind: source
---

## Configuration

{{< component/config >}}

## Output

{{< component/output >}}

## Telemetry

{{< component/telemetry >}}

## How it works

### Context

{{< snippet "context" >}}

### Metric type interpretation

The `remote_write` protocol used by this source transmits only the metric tags, timestamp, and numerical value. No explicit information about the original type of the metric (i.e. counter, histogram, etc) is included. This source guesses what the original metric type was.

For metrics named with a suffix of `_total`, this source emits the value as a counter metric. All other metrics are emitted as gauges.

### State

{{< snippet "stateless" >}}

### Transport Layer Security (TLS)

{{< snippet "tls" >}}
