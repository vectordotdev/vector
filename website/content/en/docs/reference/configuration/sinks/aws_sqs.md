---
title: AWS SQS
kind: sink
---

## Configuration

{{< component/config >}}

## Environment variables

{{< component/env-vars >}}

## Telemetry

{{< component/telemetry >}}

## How it works

### AWS authentication

{{< snippet "aws/auth" >}}

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
