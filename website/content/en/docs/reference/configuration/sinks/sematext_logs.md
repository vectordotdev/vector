---
title: Sematext logs
kind: sink
---

## Configuration

{{< component/config >}}

## Telemetry

{{< component/config >}}

## How it works

### Buffers and batches

{{< snippet "buffers-and-batches" >}}

### Health checks

{{< snippet "health-checks" >}}

### Partitioning

{{< snippet "partitioning" >}}

### Rate limits and adaptive concurrency

{{< snippet "arc" >}}

### Retry policy

{{< snippet "retry-policy" >}}

### Setup

1. Register for a free account at [Sematext.com][sematext].
1. [Create a Logs App][app] to get a Logs Token for [Sematext Logs][logs].

### State

{{< snippet "stateless" >}}

[app]: https://apps.sematext.com/ui/integrations
[logs]: https://www.sematext.com/logsene
[sematext]: https://apps.sematext.com/ui/registration
