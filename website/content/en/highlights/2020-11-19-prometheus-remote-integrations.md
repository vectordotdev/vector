---
date: "2020-11-19"
title: "Support for the Prometheus remote write protocol"
description: "Interoperability with the Prometheus ecosystem."
authors: ["jamtur01"]
pr_numbers: [4856, 5144]
release: "0.11.0"
hide_on_release_notes: false
badges:
  type: "new feature"
  domains: ["metrics", "sources"]
---

We're big fans of Prometheus at Timber, and as an extension of our Kubernetes
integration we wanted to better understand how Vector could assist Prometheus
operators. As noted in the [Kubernetes highlight][kubernetes_highlight], it is
our intent to be the only tool needed to collect and process _all_ Kubernetes
observability data, and working with Prometheus is core to our
metrics strategy. As a result, 0.11.0 includes two new components that assist
with a variety of Prometheus use cases:

1. **A new [`prometheus_remote_write` source][prometheus_remote_write_source]**
2. **A new [`prometheus_remote_write` sink][prometheus_remote_write_sink]**

## Get Started

### Backing up Prometheus data

Using the new [`prometheus_remote_write` source][prometheus_remote_write_source],
Prometheus operators can route a stream of Prometheus data to the archiving
solution of their choice. For this use case we recommend object stores for their
cheap and durable qualities:

```toml title="vector.toml"
[sources.prometheus]
  type = "prometheus_remote_write"

[transforms.convert]
  type = "metric_to_log"
  inputs = ["prometheus"]

[sinks.backup]
  type = "aws_s3"
  inputs = ["convert"]
```

Swap `aws_s3` with `gcp_cloud_storage` or other object stores.

### Long-term, highly-available Prometheus setups

For large setups it's common to couple Prometheus with another solution designed
for long term storage and querying. This separates the concerns of fast short
term querying and long-term low cost archiving.

With the new [`prometheus_remote_write` source][prometheus_remote_write_source],
users can only retain near-term data in Prometheus and store long term metrics
in databases like [M3 (Chronosphere)][chronosphere],
[Victoria metrics][victoria_metrics], and [Timescale][timescale].

### Using Prometheus as a centralized export proxy

Not using Prometheus? It's very common to use Prometheus as a central proxy
for exporting all metrics data. This is especially relevant in ecosystems like
Kubernetes where Prometheus is tightly integrated.

To get started, setup the new
[`prometheus_remote_write` source][prometheus_remote_write_source] and send
your metrics to [Datadog][datadog], [New Relic][new_relic], [Influx][influx],
[Elasticsearch][elastic], and [more][sinks]:

```toml title="vector.toml"
[sources.prometheus]
  type = "prometheus_remote_write"

[sinks.datadog]
  type = "datadog_metrics"
  inputs = ["prometheus"]
```

[chronosphere]: https://chronosphere.io/
[datadog]: https://datadog.com
[elastic]: https://www.elastic.co/
[influx]: https://www.influxdata.com/
[kubernetes_highlight]: /highlights/2020-10-27-kubernetes-integration
[new_relic]: https://newrelic.com
[prometheus_remote_write_sink]: /docs/reference/configuration/sinks/prometheus_remote_write
[prometheus_remote_write_source]: /docs/reference/configuration/sources/prometheus_remote_write
[sinks]: /docs/reference/configuration/sinks/
[timescale]: https://www.timescale.com/
[victoria_metrics]: https://victoriametrics.com/
