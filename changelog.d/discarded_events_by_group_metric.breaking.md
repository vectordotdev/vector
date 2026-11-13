The `throttle` and `sample` transforms' `internal_metrics.include_group_tag` option now counts discarded events per group in a new internal metric, `component_discarded_events_by_group_total`, instead of adding a `group` tag to `component_discarded_events_total`. `component_discarded_events_total` is now always reported without a `group` tag.

If you query `component_discarded_events_total` by `group`, query `component_discarded_events_by_group_total` instead.
authors: ArunPiduguDD
