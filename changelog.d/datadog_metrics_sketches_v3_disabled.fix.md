The `datadog_metrics` sink's `sketches_api_version: v3` option can no longer be configured; it
is rejected at config-load time with an `unknown variant` error.

Datadog's V3 sketches intake routes don't currently exist (both `/api/intake/metrics/v3/sketches`
and its beta counterpart return `404`), and a `404` response is treated as retriable, so a sink
configured this way would retry every sketches flush forever without ever delivering it.

`sketches_api_version: v2` (the default) is unaffected. `series_api_version: v3` is unaffected;
this only restricts the sketches endpoint. The V3 sketches encoder and its plumbing remain in the
codebase and are exercised by tests directly, so re-enabling it later is a small change once the
intake side is ready.

authors: stephenwakely
