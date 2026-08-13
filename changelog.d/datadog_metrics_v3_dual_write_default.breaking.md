# `datadog_metrics` sink now dual-writes a V3 shadow payload by default when submitting directly to Datadog

The `datadog_metrics` sink's `dual_write` V3 shadow option is now enabled by default (with
`shadow_every: 1000`, sampling 1 in every 1000 legacy series flushes), but only when submitting
directly to Datadog (no custom `endpoint` configured). This means Vector now sends an additional,
sampled V3-encoded payload to Datadog's shadow intake endpoint alongside the normal legacy
payload, without any configuration required.

If a custom `endpoint` is configured (for example, a Datadog Agent, relay, or test collector),
dual-write defaults to **disabled** instead. The shadow route
(`/api/intake/metrics/v3beta/series`) is only guaranteed to exist on Datadog's own intake; hitting
it on a custom endpoint that doesn't implement it returns a `404`, which is treated as retriable,
so every sampled flush would otherwise add a request that retries forever.

Only series metrics are dual-written. Sketches (distributions and histograms) are never shadowed,
because the V3 sketches intake endpoints do not exist.

`dual_write.enabled` can be set explicitly to override either default in either direction: `true`
to opt in to shadow traffic against a custom endpoint, or `false` to disable it even when
submitting directly to Datadog.

authors: stephenwakely
