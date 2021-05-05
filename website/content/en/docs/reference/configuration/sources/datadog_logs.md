---
title: Datadog logs
kind: source
---

The `datadog_logs` source receives logs from a [Datadog Agent][datadog_agent] over HTTP or HTTPS.

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

### Transport Layer Security (TLS)

{{< snippet "tls" >}}

[datadog_agent]: https://docs.datadoghq.com/agent
