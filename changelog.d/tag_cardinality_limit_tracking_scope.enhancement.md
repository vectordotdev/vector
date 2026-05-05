The `tag_cardinality_limit` transform gained a new top-level `tracking_scope` setting
that controls how tag tracking state is partitioned across metrics:

- `global` (default — preserves existing behavior): all metrics share a single
  tracking bucket, and the global `value_limit` caps the combined set of tag values
  across them.
- `per_metric`: every distinct metric gets its own tracking bucket, providing tag
  cardinality limiting for each metric in isolation at the cost of higher memory.

authors: ArunPiduguDD
