The `throttle` transform can add configurable event-derived tags to discard
metrics using `internal_metrics.events_discarded_tags`. Set `internal_metrics.include_key_tag`
to false to omit the throttle key from `events_discarded_total`. These options do not change
rate-limit buckets. Missing template fields warn and omit the tag; empty values omit it.

authors: akashvbabu91
