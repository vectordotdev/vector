---
title: Log to metric
kind: transform
---

## Configuration

{{< component/config >}}

## Output

{{< component/output >}}

## Telemetry

{{< component/telemetry >}}

## Examples

{{< component/examples >}}

## How it works

### Multiple metrics

For clarification, when you convert a single [log event][log] into multiple [metric events][metric], the metric events aren't emitted as a single array. They are emitted individually, and the downstream components treat them as individual events. Downstream components are not aware they were derived from a single log event.

### Null fields

If the target log [`field`](#field) contains a `null` value that value is ignored and no metric is emitted.

### Reducing

It's important to understand that this transform does not reduce multiple logs to a single metric. Instead, this transform converts logs into granular individual metrics that can then be reduced at the edge. Where the reduction happens depends on your metrics storage. For example, the [`prometheus_exporter` sink][prometheus_exporter] reduces logs in the sink itself for the next scrape, while other metric sinks proceed to forward the individual metrics for reduction in the metrics storage itself.

### State

{{< snippet "stateless" >}}

[log]: /docs/about/under-the-hood/architecture/data-model/log
[metric]: /docs/about/under-the-hood/architecture/data-model/metric
[prometheus_exporter]: /docs/reference/configuration/sinks/prometheus_exporter
