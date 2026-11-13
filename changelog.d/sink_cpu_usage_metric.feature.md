Sinks now support `measure_cpu_usage: true` to emit the `component_cpu_usage_ns_total`
counter. It covers the sink task and the tasks it spawns, such as request building and
HTTP connections. The `kafka` sink's compression and network I/O run on librdkafka threads
and are not counted.

authors: gwenaskell
