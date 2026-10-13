Bound the `sample` transform's per-group state with the new `max_state_bytes` option, which defaults to 10 MiB and evicts the least recently used groups to prevent unbounded memory growth from high-cardinality or very large `group_by` values. Each group counts the size of its rendered `group_by` value plus a fixed per-group overhead.

authors: thomasqueirozb
