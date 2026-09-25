Added gRPC transport support for the `opentelemetry` sink, including TLS and gzip or zstd
compression for requests and responses. Set `protocol: grpc` and `uri` to your OTLP/gRPC
endpoint, such as `http://localhost:4317`.

authors: thomasqueirozb sakateka
