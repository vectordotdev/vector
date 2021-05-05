---
title: StatsD
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

### State

{{< snippet "stateless" >}}

### Timestamps

The StatsD protocol doesn't provide support for sending metric timestamps. Each parsed metric is assigned a `null` timestamp, which is a special value which means "a realtime metric," i.e. not a historical metric. Normally, such null timestamps are substituted by current time by downstream sinks or third-party services during sending/ingestion. See the [metric data model][metric] page for more info.

[metric]: /docs/about/under-the-hood/architecture/data-model/metric
