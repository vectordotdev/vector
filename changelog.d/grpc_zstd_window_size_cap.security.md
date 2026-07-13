The `vector` source and the `opentelemetry` source's gRPC mode now bound gRPC message size at the header read and cap `gzip`/`zstd` decompression mid-stream, matching the decompressed-size limit enforced elsewhere, so a compressed gRPC message can no longer expand past the cap during decompression before being rejected.

authors: thomasqueirozb
