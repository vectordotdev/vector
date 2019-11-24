---
delivery_guarantee: "best_effort"
event_types: ["metric"]
issues_url: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22sink%3A+prometheus%22
operating_systems: ["linux","macos","windows"]
sidebar_label: "prometheus|[\"metric\"]"
source_url: https://github.com/timberio/vector/tree/master/src/sinks/prometheus.rs
status: "beta"
title: "prometheus sink"
unsupported_operating_systems: []
---

The `prometheus` sink [exposes](#exposing-and-scraping) [`metric`][docs.data-model#metric] events to [Prometheus][urls.prometheus] metrics service.

## Configuration

import Tabs from '@theme/Tabs';

<Tabs
  block={true}
  defaultValue="common"
  values={[
    { label: 'Common', value: 'common', },
    { label: 'Advanced', value: 'advanced', },
  ]
}>

import TabItem from '@theme/TabItem';

<TabItem value="common">

import CodeHeader from '@site/src/components/CodeHeader';

<CodeHeader fileName="vector.toml" learnMoreUrl="/docs/setup/configuration"/ >

```toml
[sinks.my_sink_id]
  type = "prometheus" # example, must be: "prometheus"
  inputs = ["my-source-id"] # example
  address = "0.0.0.0:9598" # example
  namespace = "service" # example
```

</TabItem>
<TabItem value="advanced">

<CodeHeader fileName="vector.toml" learnMoreUrl="/docs/setup/configuration" />

```toml
[sinks.my_sink_id]
  # REQUIRED
  type = "prometheus" # example, must be: "prometheus"
  inputs = ["my-source-id"] # example
  address = "0.0.0.0:9598" # example
  namespace = "service" # example
  
  # OPTIONAL
  buckets = [0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0] # default, seconds
  healthcheck = true # default
```

</TabItem>

</Tabs>

## Options

import Fields from '@site/src/components/Fields';

import Field from '@site/src/components/Field';

<Fields filters={true}>


<Field
  common={true}
  defaultValue={null}
  enumValues={null}
  examples={["0.0.0.0:9598"]}
  name={"address"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={true}
  templateable={false}
  type={"string"}
  unit={null}
  >

### address

The address to expose for scraping. See [Exposing & Scraping](#exposing-scraping) for more info.


</Field>


<Field
  common={false}
  defaultValue={[0.005,0.01,0.025,0.05,0.1,0.25,0.5,1.0,2.5,5.0,10.0]}
  enumValues={null}
  examples={[[0.005,0.01,0.025,0.05,0.1,0.25,0.5,1.0,2.5,5.0,10.0]]}
  name={"buckets"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  templateable={false}
  type={"[float]"}
  unit={"seconds"}
  >

### buckets

Default buckets to use for [histogram][docs.data-model.metric#histogram] metrics. See [Histogram Buckets](#histogram-buckets) for more info.


</Field>


<Field
  common={false}
  defaultValue={true}
  enumValues={null}
  examples={[true,false]}
  name={"healthcheck"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  templateable={false}
  type={"bool"}
  unit={null}
  >

### healthcheck

Enables/disables the sink healthcheck upon start.


</Field>


<Field
  common={true}
  defaultValue={null}
  enumValues={null}
  examples={["service"]}
  name={"namespace"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={true}
  templateable={false}
  type={"string"}
  unit={null}
  >

### namespace

A prefix that will be added to all metric names.
It should follow Prometheus [naming conventions][urls.prometheus_metric_naming].


</Field>


</Fields>

## Output

The `prometheus` sink [exposes](#exposing-and-scraping) [`metric`][docs.data-model#metric] events to [Prometheus][urls.prometheus] metrics service.
For example:

<Tabs
  block={true}
  defaultValue="histograms"
  values={[{"label":"Histograms","value":"histograms"},{"label":"Counters","value":"counters"},{"label":"Gauges","value":"gauges"}]}>

<TabItem value="histograms">

Given the following input:

```json
[
  {
    "histogram": {
      "name": "response_time_s",
      "val": 0.243
    }
  },
  {
    "histogram": {
      "name": "response_time_s",
      "val": 0.546
    }
  }
]
```

The following output will be produced:

```text
# HELP response_time_s response_time_s
# TYPE response_time_s histogram
response_time_s_bucket{le="0.005"} 0
response_time_s_bucket{le="0.01"} 1
response_time_s_bucket{le="0.025"} 0
response_time_s_bucket{le="0.05"} 1
response_time_s_bucket{le="0.1"} 0
response_time_s_bucket{le="0.25"} 0
response_time_s_bucket{le="0.5"} 0
response_time_s_bucket{le="1.0"} 0
response_time_s_bucket{le="2.5"} 0
response_time_s_bucket{le="5.0"} 0
response_time_s_bucket{le="10.0"} 0
response_time_s_bucket{le="+Inf"} 0
response_time_s_sum 0.789
response_time_s_count 2
```

</TabItem>

<TabItem value="counters">

Given the following input:

```json
[
  {
    "counter": {
      "name": "logins",
      "val": 1
    }
  },
  {
    "counter": {
      "name": "logins",
      "val": 3
    }
  }
]
```

The following output will be produced:

```text
# HELP logins logins
# TYPE logins counter
logins 4

```

</TabItem>

<TabItem value="gauges">

Given the following input:

```json
[
  {
    "gauge": {
      "name": "memory_rss",
      "val": 250,
      "direction": "plus"
    }
  },
  {
    "gauge": {
      "name": "memory_rss",
      "val": 25
      "direction": "minus"
    }
  }
]
```

The following output will be produced:

```text
# HELP memory_rss memory_rss
# TYPE memory_rss gauge
memory_rss 225

```

</TabItem>
</Tabs>

## How It Works

### Buffers

Due to the nature of Prometheus' pull model design the `prometheus`
sink does not utilize a buffer. You can read more about this in the [in-memory \
aggregation](#in-memory-aggregation) section.

### Environment Variables

Environment variables are supported through all of Vector's configuration.
Simply add `${MY_ENV_VAR}` in your Vector configuration file and the variable
will be replaced before being evaluated.

You can learn more in the [Environment Variables][docs.configuration#environment-variables]
section.

### Exposing & Scraping

The `prometheus` sink exposes data for scraping.
The[`address`](#address) option determines the address and port the data is made available
on. You'll need to configure your networking so that the configured port is
accessible by the downstream service doing the scraping.

### High Cardinality Names

High cardinality metric names and labels are [discouraged by \
Prometheus][urls.prometheus_high_cardinality] and you should consider alternative
strategies to reduce the cardinality as this can provide performance and
operation problems. In general, high cardinality analysis should be left logs
and storages designed for this use case (not Promtheus).

### Histogram Buckets

Choosing the appropriate buckets for Prometheus histgorams is a complicated
point of discussion. The [Histograms and Summaries Prometheus \
guide][urls.prometheus_histograms_guide] provides a good overview of histograms,
buckets, summaries, and how you should think about configuring them. The buckets
you choose should align with your known range and distribution of values as
well as how you plan to report on them. The aforementioned guide provides
examples on how you should align them.

#### Default buckets

The[`buckets`](#buckets) option defines the global default buckets for histograms:

```toml
[0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]
```

These defaults are tailored to broadly measure the response time (in seconds)
of a network service. Most likely, however, you will be required to define
buckets customized to your use case.

import Alert from '@site/src/components/Alert';

<Alert type="warning">

Note: These values are in `seconds`, therefore,
your metric values should also be in `seconds`.
If this is not the case you should adjust your metric or buckets to coincide.

</Alert>

### In-Memory Aggregation

Like other Prometheus instances, the `prometheus` sink aggregates
metrics in memory which keeps the memory footprint to a minimum if Prometheus
fails to scrape the Vector instance over an extended period of time. The
downside is that data will be lost if Vector is restarted. This is by design of
Prometheus' pull model approach, but is worth noting if restart Vector
frequently.

### Metric Definitions

By default, Vector will use the original metric names and labels set upon
creation. For most cases this makes Vector low friction, allowing you to define
the `prometheus` sink without tedious metrics definitions. You can
see examples of this in the [examples section](#examples).

### Metric Types

As described in the [metric data model][docs.data-model.metric] page, Vector offers
a variety of metric types. Their support, as as well as their mappings, are
described below:

| Vector Metric Type                               | Prometheus Metric Type               |
|:-------------------------------------------------|:-------------------------------------|
| [`counter`][docs.data-model.metric#counters]     | ['counter'][urls.prometheus_counter] |
| [`gauge`][docs.data-model.metric#gauges]         | ['gauge'][urls.prometheus_gauge]     |
| [`histogram`][docs.data-model.metric#histograms] | ['histogram'][urls.prometheus_gauge] |
| [`set`][docs.data-model.metric#sets]             | ⚠️ not supported                     |
| -                                                | ['summary'][urls.prometheus_summary] |

#### Sets

Prometheus does not have a [`set`][docs.data-model.metric#sets] type. Sets are
generally specific to [Statsd][urls.statsd_set], and if a set is received in the
`prometheus` sink it will be dropped, and a rate limited warning
level log will be output.

#### Summaries

Summaries are a Prometheus specific type and Vector does not default to them
by default. [issue #710][urls.issue_710] addresses the ability to define metrics,
including the ability change their types (such as changing them to `summary`
types).

#### OOM Errors

If you experience out of memory (OOM) errors it's likely you're using extremely
[high cardinality](#high-cardinality) metric names or labels. This is
[discouraged by Prometheus][urls.prometheus_high_cardinality] and you should
consider alternative strategies to reduce the cardinality. Such as leveraging
logs for high cardinality analysis. [Issue #387][urls.issue_387] discusses the
ability to provide safeguards around this. We encourage you to add to that
discussion with your use case if you find this to be a problem.


[docs.configuration#environment-variables]: /docs/setup/configuration#environment-variables
[docs.data-model#metric]: /docs/about/data-model#metric
[docs.data-model.metric#counters]: /docs/about/data-model/metric#counters
[docs.data-model.metric#gauges]: /docs/about/data-model/metric#gauges
[docs.data-model.metric#histogram]: /docs/about/data-model/metric#histogram
[docs.data-model.metric#histograms]: /docs/about/data-model/metric#histograms
[docs.data-model.metric#sets]: /docs/about/data-model/metric#sets
[docs.data-model.metric]: /docs/about/data-model/metric
[urls.issue_387]: https://github.com/timberio/vector/issues/387
[urls.issue_710]: https://github.com/timberio/vector/issues/710
[urls.prometheus]: https://prometheus.io/
[urls.prometheus_counter]: https://prometheus.io/docs/concepts/metric_types/#counter
[urls.prometheus_gauge]: https://prometheus.io/docs/concepts/metric_types/#gauge
[urls.prometheus_high_cardinality]: https://prometheus.io/docs/practices/naming/#labels
[urls.prometheus_histograms_guide]: https://prometheus.io/docs/practices/histograms/
[urls.prometheus_metric_naming]: https://prometheus.io/docs/practices/naming/#metric-names
[urls.prometheus_summary]: https://prometheus.io/docs/concepts/metric_types/#summary
[urls.statsd_set]: https://github.com/statsd/statsd/blob/master/docs/metric_types.md#sets
