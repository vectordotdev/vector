---
date: "2022-01-19"
title: "New `output` tag in internal metrics"
description: "Adding an `output` tag for `component_sent_*` metrics"
authors: ["001wwang"]
pr_numbers: []
release: "0.20.0"
hide_on_release_notes: false
---

In light of our work to support multiple output streams for components, we've
added an `output` tag in the following metrics emitted by sources and transforms
to ensure a seamless observability experience. You will not see the `output` tag
for sink metrics because sinks currently do not use multiple outputs.

- `component_sent_events_total`
- `component_sent_event_bytes_total`
- `events_out_total` (Note, this metric is deprecated in favor of the above)

For example, the `remap` transform has a default output and an optionally
enabled `dropped` output. The default output is specified simply as the
`<component_id>` while the `dropped` output is specified as
`<component_id>.dropped` in downstream components. Metrics for events sent to
the default output will include a tag `output: _default`. Metrics for events
sent to the `dropped` output will include a tag `output: dropped`. In general,
all sources and transforms will use `output: _default` in metrics for events
sent to their default output.

```json
{"counter":{"value":1.0},"kind":"absolute","name":"component_sent_events_total","namespace":"vector","tags":{"component_id":"foo","component_kind":"transform","component_name":"foo","component_type":"remap","output":"_default"}}
{"counter":{"value":1.0},"kind":"absolute","name":"component_sent_events_total","namespace":"vector","tags":{"component_id":"foo","component_kind":"transform","component_name":"foo","component_type":"remap","output":"dropped"}}
```

For user-defined outputs, the `output` tag will be populated accordingly with
the user's custom output name. Importantly, to avoid naming collisions,
`_default` is now a reserved name and cannot be used for user-defined outputs.
The `route` transform is the only component that currently supports custom
outputs (each configured route is an output).

Components that can currently use multiple outputs include:

- [`datadog_agent` source][datadog_agent]
- [`remap` transform][remap]
- [`route` transform][route]

You can view information about all of a component's outputs on its documentation
page.

[datadog_agent]: https://vector.dev/docs/reference/configuration/sources/datadog_agent/
[remap]: https://vector.dev/docs/reference/configuration/transforms/remap/
[route]: https://vector.dev/docs/reference/configuration/transforms/route/
