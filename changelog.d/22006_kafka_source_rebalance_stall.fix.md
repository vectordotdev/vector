Fixed the `kafka` source silently stopping consumption of a partition that was re-assigned to it during a consumer group rebalance, which caused consumer lag to build up until the process was restarted. This could happen when pending acknowledgements could not be drained within `drain_timeout_ms`, for example under sink backpressure. In that situation the source now waits for the stopped partition consumers to fully end before the rebalance or shutdown continues, which can take slightly longer than `drain_timeout_ms`.

authors: sandervandegeijn
