The `opentelemetry` source now serves the standard `grpc.health.v1.Health` service on its gRPC
listener, matching the `vector` source. The aggregate (empty) service name reports `SERVING`, as do
`opentelemetry.proto.collector.logs.v1.LogsService`,
`opentelemetry.proto.collector.metrics.v1.MetricsService` and
`opentelemetry.proto.collector.trace.v1.TraceService`.

Previously a health check request to the OTLP gRPC port was unrouted and returned a bare HTTP 404
with no gRPC status, so load balancers and `grpc-health-probe` had no usable health endpoint and had
to probe an `Export` method instead. Each such probe also logged a `Grpc error` at `ERROR` level for
the source.

authors: stigglor
