Sources now support `measure_cpu_usage: true` to emit the `component_cpu_usage_ns_total`
counter. It covers the source task and the tasks it spawns, such as per-connection decoding.
gRPC requests of the `vector` and `opentelemetry` sources are decoded on `tonic` connection
tasks, and the `kafka` source decompresses batches on librdkafka threads: neither is counted.

authors: gwenaskell
