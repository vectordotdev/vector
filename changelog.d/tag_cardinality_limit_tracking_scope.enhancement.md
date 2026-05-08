The `tag_cardinality_limit` transform gained two new settings:

- **`tracking_scope`**: isolate tag tracking per metric (`per_metric`) instead of sharing a single bucket across all metrics (`global`, the default).
- **`max_tracked_keys`**: cap the total number of tag keys tracked to bound memory usage.

authors: ArunPiduguDD
