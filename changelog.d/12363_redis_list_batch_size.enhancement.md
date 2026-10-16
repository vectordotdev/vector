The `redis` source now supports a `list.batch_size` option for the `list` data type. When set above `1`, the source still blocks until an entry is available, then reads up to `batch_size - 1` more entries in the same round trip, preserving list order. This greatly increases throughput and reduces the number of Redis commands when entries are queued. Values above `1` use the `count` argument of `LPOP`/`RPOP` (Redis 6.2 or newer); older servers fall back to pipelined single pops. The default of `1` keeps the previous behavior.

authors: dsavov
