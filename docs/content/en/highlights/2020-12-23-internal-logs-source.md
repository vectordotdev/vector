---
date: "2020-12-23"
title: "The `internal_logs` source"
description: "A new source for observing Vector itself"
authors: ["lucperkins"]
pr_numbers: []
release: "0.12.0"
hide_on_release_notes: false
badges:
  type: "new feature"
  sources: ["internal_logs"]
---

Vector has a new [`internal_logs`][internal_logs] source that you can use to
process log events produced by Vector itself. Here's an example Vector log
message:

```json
{
  "*": null,
  "message": "Vector has started.",
  "metadata": {
    "kind": "event",
    "level": "TRACE",
    "module_path": "vector::internal_events::heartbeat",
    "target": "vector"
  },
  "timestamp": "2020-10-10T17:07:36+00:00"
}
```

`internal_logs` is a helpful accompaniment to the
[`internal_metrics`][internal_metrics] source, which exports Vector's own
metrics and modify and ship them however you wish.

## Example usage

Here's an example Vector configuration that ships Vector's logs to Splunk and
allows its internal metrics to be scraped by [Prometheus]:

```toml
[sources.vector_logs]
type = "internal_logs"

[sources.vector_metrics]
type = "internal_metrics"

[sinks.splunk]
type = "splunk_hec"
inputs = ["vector_logs"]
endpoint = "https://my-account.splunkcloud.com"
token = "${SPLUNK_HEC_TOKEN}"
encoding.codec = "json"

[sinks.prometheus]
type = "prometheus"
inputs = ["vector_metrics"]
address = "0.0.0.0:9090"
```

[internal_logs]: /docs/reference/configuration/sources/internal_logs
[internal_metrics]: /docs/reference/configuration/sources/internal_metrics
[prometheus]: https://prometheus.io
