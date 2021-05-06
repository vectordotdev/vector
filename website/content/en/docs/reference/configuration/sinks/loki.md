---
title: Loki
kind: sink
---

## Configuration

{{< component/config >}}

## Telemetry

{{< component/telemetry >}}

## How it works

### Buffers and batches

{{< snippet "buffers-and-batches" >}}

### Concurrency

To make sure logs arrive at Loki in the correct order, the `loki` sink only sends one request at a time. Setting `request.concurrency` will not have any effects.

### Decentralized deployments

Loki currently does not support out-of-order inserts. If Vector is deployed in a decentralized setup then there is the possibility that logs might get rejected due to data races between Vector instances. To avoid this we suggest either assigning each Vector instance with a unique label or deploying a centralized Vector which will ensure no logs will get sent out-of-order.

### Event ordering

The `loki` sink ensures that all logs are sorted via their [`timestamp`](#timestamp). This ensures that logs are accepted by Loki. If no timestamp is supplied with events then the Loki sink will supply its own monotonically increasing timestamp.

### Health checks

{{< snippet "health-checks" >}}

### Partitioning

{{< snippet "partitioning" >}}

### Rate limits and adaptive concurrency

{{< snippet "arc" >}}

### Retry policy

{{< snippet "retry-policy" >}}

### State

{{< snippet "stateless" >}}

### Transport Layer Security (TLS)

{{< snippet "tls" >}}
