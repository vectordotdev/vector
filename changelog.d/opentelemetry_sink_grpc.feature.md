Added gRPC transport support for the `opentelemetry` sink. Configure with `protocol.type = "grpc"` and set `protocol.endpoint` to your OTLP/gRPC endpoint (e.g. `http://localhost:4317`). Supports TLS and gzip compression.

authors: thomasqueirozb
