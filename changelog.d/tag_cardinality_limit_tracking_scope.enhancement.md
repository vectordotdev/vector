The `tag_cardinality_limit` transform gained two new top-level settings:

- **`tracking_scope`** controls how tag tracking state is partitioned across metrics:
  - `global` (default — preserves existing behavior): all metrics share a single
    tracking bucket, and the global `value_limit` caps the combined set of tag values
    across them.
  - `per_metric`: every distinct metric gets its own tracking bucket, providing tag
    cardinality limiting for each metric in isolation at the cost of higher memory.

- **`max_tracked_keys`** caps the total number of distinct (metric, tag-key) pairs
  tracked across the entire transform. When the cap is reached, additional pairs are
  not allocated and tag values for those pairs pass through unchecked (they are
  *not* dropped). Operators can detect this via the new
  `tag_cardinality_untracked_events_total` counter (incremented once per event with at
  least one untracked tag) and the `tag_cardinality_tracked_keys` gauge (current size
  of the cardinality cache). Defaults to unlimited, preserving existing behavior.

authors: ArunPiduguDD
