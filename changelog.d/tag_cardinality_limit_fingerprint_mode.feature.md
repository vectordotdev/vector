The `tag_cardinality_limit` transform now supports `mode: exact_fingerprint`, a new storage
mode that can reduce memory usage for high-cardinality tag values compared to
`mode: exact`. Instead of storing the full tag-value strings, only a 64 bit fingerprint hash of
each value is kept. The trade-off is that throughput is slightly impacted due to extra hashing
operations, and there is technically a (unlikely) chance of collisions at very high cardinalities

authors: ArunPiduguDD
