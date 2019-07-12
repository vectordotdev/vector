---
description: Accepts `log` events and allows you to convert logs into one or more metrics.
---

<!--
     THIS FILE IS AUTOOGENERATED!

     To make changes please edit the template located at:

     scripts/generate/templates/docs/usage/configuration/transforms/log_to_metric.md.erb
-->

# log_to_metric transform

![][images.log_to_metric_transform]


The `log_to_metric` transform accepts [`log`][docs.log_event] events and allows you to convert logs into one or more metrics.

## Config File

{% code-tabs %}
{% code-tabs-item title="vector.toml (example)" %}
```coffeescript
[transforms.my_log_to_metric_transform_id]
  # REQUIRED - General
  type = "log_to_metric" # must be: "log_to_metric"
  inputs = ["my-source-id"]
  
  # REQUIRED - Metrics
  [[transforms.my_log_to_metric_transform_id.metrics]]
    type = "counter" # enum: "counter", "gauge"
    field = "duration"
    
    increment_by_value = false # default
    name = "duration_total" # default
    labels = {host = "${HOSTNAME}", region = "us-east-1"}
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (schema)" %}
```coffeescript
[transforms.<transform-id>]
  # REQUIRED - General
  type = "log_to_metric"
  inputs = ["<string>", ...]

  # REQUIRED - Metrics
  [[transforms.<transform-id>.metrics]]
    type = {"counter" | "gauge"}
    field = "<string>"
    increment_by_value = <bool>
    name = "<string>"
    labels = {* = "<string>"}
```
{% endcode-tabs-item %}
{% endcode-tabs %}

## Options

| Key  | Type  | Description |
|:-----|:-----:|:------------|
| **REQUIRED** - General | | |
| `type` | `string` | The component type<br />`required` `enum: "log_to_metric"` |
| `inputs` | `[string]` | A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.config_composition] for more info.<br />`required` `example: ["my-source-id"]` |
| **REQUIRED** - Metrics | | |
| `metrics.type` | `string` | The metric type.<br />`required` `enum: "counter", "gauge"` |
| `metrics.field` | `string` | The log field to use as the metric. See [Null Fields](#null-fields) for more info.<br />`required` `example: "duration"` |
| `metrics.increment_by_value` | `bool` | If `true` the metric will be incremented by the `field` value. If `false` the metric will be incremented by 1 regardless of the `field` value.<br />`default: false` |
| `metrics.name` | `string` | The name of the metric. Defaults to `<field>_total` for `counter` and `<field>` for `gauge`.<br />`default: "dynamic"` |
| `metrics.labels.*` | `string` | Key/value pairs representing the metric labels.<br />`required` `example: (see above)` |

## Examples

{% tabs %}
{% tab title="Counting" %}
This example demonstrates counting HTTP status codes.

Given the following log line:

{% code-tabs %}
{% code-tabs-item title="log" %}
```json
{
  "host": "10.22.11.222",
  "message": "Sent 200 in 54.2ms",
  "status": 200
}
```
{% endcode-tabs-item %}
{% endcode-tabs %}

You can count the number of responses by status code:

{% code-tabs %}
{% code-tabs-item title="vector.toml" %}
```coffeescript
[transforms.log_to_metric]
  type = "log_to_metric"
  
  [[transforms.log_to_metric.metrics]]
    type = "counter"
    field = "status"
    name = "response_total"
    labels.status = "${event.status}" 
    labels.host = "${event.host}"
```
{% endcode-tabs-item %}
{% endcode-tabs %}

A [`metric` event][docs.metric_event] will be emitted with the following
structure:

```javascript
{
  "counter": {
    "name": "response_total",
    "val": 1.0,
    "labels": {
      "status": "200",
      "host": "10.22.11.222"
    }
  }
}
```

This metric will then proceed down the pipeline, and depending on the sink,
will be aggregated in Vector (ex: [prometheus][docs.prometheus_sink]) or will
be aggregated in the store itself.
{% endtab %}
{% tab title="Summing" %}
In this example we'll demonstrate computing a sum. The scenario we've chosen
is to compute the total of orders placed.

Given the following log line:

{% code-tabs %}
{% code-tabs-item title="log" %}
```json
{
  "host": "10.22.11.222",
  "message": "Order placed for $122.20",
  "total": 122.2
}
```
{% endcode-tabs-item %}
{% endcode-tabs %}

You can reduce this log into a `counter` metric that increases by the
field's value:

{% code-tabs %}
{% code-tabs-item title="vector.toml" %}
```coffeescript
[transforms.log_to_metric]
  type = "log_to_metric"
  
  [[transforms.log_to_metric.metrics]]
    type = "counter"
    field = "total"
    name = "order_total"
    increment_by_value = true
    labels.host = "${event.host}"
