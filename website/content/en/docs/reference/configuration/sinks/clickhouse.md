---
title: Clickhouse
description: Deliver log data to the [Clickhouse](https://clickhouse.tech) database
kind: sink
---

## Requirements

{{< component/requirements >}}

## Configuration

{{< component/config >}}

## Telemetry

{{< component/telemetry >}}

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

### State

{{< snippet "stateless" >}}

### Transport Layer Security (TLS)

{{< snippet "tls" >}}
