---
description: Exposes `metric` events to Prometheus metrics service.
---

<!--
     THIS FILE IS AUTOOGENERATED!

     To make changes please edit the template located at:

     scripts/generate/templates/docs/usage/configuration/sinks/prometheus.md.erb
-->

# prometheus sink

![][images.prometheus_sink]

{% hint style="warning" %}
The `prometheus` sink is in beta. Please see the current
[enhancements][url.prometheus_sink_enhancements] and
[bugs][url.prometheus_sink_bugs] for known issues.
We kindly ask that you [add any missing issues][url.new_prometheus_sink_issue]
as it will help shape the roadmap of this component.
{% endhint %}

The `prometheus` sink [exposes](#exposing-and-scraping) [`metric`][docs.metric_event] events to [Prometheus][url.prometheus] metrics service.

## Config File

{% code-tabs %}
{% code-tabs-item title="vector.toml (example)" %}
```coffeescript
[sinks.my_sink_id]
  type = "prometheus" # must be: "prometheus"
  inputs = ["my-source-id"]
  address = "0.0.0.0:9598"
  
  buckets = [0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0] # default, seconds
  healthcheck = true # default
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (schema)" %}
```coffeescript
[sinks.<sink-id>]
  type = "prometheus"
  inputs = ["<string>", ...]
  address = "<string>"
  buckets = [<float>, ...]
  healthcheck = <bool>
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (specification)" %}
```coffeescript
[sinks.prometheus_sink]
  # The component type
  # 
  # * required
  # * no default
  # * must be: "prometheus"
  type = "prometheus"

  # A list of upstream source or transform IDs. See Config Composition for more
  # info.
  # 
  # * required
  # * no default
  inputs = ["my-source-id"]

  # The address to expose for scraping.
  # 
  # * required
  # * no default
  address = "0.0.0.0:9598"

  # Default buckets to use for histogram metrics.
  # 
  # * optional
  # * default: [0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]
  # * unit: seconds
  buckets = [0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]

  # Enables/disables the sink healthcheck upon start.
  # 
  # * optional
  # * default: true
  healthcheck = true
```
{% endcode-tabs-item %}
{% endcode-tabs %}

## Options

| Key  | Type  | Description |
|:-----|:-----:|:------------|
| **REQUIRED** | | |
| `type` | `string` | The component type<br />`required` `must be: "prometheus"` |
| `inputs` | `[string]` | A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.config_composition] for more info.<br />`required` `example: ["my-source-id"]` |
| `address` | `string` | The address to expose for scraping. See [Exposing & Scraping](#exposing-scraping) for more info.<br />`required` `example: "0.0.0.0:9598"` |
| **OPTIONAL** | | |
| `buckets` | `[float]` | Default buckets to use for [histogram][docs.metric_event.histogram] metrics. See [Histogram Buckets](#histogram-buckets) for more info.<br />`default: [0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]` `unit: seconds` |
| `healthcheck` | `bool` | Enables/disables the sink healthcheck upon start.<br />`default: true` |

## Examples

{% tabs %}
{% tab title="Histograms" %}
This example demonstrates how Vector's internal [`histogram` metric \
type][docs.metric_event.histogram] is exposed via Prometheus' [text-based \
exposition format][url.prometheus_text_based_exposition_format].

For example, given the following internal Vector histograms:

```javascript
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
````

The `prometheus` sink will expose this data in Prometheus'
[text-based exposition format][url.prometheus_text_based_exposition_format]:

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

Note, the buckets used are those defined by the `buckets` option, and the
buckets above reflect the default buckets.

{% hint style="warning" %}
It's important that your metric units align with your bucket units. The default
buckets are in seconds and any metric values passed should also be in seconds.
If your metrics are not in seconds you can override the buckets to reflect
your units.
{% endhint %}
{% endtab %}
{% tab title="Counters" %}
This example demonstrates how Vector's internal [`counter` metric \
type][docs.metric_event.gauge] is exposed via Prometheus' [text-based \
exposition format][url.prometheus_text_based_exposition_format].

For example, given the following internal Vector gauges:

```javascript
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
````

The `prometheus` sink will expose this data in Prometheus'
[text-based exposition format][url.prometheus_text_based_exposition_format]:

```text
# HELP logins logins
# TYPE logins counter
logins 4
```

Notice that Vector aggregates the metric and exposes the final value.
{% endtab %}
{% tab title="Gauges" %}
This example demonstrates how Vector's internal [`gauge` metric \
type][docs.metric_event.gauge] is exposed via Prometheus' [text-based \
exposition format][url.prometheus_text_based_exposition_format].

For example, given the following internal Vector gauges:

```javascript
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
````

The `prometheus` sink will expose this data in Prometheus'
[text-based exposition format][url.prometheus_text_based_exposition_format]:

```text
# HELP memory_rss memory_rss
# TYPE memory_rss gauge
memory_rss 225
```

Notice that Vector aggregates the metric and exposes the final value.
{% endtab %}
{% endtabs %}

## How It Works

### Buffers

Due to the nature of Prometheus' pull model design the `prometheus`
sink does not utilize a buffer. You can read more about this in the [in-memory \
aggregation](#in-memory-aggregation) section.

### Delivery Guarantee

Due to the nature of this component, it offers a
[**best effort** delivery guarantee][docs.best_effort_delivery].

### Environment Variables

Environment variables are supported through all of Vector's configuration.
Simply add `${MY_ENV_VAR}` in your Vector configuration file and the variable
will be replaced before being evaluated.

You can learn more in the [Environment Variables][docs.configuration.environment-variables]
section.

### Exposing & Scraping

The `prometheus` sink exposes data for scraping.
The `address` option determines the address and port the data is made available
on. You'll need to configure your networking so that the configured port is
accessible by the downstream service doing the scraping.

### High Cardinality Names

High cardinality metric names and labels are [discouraged by \
Prometheus][url.prometheus_high_cardinality] and you should consider alternative
strategies to reduce the cardinality as this can provide performance and
operation problems. In general, high cardinality analysis should be left logs
and storages designed for this use case (not Promtheus).

### Histogram Buckets

Choosing the appropriate buckets for Prometheus histgorams is a complicated
point of discussion. The [Histograms and Summaries Prometheus \
guide][url.prometheus_histograms_guide] provides a good overview of histograms,
buckets, summaries, and how you should think about configuring them. The buckets
you choose should align with your known range and distribution of values as
well as how you plan to report on them. The aforementioned guide provides
examples on how you should align them.

#### Default buckets

The `buckets` option defines the global default buckets for histograms:

```coffeescript
[0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]
```

These defaults are tailored to broadly measure the response time (in seconds)
of a network service. Most likely, however, you will be required to define
buckets customized to your use case.

{% hint style="warning" %}
Note: These values are in `seconds`, therefore,
your metric values should also be in `seconds`.
If this is not the case you should adjust your metric or buckets to coincide.
{% endhint %}

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

As described in the [metric data model][docs.metric_event] page, Vector offers
a variety of metric types. Their support, as as well as their mappings, are
described below:

| Vector Metric Type                     | Prometheus Metric Type              |
|:---------------------------------------|:------------------------------------|
| [`counter`][docs.metric_event.counter] | ['counter'][url.prometheus_counter] |
| [`gauge`][docs.metric_event.gauge]     | ['gauge'][url.prometheus_gauge]     |
| [`histogram`][docs.metric_event.gauge] | ['histogram'][url.prometheus_gauge] |
| [`set`][docs.metric_event.gauge]       | ⚠️ not supported                    |
| -                                      | ['summary'][url.prometheus_summary] |

#### Sets

Prometheus does not have a [`set`][docs.metric_event.set] type. Sets are
generally specific to [Statsd][url.statsd_set], and if a set is received in the
`prometheus` sink it will be dropped, and a rate limited warning
level log will be emitted.

#### Summaries

Summaries are a Prometheus specific type and Vector does not default to them
by default. [issue #710][url.issue_710] addresses the ability to define metrics,
including the ability change their types (such as changing them to `summary`
types).

## Troubleshooting

The best place to start with troubleshooting is to check the
[Vector logs][docs.monitoring_logs]. This is typically located at
`/var/log/vector.log`, then proceed to follow the
[Troubleshooting Guide][docs.troubleshooting].

If the [Troubleshooting Guide][docs.troubleshooting] does not resolve your
issue, please:

1. Check for any [open `prometheus_sink` issues][url.prometheus_sink_issues].
2. If encountered a bug, please [file a bug report][url.new_prometheus_sink_bug].
3. If encountered a missing feature, please [file a feature request][url.new_prometheus_sink_enhancement].
4. If you need help, [join our chat/forum community][url.vector_chat]. You can post a question and search previous questions.

### OOM Errors

If you experience out of memory (OOM) errors it's likely you're using extremely
[high cardinality](#high-cardinality) metric names or labels. This is
[discouraged by Prometheus][url.prometheus_high_cardinality] and you should
consider alternative strategies to reduce the cardinality. Such as leveraging
logs for high cardinality analysis. [Issue #387][url.issue_387] discusses the
ability to provide safeguards around this. We encourage you to add to that
discussion with your use case if you find this to be a problem.

## Resources

* [**Issues**][url.prometheus_sink_issues] - [enhancements][url.prometheus_sink_enhancements] - [bugs][url.prometheus_sink_bugs]
* [**Source code**][url.prometheus_sink_source]


[docs.best_effort_delivery]: ../../../about/guarantees.md#best-effort-delivery
[docs.config_composition]: ../../../usage/configuration/README.md#composition
[docs.configuration.environment-variables]: ../../../usage/configuration#environment-variables
[docs.metric_event.counter]: ../../../about/data-model/metric.md#counter
[docs.metric_event.gauge]: ../../../about/data-model/metric.md#gauge
[docs.metric_event.histogram]: ../../../about/data-model/metric.md#histogram
[docs.metric_event.set]: ../../../about/data-model/metric.md#set
[docs.metric_event]: ../../../about/data-model/metric.md
[docs.monitoring_logs]: ../../../usage/administration/monitoring.md#logs
[docs.sources]: ../../../usage/configuration/sources
[docs.transforms]: ../../../usage/configuration/transforms
[docs.troubleshooting]: ../../../usage/guides/troubleshooting.md
[images.prometheus_sink]: ../../../assets/prometheus-sink.svg
[url.issue_387]: https://github.com/timberio/vector/issues/387
[url.issue_710]: https://github.com/timberio/vector/issues/710
[url.new_prometheus_sink_bug]: https://github.com/timberio/vector/issues/new?labels=Sink%3A+prometheus&labels=Type%3A+Bug
[url.new_prometheus_sink_enhancement]: https://github.com/timberio/vector/issues/new?labels=Sink%3A+prometheus&labels=Type%3A+Enhancement
[url.new_prometheus_sink_issue]: https://github.com/timberio/vector/issues/new?labels=Sink%3A+prometheus
[url.prometheus]: https://prometheus.io/
[url.prometheus_counter]: https://prometheus.io/docs/concepts/metric_types/#counter
[url.prometheus_gauge]: https://prometheus.io/docs/concepts/metric_types/#gauge
[url.prometheus_high_cardinality]: https://prometheus.io/docs/practices/naming/#labels
[url.prometheus_histograms_guide]: https://prometheus.io/docs/practices/histograms/
[url.prometheus_sink_bugs]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Sink%3A+prometheus%22+label%3A%22Type%3A+Bug%22
[url.prometheus_sink_enhancements]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Sink%3A+prometheus%22+label%3A%22Type%3A+Enhancement%22
[url.prometheus_sink_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Sink%3A+prometheus%22
[url.prometheus_sink_source]: https://github.com/timberio/vector/tree/master/src/sinks/prometheus.rs
[url.prometheus_summary]: https://prometheus.io/docs/concepts/metric_types/#summary
[url.prometheus_text_based_exposition_format]: https://github.com/prometheus/docs/blob/master/content/docs/instrumenting/exposition_formats.md#text-based-format
[url.statsd_set]: https://github.com/statsd/statsd/blob/master/docs/metric_types.md#sets
[url.vector_chat]: https://chat.vector.dev
