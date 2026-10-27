Fixed a race in the `redis` source with `data_type: list` that could lose a list entry during graceful shutdown. The source now waits up to five seconds for an in-flight pop to finish and forwards any returned entry before stopping. Blocking pops time out after one second on an empty list, keeping shutdown responsive when Redis is healthy.

authors: dsavov
