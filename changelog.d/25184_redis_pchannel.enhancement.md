The `redis` source now supports a `pchannel` data type for subscribing to Redis channels by pattern (`PSUBSCRIBE`). When using `pchannel`, the `key` is interpreted as a glob-style channel pattern, and the new optional `redis_channel` option records the concrete channel that matched the pattern on each event.

authors: divadpoc
