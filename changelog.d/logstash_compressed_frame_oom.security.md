The `logstash` source now caps the size of compressed frame payloads. Previously, the 32-bit payload size field of a compressed (`C`) frame was read straight from the wire and used to reserve a buffer, so a 6-byte malformed frame advertising a multi-gigabyte size could trigger an allocation large enough to abort the process. The size of the decompressed output is now 100MiB by default and can be configured by `--max-decompressed-size-bytes` or `VECTOR_MAX_DECOMPRESSED_SIZE_BYTES`. Oversized frames are rejected as a decode error.

authors: thomasqueirozb
