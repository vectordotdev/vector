---
title: The Vector Observability API
short: API
weight: 6
tags: ["api", "grpc"]
---

Vector ships with a [gRPC](https://grpc.io) API that allows you to interact with a running Vector
instance. This page covers how to configure and enable Vector's API.

## Configuration

{{< config/group group="api" >}}

## Endpoints

{{< api/endpoints >}}

## How it works

The API exposes a gRPC service defined in [`proto/vector/observability.proto`](https://github.com/vectordotdev/vector/blob/master/proto/vector/observability.proto).
You can interact with it using any standard gRPC tooling.

### Example using grpcurl

```bash
# Check health (standard gRPC health check, compatible with Kubernetes gRPC probes)
grpcurl -plaintext localhost:8686 grpc.health.v1.Health/Check

# List components
grpcurl -plaintext localhost:8686 vector.observability.v1.ObservabilityService/GetComponents

# Stream events (tap)
grpcurl -plaintext \
  -d '{"outputs_patterns": ["*"], "limit": 100, "interval_ms": 500}' \
  localhost:8686 vector.observability.v1.ObservabilityService/StreamOutputEvents
```
