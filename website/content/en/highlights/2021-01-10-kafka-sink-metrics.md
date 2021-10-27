---
date: "2021-01-10"
title: "The `kafka` sink now supports metrics"
description: "Send metric events to Kafka"
authors: ["binarylogic"]
featured: false
pr_numbers: []
release: "0.12.0"
hide_on_release_notes: false
badges:
  type: "enhancement"
  domains: ["metrics"]
  sinks: ["kafka"]
---

The [`kafka` sink][kafka_sink] now supports metrics, making it possible to send
metric events through Kafka. Metrics events are encoded into a format that
mimics our [internal metrics data model], ideal for custom consumers on the
other end. Getting started is easy:

```toml
[sources.host_metrics]
type = "host_metrics"

[sinks.kafka]
type = "kafka"
inputs = ["host_metrics"]
encoding.codec = "json"
```

## Caveats

We currently do not support ingesting metric events in the `kafka` source. This
is due to the hesitation to introduce yet another metrics format into the world.
Instead, we are working to support an open metrics format that the `kafka`
source and sink will support. See [issue 5809] for more info.

[issue 5809]: https://github.com/vectordotdev/vector/issues/5809
[kafka_sink]: /docs/reference/configuration/sinks/kafka/
[metrics data model]: /docs/about/under-the-hood/architecture/data-model/metric/#schema
