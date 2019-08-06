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
[transforms.my_transform_id]
  # REQUIRED - General
  type = "log_to_metric" # must be: "log_to_metric"
  inputs = ["my-source-id"]
  
  # REQUIRED - Metrics
  [[transforms.my_transform_id.metrics]]
    type = "counter" # enum: "counter", "gauge", "histogram", and "set"
    field = "duration"
    name = "duration_total"
    
    increment_by_value = false # default, relevant when type = "counter"
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
    type = {"counter" | "gauge" | "histogram" | "set"}
    field = "<string>"
    name = "<string>"
    increment_by_value = <bool>
    labels = {* = "<string>"}
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (specification)" %}
```coffeescript
[transforms.log_to_metric_transform]
  #
  # General
  #

  # The component type
  # 
  # * required
  # * no default
  # * must be: "log_to_metric"
  type = "log_to_metric"

  # A list of upstream source or transform IDs. See Config Composition for more
  # info.
  # 
  # * required
  # * no default
  inputs = ["my-source-id"]

  #
  # Metrics
  #

  [[transforms.log_to_metric_transform.metrics]]
    # The metric type.
    # 
    # * required
    # * no default
    # * enum: "counter", "gauge", "histogram", and "set"
    type = "counter"
    type = "gauge"
    type = "histogram"
    type = "set"

    # The log field to use as the metric.
    # 
    # * required
    # * no default
    field = "duration"

    # If `true` the metric will be incremented by the `field` value. If `false` the
    # metric will be incremented by 1 regardless of the `field` value.
    # 
    # * optional
    # * default: false
    increment_by_value = false

    # The name of the metric. Defaults to `<field>_total` for `counter` and
    # `<field>` for `gauge`.
    # 
    # * required
    # * no default
    name = "duration_total"

    [transforms.log_to_metric_transform.metrics.labels]
      # Key/value pairs representing the metric labels.
      # 
      # * required
      # * no default
      host = "${HOSTNAME}"
      region = "us-east-1"
```
{% endcode-tabs-item %}
{% endcode-tabs %}

## Options

