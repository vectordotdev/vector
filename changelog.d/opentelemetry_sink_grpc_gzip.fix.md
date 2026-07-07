The `opentelemetry` sink's gRPC transport now accepts gzip-compressed responses from
collectors when `compression = "gzip"` is configured. Previously, only outgoing request
compression was enabled, which caused collectors that mirror the request encoding in
responses to fail with `Unimplemented: Content is compressed with 'gzip' which isn't supported`.

authors: sakateka
