---
title: The Vector Observability API
short: API
weight: 6
tags: ["api", "grpc"]
---

Vector ships with a [gRPC](https://grpc.io) API that allows you to interact with a running Vector
instance.

## Enable local inspection

The API is disabled by default. Enable it in the configuration of the Vector process you want to inspect:

```yaml
api:
  enabled: true
  address: "127.0.0.1:8686"
```

{{< warning >}}
The API has no authentication. Enable it only in isolated environments or for debugging,
and restrict access to trusted clients.
{{< /warning >}}

Start or reload Vector, then inspect it from another terminal:

```shell
vector top
vector tap --outputs-of my_transform
```

`vector top` requires an interactive terminal and displays component activity. To sample events with `vector tap`,
replace `my_transform` with a source or transform ID. Both commands connect to an already running Vector process.
If the connection fails, check `api.enabled`, `api.address`, and the client's `--url`.

For a walkthrough, see [Debug a pipeline with `vector top` and `vector tap`](/guides/developer/debugging/#vector-tools).

## Configuration

{{< config/group group="api" >}}

## Endpoints

{{< api/endpoints >}}

## How it works

The API exposes a gRPC service defined in [`proto/vector/observability.proto`](https://github.com/vectordotdev/vector/blob/master/proto/vector/observability.proto).
You can interact with it using any standard gRPC tooling.

For compatibility with Vector 0.54.0 and earlier, the HTTP `GET /health`
endpoint continues to be served on the same port as the gRPC API, so
existing HTTP probes (for example AWS ALB health checks and Kubernetes
HTTP liveness/readiness probes) keep working without changes. See the
[Endpoints](#endpoints) section above for details.

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

### Example using curl (HTTP health)

```bash
# 200 with body {"ok":true} while serving, 503 {"ok":false} during drain
curl -i http://localhost:8686/health
```
