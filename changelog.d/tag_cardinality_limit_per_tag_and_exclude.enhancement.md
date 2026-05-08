The `tag_cardinality_limit` transform gained two new configuration capabilities:

- **Per-tag overrides** (`per_tag_limits`): configure cardinality limits per tag key within a metric, or exclude individual tags from tracking.
- **Metric exclusion**: opt entire metrics out of cardinality tracking via `mode: excluded` in `per_metric_limits`.

authors: ArunPiduguDD
