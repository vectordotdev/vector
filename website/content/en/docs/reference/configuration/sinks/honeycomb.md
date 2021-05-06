---
title: Honeycomb
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

1. Register for a free account at [honeycomb.io][honeycomb].
1. Once registered, create a new dataset and when presented with log shippers select the curl option and use the key provided with the curl example.

### State

{{< snippet "stateless" >}}

### Transport Layer Security (TLS)

{{< snippet "tls" >}}

[honeycomb]: https://ui.honeycomb.io/signup
