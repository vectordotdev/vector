The `throttle` transform can add configurable event-derived tags to per-key discard
metrics using `internal_metrics.events_discarded_tags`. These tags do not change
rate-limit buckets; missing or empty template values omit the corresponding tag.

authors: akashvbabu91
