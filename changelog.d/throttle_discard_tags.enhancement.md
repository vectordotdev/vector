The `throttle` transform can add configurable event-derived tags to per-key discard
metrics using `internal_metrics.events_discarded_tags`. These tags do not change
rate-limit buckets. Missing template fields warn and omit the tag; empty values omit it.

authors: akashvbabu91
