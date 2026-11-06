The `throttle` transform supports `internal_metrics.events_discarded_tags` to enable
`events_discarded_total` with selected event-derived tags. The map fully selects attribution
tags, including `key`; an empty map emits without attribution tags. Existing configurations
without the map retain their behavior. Rate-limit buckets are unchanged. Missing template
fields warn and omit the tag; empty values omit it.

authors: akashvbabu91
