Several fixes to the `tag_cardinality_limit` transform's sliding-window TTL:

- The probabilistic backend's `value_limit` cap is now enforced against an
  upper-bound estimate of the union cardinality across all generational
  shards. Previously the cap was checked against the maximum count of any
  single shard, which under-counts when distinct values are spread across
  shards (high-churn / low-repeat traffic) and silently admitted values past
  the configured `value_limit`.
- `ttl_generations` is now silently capped to `ttl_secs` when `ttl_secs` is
  smaller, so the configured TTL window is honored exactly. Previously the
  per-slice duration was floored to 1 second, which stretched the effective
  retention window to `ttl_generations` seconds when `ttl_secs <
  ttl_generations` (for example `ttl_secs: 1, ttl_generations: 4` kept values
  for about 4 seconds).
- Rolling-bloom refresh-on-sighting now sets the bits in the newest shard
  unconditionally instead of skipping the write when the bloom already reports
  the value as present. The skip path could leave a recently-observed value
  riding on another value's false-positive bits, so its lifetime depended on
  when those unrelated bits aged out rather than on its own activity.
- Pathological `ttl_secs` configs near `u64::MAX` no longer panic the
  transform on construction or rotation. `Instant + Duration` overflows are
  now caught and saturated so the transform stays alive instead of crashing
  on misconfiguration.
- When TTL eviction empties a `(metric, tag-key)` bucket, the slot is now
  reclaimed under `max_tracked_keys`. Previously the bucket lingered with an
  empty inner set and permanently consumed a slot, so high-churn workloads
  could hit the cap forever even after older pairs had fully aged out.
  Reclamation runs lazily on the cap-hit path, so steady-state allocations
  are unaffected.

authors: kaarolch
