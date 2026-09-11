Bound the `sample` transform's per-group state with the new `max_groups` option, which defaults to 5,000 and evicts the least recently used group to prevent unbounded memory growth from high-cardinality `group_by` values.

authors: thomasqueirozb
