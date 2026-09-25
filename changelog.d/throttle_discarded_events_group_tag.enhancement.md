Add `internal_metrics.include_group_tag` to the `throttle` transform. When enabled, `component_discarded_events_total` includes a `group` tag showing which `key_field` group exceeded the rate limit. This option is disabled by default because configurations with many group values can produce a large number of unique metric tags.

authors: arun.pidugu
