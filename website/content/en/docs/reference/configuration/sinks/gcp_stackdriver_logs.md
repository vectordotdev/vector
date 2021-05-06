---
title: GCP Operations (formerly Stackdriver) logs
short: GCP Stackdriver
kind: sink
---

## Configuration

{{< component/config >}}

## Environment variables

{{< component/env-vars >}}

## Telemetry

{{< component/config >}}

## How it works

### Buffers and batches

{{< snippet "buffers-and-batches" >}}

### GCP authentication

{{< snippet "gcp/auth" >}}

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
