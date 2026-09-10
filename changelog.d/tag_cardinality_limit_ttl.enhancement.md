The `tag_cardinality_limit` transform gained an optional sliding-window TTL
for tracked tag values. Two new settings — `ttl_secs` and `ttl_generations` —
are available on the global block and each `per_metric_limits` entry; values
not observed within `ttl_secs` are evicted, freeing room under `value_limit`.
TTL is supported in `exact` and `probabilistic` modes; combining it with
`exact_fingerprint` is rejected at startup because that mode stores no
last-seen timestamps.
A new internal counter `tag_cardinality_ttl_expirations_total` reports the
number of cache slots reclaimed by TTL eviction — exact in `exact` mode, an
upper bound in `probabilistic` mode. See the transform documentation for
configuration details and mode-specific behavior.

authors: kaarolch
