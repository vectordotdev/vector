The `tag_cardinality_limit` transform gained two new configuration capabilities:

- **Per-tag overrides**: each entry in `per_metric_limits` now supports a `per_tag_limits` map
  whose entries can override `value_limit` and `mode` for a specific tag key. When a per-tag
  entry omits `value_limit`, it inherits the enclosing per-metric (or global) `value_limit`
  rather than falling back to the default. `limit_exceeded_action` and `internal_metrics`
  are always inherited from the enclosing per-metric (or global) configuration and are not
  per-tag-overridable. Resolution order is per-tag → per-metric → global.
- **Exclusion**: `mode: excluded` is now available as a third mode option in `per_metric_limits`
  and `per_tag_limits` entries (not at the global level). When set, the metric or tag is opted
  out of cardinality control — all tag values pass through and nothing is tracked. Other
  tracking fields on the entry (`value_limit`) are ignored when `mode: excluded` is selected.
  Per-metric exclusion is blanket: when a metric's `mode` is `excluded`, every tag on that
  metric is excluded and any `per_tag_limits` overrides on it are ignored.

authors: ArunPiduguDD