```
{% endcode-tabs-item %}
{% endcode-tabs %}

A [`metric` event][docs.metric_event] will be emitted with the following
structure:

```javascript
{
  "counter": {
    "name": "order_total",
    "val": 122.20,
    "labels": {
      "host": "10.22.11.222"
    }
  }
}
```

This metric will then proceed down the pipeline, and depending on the sink,
will be aggregated in Vector (ex: [prometheus][docs.prometheus_sink]) or will
be aggregated in the store itself.
{% endtab %}
{% tab title="Gauges" %}
In this example we'll demonstrate creating a gauge that represents the current
CPU load verages.

Given the following log line:

{% code-tabs %}
{% code-tabs-item title="log" %}
```json
{
  "host": "10.22.11.222",
  "message": "CPU activity sample",
  "1m_load_avg": 78.2,
  "5m_load_avg": 56.2,
  "15m_load_avg": 48.7
}
```
{% endcode-tabs-item %}
{% endcode-tabs %}

You can reduce this logs into multiple `gauge` metrics:

{% code-tabs %}
{% code-tabs-item title="vector.toml" %}
```coffeescript
[transforms.log_to_metric]
  type = "log_to_metric"
  
  [[transforms.log_to_metric.metrics]]
    type = "gauge"
    field = "1m_load_avg"
    labels.host = "${event.host}"

  [[transforms.log_to_metric.metrics]]
    type = "gauge"
    field = "5m_load_avg"
    labels.host = "${event.host}"

  [[transforms.log_to_metric.metrics]]
    type = "gauge"
    field = "15m_load_avg"
    labels.host = "${event.host}"
```
{% endcode-tabs-item %}
{% endcode-tabs %}

Multiple [`metric` events][docs.metric_event] will be emitted with the following
structure:

```javascript
[
  {
    "gauge": {
      "name": "1m_load_avg",
      "val": 78.2,
      "labels": {
        "host": "10.22.11.222"
      }
    }
  },
  {
    "gauge": {
      "name": "5m_load_avg",
      "val": 56.2,
      "labels": {
        "host": "10.22.11.222"
      }
    }
  },
  {
    "gauge": {
      "name": "15m_load_avg",
      "val": 48.7,
      "labels": {
        "host": "10.22.11.222"
      }
    }
  }
]
```

These metrics will then proceed down the pipeline, and depending on the sink,
will be aggregated in Vector (ex: [prometheus][docs.prometheus_sink]) or will
be aggregated in the store itself.
{% endtab %}
{% tab title="Timings" %}
{% hint style="info" %}
We are working on supporting timings and histograms. See
[issue 540][url.issue_540] for more info.
{% endhint %}
{% endtab %}
{% endtabs %}

## How It Works

### Environment Variables

Environment variables are supported through all of Vector's configuration.
Simply add `${MY_ENV_VAR}` in your Vector configuration file and the variable
will be replaced before being evaluated.

You can learn more in the [Environment Variables][docs.configuration.environment-variables]
section.

### Null Fields

If the target log `field` contains a `null` value it will ignored, and a metric
will not be emitted.

### Reducing

It's important to understand that this transform does not reduce multiple logs
into a single metric. Instead, this transform converts logs into granular
individual metrics that can then be reduced at the edge. Where the reduction
happens depends on your metrics storage. For example, the
[`prometheus` sink][docs.prometheus_sink] will reduce logs in the sink itself
for the next scrape, while other metrics sinks will proceed to forward the
individual metrics for reduction in the metrics storage itself.

## Troubleshooting

The best place to start with troubleshooting is to check the
[Vector logs][docs.monitoring_logs]. This is typically located at
`/var/log/vector.log`, then proceed to follow the
[Troubleshooting Guide][docs.troubleshooting].

If the [Troubleshooting Guide][docs.troubleshooting] does not resolve your
issue, please:

1. Check for any [open sink issues][url.log_to_metric_transform_issues].
2. [Search the forum][url.search_forum] for any similar issues.
2. Reach out to the [community][url.community] for help.


### Alternatives

Finally, consider the following alternatives:

* [`coercer` transform][docs.coercer_transform]

## Resources

* [**Issues**][url.log_to_metric_transform_issues] - [enhancements][url.log_to_metric_transform_enhancements] - [bugs][url.log_to_metric_transform_bugs]
* [**Source code**][url.log_to_metric_transform_source]


[docs.coercer_transform]: ../../../usage/configuration/transforms/coercer.md
[docs.config_composition]: ../../../usage/configuration/README.md#composition
[docs.configuration.environment-variables]: ../../../usage/configuration#environment-variables
[docs.log_event]: ../../../about/data-model.md#log
[docs.metric_event]: ../../../about/data-model.md#metric
[docs.monitoring_logs]: ../../../usage/administration/monitoring.md#logs
[docs.prometheus_sink]: ../../../usage/configuration/sinks/prometheus.md
[docs.sources]: ../../../usage/configuration/sources
[docs.transforms]: ../../../usage/configuration/transforms
[docs.troubleshooting]: ../../../usage/guides/troubleshooting.md
[images.log_to_metric_transform]: ../../../assets/log_to_metric-transform.svg
[url.community]: https://vector.dev/community
[url.issue_540]: https://github.com/timberio/vector/issues/540
[url.log_to_metric_transform_bugs]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Transform%3A+log_to_metric%22+label%3A%22Type%3A+Bug%22
[url.log_to_metric_transform_enhancements]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Transform%3A+log_to_metric%22+label%3A%22Type%3A+Enhancement%22
[url.log_to_metric_transform_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Transform%3A+log_to_metric%22
[url.log_to_metric_transform_source]: https://github.com/timberio/vector/tree/master/src/transforms/log_to_metric.rs
[url.search_forum]: https://forum.vector.dev/search?expanded=true
