Add `internal_metrics.include_group_tag` to the `sample` transform. When enabled, `component_discarded_events_total` includes a `group` tag showing which `group_by` group an event belonged to when it was discarded. This option is disabled by default because configurations with many group values can produce a large number of unique metric tags.

authors: arun.pidugu
