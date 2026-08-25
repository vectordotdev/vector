The `kafka` source now supports a `decompression` option for decompressing message payloads that
were compressed by the producer at the application level (as opposed to Kafka protocol-level
compression, which is handled transparently by the client library). Supported algorithms are
`gzip`, `zlib`, and `zstd`, and zstd decompression supports custom dictionaries via
`dictionary_path`. Payloads are decompressed before framing and decoding are applied.

authors: cjford
