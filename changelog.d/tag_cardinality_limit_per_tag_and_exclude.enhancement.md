The `tag_cardinality_limit` transform gained two new configuration capabilities:

- **Per-tag overrides**: each entry in `per_metric_limits` now supports a `per_tag_limits`
  map whose entries can override settings for a specific tag key on that metric. The
  following per-tag fields are available:
  - `value_limit` (optional): caps distinct values for this tag key. Inherits from the
    enclosing per-metric `value_limit` when unset.
  - `excluded` (default `false`): when `true`, opts this tag out of cardinality tracking
    entirely — all values pass through unchecked.

  The tracking mode (`exact`/`probabilistic`), `cache_size_per_key`, `limit_exceeded_action`,
  and `internal_metrics` are always inherited from the enclosing per-metric configuration
  and cannot be overridden per-tag.

- **Metric exclusion**: `mode: excluded` is now available in `per_metric_limits` entries
  (not at the global level). When set, every tag on that metric is opted out of cardinality
  control — all tag values pass through and nothing is tracked. Any `per_tag_limits`
  overrides on an excluded metric are ignored.

authors: ArunPiduguDD
