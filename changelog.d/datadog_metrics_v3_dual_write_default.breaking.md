The `datadog_metrics` sink's `dual_write` V3 shadow option is now enabled by default (with
`shadow_every: 1000`, sampling 1 in every 1000 legacy series flushes). This means Vector now sends
an additional, sampled V3-encoded payload to Datadog's shadow intake endpoint alongside the normal
legacy payload, without any configuration required.

Only series metrics are dual-written. Sketches (distributions and histograms) are never shadowed,
because the V3 sketches intake endpoints do not exist.

If you don't want this additional traffic, set `dual_write.enabled: false` on your `datadog_metrics`
sink configuration.

authors: stephenwakely
