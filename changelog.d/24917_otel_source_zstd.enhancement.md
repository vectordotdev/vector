The `opentelemetry` source's gRPC OTLP receiver now accepts `zstd`-compressed
requests in addition to `gzip`, matching the compression schemes advertised via
the `grpc-accept-encoding` response header. No configuration change is required;
clients can send OTLP payloads with `grpc-encoding: zstd` and they will be
transparently decompressed.
