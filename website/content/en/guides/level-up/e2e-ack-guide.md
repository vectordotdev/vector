---
title: Ensure data is always successfully delivered
short: End-to-end acknowledgement
description: Learn how to use end-to-end acknowledgement
author_github: https://github.com/barieom
domain: delivery
weight: 7
tags: ["delivery", "logs", "level up", "guides", "guide"]
---

{{< requirement >}}
Before you begin, this guide assumes the following:

* You understand the [basic Vector concepts][concepts]
* You understand [how to set up a basic pipeline][pipeline]

[concepts]: /docs/about/concepts
[pipeline]: /docs/setup/quickstart
{{< /requirement >}}


Vector is often deployed in mission critical environments where loss of data is
non-negotiable. Ergo, assurance that every data flowing through your Vector
pipeline is successfully delivered to the end destination is critical.

We're excited to walk you through exactly you can accomplish this by using
Vector's end-to-end acknowledgement feature.

## Getting Started

You can set and control the acknowledgement feature either at the global level or
the source level. Let's start with the global configuration option first, as it is
easy as flipping on a switch. As you would expect from a global configuration, the
configuration below turns end-to-end acknowledgement on for every source:

```toml
acknowledgement = true
```

Even if you have a relatively complex topology and sending data from one source to
multiple sinks, by enabling this global config, all supported sources will wait for
acknowledgement from all the sinks. A source that does acknowledgements will wait
forever for an ack before responding

But you can enable acknowledgements at each sink level if you want more granular
control over how acknowledgements for specific cases (e.g. you're buffering your
data). You can set acknowledgements at the sink level by doing the following:

```toml
[sinks.very_cool_id]
   # Enable or disable waiting for acknowledgements for this sink.
   # Defaults to the global value of `acknowledgements`
   acknowledgement = true
```

## Edge cases

Unsurprisingly, there are a few exceptions and edge cases for the end-to-end
acknowledgement feature. First, as alluded to earlier, not all sources and sinks
are supported because some sources and sinks are unable to provide acknowledgements
at the protocol level. A list of sources and sinks that _are_ supported are:

#### Sources:
- `aws_kinesis_firehose`
- `aws_s3.cue`
- `aws_sqs.cue`
- `datadog_agent.cue`
- `file.cue`
- `fluent.cue`
- `heroku_logs.cue`
- `http.cue`
- `journald.cue`
- `kafka.cue`
- `logstash.cue`
- `prometheus_remote_write.cue`
- `splunk_hec.cue`
- `vector.cue`

#### Sinks:
- `aws_cloudwatch_logs.cue`
- `aws_cloudwatch_metrics.cue`
- `aws_kinesis_firehose.cue`
- `aws_kinesis_streams.cue`
- `aws_s3.cue`
- `aws_sqs.cue`
- `azure_blob.cue`
- `azure_monitor_logs.cue`
- `blackhole.cue`
- `clickhouse.cue`
- `datadog_archives.cue`
- `datadog_events.cue`
- `datadog_logs.cue`
- `datadog_metrics.cue`
- `elasticsearch.cue`
- `file.cue`
- `gcp_cloud_storage.cue`
- `gcp_pubsub.cue`
- `gcp_stackdriver_logs.cue`
- `gcp_stackdriver_metrics.cue`
- `honeycomb.cue`
- `http.cue`
- `humio.cue`
- `influxdb_logs.cue`
- `influxdb_metrics.cue`
- `kafka.cue`
- `logdna.cue`
- `loki.cue`
- `new_relic.cue`
- `new_relic_logs.cue`
- `redis.cue`
- `sematext_logs.cue`
- `sematext_metrics.cue`
- `splunk_hec_logs.cue`
- `splunk_hec_metrics.cue`
- `vector.cue`

Second, when buffering your observability data, the behavior of end-to-end
acknowledgement is a little different. When an event is persisted in a buffer, the
event will be marked as acknowledged to the source.

Third, the final edge case is if you are using any user-space transforms, such
as Lua. Because user-space transforms can drop, combine, split, or create events
that may have no relation to the source event, tracking whether an event has been
successfully delivered gets extremely complex. As such, end-to-end acknowledgement
behavior will be put in the hands of script writers.

## Parting thoughts

This guide intended to serve as a basic guide on how you can start
leveraging `acknowledgement`. If you have any feedback regarding this feature,
please let us know on our [Discord chat] or [Twitter], along with
any feedback or request for additional support you have for the Vector team!

[Discord chat]: https://discord.com/invite/dX3bdkF
[Twitter]: https://twitter.com/vectordotdev
