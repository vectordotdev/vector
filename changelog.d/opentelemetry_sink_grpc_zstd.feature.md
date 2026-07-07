Added `compression = "zstd"` support for the `opentelemetry` sink's gRPC transport. Both
outgoing requests and incoming responses are negotiated with the collector using gRPC
compression headers.

authors: sakateka
