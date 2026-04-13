The `greptimedb_metrics` and `greptimedb_logs` sinks now require GreptimeDB v1.x. Users running GreptimeDB v0.x must upgrade their GreptimeDB instance before upgrading Vector.

The `grpc_compression` option no longer accepts `gzip`. Only `zstd` is supported. Users with `grpc_compression = "gzip"` must switch to `zstd` or remove the option.

authors: thomasqueirozb
