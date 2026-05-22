The `tag_cardinality_limit` transform gained an optional sliding-window TTL
for tracked tag values, controlled by two new settings on the global block,
each `per_metric_limits` entry, and the `Inner`/`OverrideInner` config schemas:

- **`ttl_secs`**: expire tracked tag values after this many seconds since they
  were last seen. Useful when the downstream system (e.g. Datadog custom
  metrics) bills on a rolling unique-series window — without TTL, the
  cardinality cache saturates `value_limit` and starts rejecting fresh values
  long after the old ones have aged out of the billing window. When unset
  (default), behavior is unchanged from previous releases.
- **`ttl_generations`**: tune how the TTL window is sliced for the
  `probabilistic` backend. Defaults to `4` (eviction granularity =
  `ttl_secs / 4`). Memory cost is `ttl_generations * cache_size_per_key` per
  (metric, tag-key) pair; lower `cache_size_per_key` to keep total memory flat.
  In `exact` mode this knob only controls the sweep cadence.

A new internal counter `tag_cardinality_ttl_expirations_total` reports how
many distinct values are evicted by TTL.

authors: kaarolch