| Key  | Type  | Description |
|:-----|:-----:|:------------|
| **REQUIRED** - General | | |
| `type` | `string` | The component type<br />`required` `must be: "log_to_metric"` |
| `inputs` | `[string]` | A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.config_composition] for more info.<br />`required` `example: ["my-source-id"]` |
| **REQUIRED** - Metrics | | |
| `metrics.type` | `string` | The metric type.<br />`required` `enum: "counter", "gauge", "histogram", and "set"` |
| `metrics.field` | `string` | The log field to use as the metric. See [Null Fields](#null-fields) for more info.<br />`required` `example: "duration"` |
| `metrics.name` | `string` | The name of the metric. Defaults to `<field>_total` for `counter` and `<field>` for `gauge`.<br />`required` `example: "duration_total"` |
| `metrics.increment_by_value` | `bool` | If `true` the metric will be incremented by the `field` value. If `false` the metric will be incremented by 1 regardless of the `field` value. Only relevant when type = "counter"<br />`default: false` |
| `metrics.labels.*` | `string` | Key/value pairs representing the metric labels.<br />`required` `example: (see above)` |

## Examples

{% tabs %}
{% tab title="Timings" %}
This example demonstrates capturing timings in your logs.

{% code-tabs %}
{% code-tabs-item title="log" %}
```json
{
  "host": "10.22.11.222",
  "message": "Sent 200 in 54.2ms",
  "status": 200,
  "time": 54.2,
}
```
{% endcode-tabs-item %}
{% endcode-tabs %}

You can convert the `time` field into a `histogram` metric:

{% code-tabs %}
{% code-tabs-item title="vector.toml" %}
```coffeescript
[transforms.log_to_metric]
  type = "log_to_metric"
  
  [[transforms.log_to_metric.metrics]]
    type = "histogram"
    field = "time"
    name = "time_ms" # optional
    labels.status = "${event.status}" # optional
    labels.host = "${event.host}" # optional
```
{% endcode-tabs-item %}
{% endcode-tabs %}

A [`metric` event][docs.metric_event] will be emitted with the following
structure:

```javascript
{
  "histogram": {
    "name": "time_ms",
    "val": 52.2,
    "smaple_rate": 1,
    "labels": {
      "status": "200",
      "host": "10.22.11.222"
    }
  }
}
```

This metric will then proceed down the pipeline, and depending on the sink,
will be aggregated in Vector (such is the case for the [`prometheus` \
sink][docs.prometheus_sink]) or will be aggregated in the store itself.

{% endtab %}
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
    name = "response_total" # optional
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
will be aggregated in Vector (such is the case for the [`prometheus` \
sink][docs.prometheus_sink]) or will be aggregated in the store itself.
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
    name = "order_total" # optional
    increment_by_value = true # optional
    labels.host = "${event.host}" # optional
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
will be aggregated in Vector (such is the case for the [`prometheus` \
sink][docs.prometheus_sink]) or will be aggregated in the store itself.
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
    labels.host = "${event.host}" # optional

  [[transforms.log_to_metric.metrics]]
    type = "gauge"
    field = "5m_load_avg"
    labels.host = "${event.host}" # optional

  [[transforms.log_to_metric.metrics]]
    type = "gauge"
    field = "15m_load_avg"
    labels.host = "${event.host}" # optional
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

This metric will then proceed down the pipeline, and depending on the sink,
will be aggregated in Vector (such is the case for the [`prometheus` \
sink][docs.prometheus_sink]) or will be aggregated in the store itself.
{% endtab %}
{% tab title="Sets" %}
In this example we'll demonstrate how to use sets. Sets are primarly a Statsd
concept that represent the number of unique values seens for a given metric.
The idea is that you pass the unique/high-cardinality value as the metric value
and the metric store will count the number of unique values seen.

For example, given the following log line:

{% code-tabs %}
{% code-tabs-item title="log" %}
```json
{
  "host": "10.22.11.222",
  "message": "Sent 200 in 54.2ms",
  "remote_addr": "233.221.232.22"
}
```
{% endcode-tabs-item %}
{% endcode-tabs %}

You can count the number of unique `remote_addr` values by using a set:

{% code-tabs %}
{% code-tabs-item title="vector.toml" %}
```coffeescript
[transforms.log_to_metric]
  type = "log_to_metric"
  
  [[transforms.log_to_metric.metrics]]
    type = "set"
    field = "remote_addr"
    labels.host = "${event.host}" # optional
```
{% endcode-tabs-item %}
{% endcode-tabs %}

A [`metric` event][docs.metric_event] will be emitted with the following
structure:

```javascript
{
  "set": {
    "name": "remote_addr",
    "val": "233.221.232.22",
    "labels": {
      "host": "10.22.11.222"
    }
  }
}
```

This metric will then proceed down the pipeline, and depending on the sink,
will be aggregated in Vector (such is the case for the [`prometheus` \
sink][docs.prometheus_sink]) or will be aggregated in the store itself.
{% endtab %}
{% endtabs %}

## How It Works

### Environment Variables

Environment variables are supported through all of Vector's configuration.
Simply add `${MY_ENV_VAR}` in your Vector configuration file and the variable
will be replaced before being evaluated.

You can learn more in the [Environment Variables][docs.configuration.environment-variables]
section.

### Multiple Metrics

For clarification, when you convert a single `log` event into multiple `metric`
events, the `metric` events are not emitted as a single array. They are emitted
individually, and the downstream components treat them as individual events.
Downstream components are not aware they were derived from a single log event.

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

1. Check for any [open `log_to_metric_transform` issues][url.log_to_metric_transform_issues].
2. If encountered a bug, please [file a bug report][url.new_log_to_metric_transform_bug].
3. If encountered a missing feature, please [file a feature request][url.new_log_to_metric_transform_enhancement].
4. If you need help, [join our chat/forum community][url.vector_chat]. You can post a question and search previous questions.

## Resources

* [**Issues**][url.log_to_metric_transform_issues] - [enhancements][url.log_to_metric_transform_enhancements] - [bugs][url.log_to_metric_transform_bugs]
* [**Source code**][url.log_to_metric_transform_source]


[docs.config_composition]: ../../../usage/configuration/README.md#composition
[docs.configuration.environment-variables]: ../../../usage/configuration#environment-variables
[docs.log_event]: ../../../about/data-model/log.md
[docs.metric_event]: ../../../about/data-model/metric.md
[docs.monitoring_logs]: ../../../usage/administration/monitoring.md#logs
[docs.prometheus_sink]: ../../../usage/configuration/sinks/prometheus.md
[docs.sources]: ../../../usage/configuration/sources
[docs.transforms]: ../../../usage/configuration/transforms
[docs.troubleshooting]: ../../../usage/guides/troubleshooting.md
[images.log_to_metric_transform]: ../../../assets/log_to_metric-transform.svg
[url.log_to_metric_transform_bugs]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Transform%3A+log_to_metric%22+label%3A%22Type%3A+Bug%22
[url.log_to_metric_transform_enhancements]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Transform%3A+log_to_metric%22+label%3A%22Type%3A+Enhancement%22
[url.log_to_metric_transform_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Transform%3A+log_to_metric%22
[url.log_to_metric_transform_source]: https://github.com/timberio/vector/tree/master/src/transforms/log_to_metric.rs
[url.new_log_to_metric_transform_bug]: https://github.com/timberio/vector/issues/new?labels=Transform%3A+log_to_metric&labels=Type%3A+Bug
[url.new_log_to_metric_transform_enhancement]: https://github.com/timberio/vector/issues/new?labels=Transform%3A+log_to_metric&labels=Type%3A+Enhancement
[url.vector_chat]: https://chat.vector.dev
