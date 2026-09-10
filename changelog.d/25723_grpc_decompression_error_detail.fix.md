gRPC-based sources (`vector`, `opentelemetry`) now include the underlying error when a compressed request payload fails to decompress, instead of the generic `reached impossible error during decompressor finalization` message. Decompression failures (for example, a corrupt or truncated gzip stream) are now diagnosable from the returned status and the sender's logs.

authors: Stunned1
