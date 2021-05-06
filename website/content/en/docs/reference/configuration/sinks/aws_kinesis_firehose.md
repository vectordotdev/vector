---
title: AWS Kinesis Firehose logs
kind: sink
---

## Configuration

{{< component/config >}}

## Environment variables

{{< component/env-vars >}}

## Telemetry

{{< component/config >}}

## How it works

### AWS authentication

{{< snippet "aws/auth" >}}

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
