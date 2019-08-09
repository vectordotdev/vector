---
description: 'A deeper look into Vector''s internal metric event.'
---

# Metric Event

![][images.data-model-metric]

{% hint style="warning" %}
The metric data model is in beta and still under development. We are working
to address core issues such as supporting [labels][url.issue_512].
{% endhint %}

As mentioned in the [data model page][docs.data-model], Vector's events must
be one of 2 types: a `log` or a `metric`. This page provides a deeper dive into
Vector's `metric` event type and how they flow through Vector internally.
Understanding this goes a long way in properly [configuring][docs.configuration]
Vector for your use case.

## Structure

Vector characterizes a `metric` event as a data structure that must be one of
a fixed set of types:

{% code-tabs %}
{% code-tabs-item title="metric.proto" %}
```coffeescript
message Metric {
  oneof metric {
    Counter counter = 1;
    Histogram histogram = 2;
    Gauge gauge = 3;
    Set set = 4;
  }
}

message Counter {
  string name = 1;
  double val = 2;
  google.protobuf.Timestamp timestamp = 3;
}

message Histogram {
  string name = 1;
  double val = 2;
  uint32 sample_rate = 3;
  google.protobuf.Timestamp timestamp = 4;
}

message Gauge {
  string name = 1;
  double val = 2;
  enum Direction {
    None = 0;
    Plus = 1;
    Minus = 2;
  }
  Direction direction = 3;
  google.protobuf.Timestamp timestamp = 4;
}

message Set {
  string name = 1;
  string val = 2;
  google.protobuf.Timestamp timestamp = 3;
}
```
{% endcode-tabs-item %}
{% code-tabs-item title="counter example" %}
```javascript
{
  "counter": {
    "name": "login.count",
    "val": 2.0,
    "timestamp": "2019-05-02T12:44:21.433184Z" // optional
  }
}
```
{% endcode-tabs-item %}
{% code-tabs-item title="histogram example" %}
```javascript
{
  "histogram": {
    "name": "duration_ms",
    "val": 2.0,
    "sample_rate": 1,
    "timestamp": "2019-05-02T12:44:21.433184Z" // optional
  }
}
```
{% endcode-tabs-item %}
{% endcode-tabs %}

You can view a complete definition in the [event proto \
definition][url.event_proto].

## Types

A vector metric must be one of the following types: `Counter`, `Histogram`,
`Gauge`, `Set`. Each are described below:

### Counter

A `counter` is a single value that can _only_ be incremented, it cannot be
decremented. Your downstream metrics [sink][docs.sinks] will receive this value
and aggregate appropriately.

| Name        | Type        | Description                       |
|:------------|:------------|:----------------------------------|
| `name`      | `string`    | Counter metric name.              |
| `val`       | `double`    | Amount to increment.              |
| `timestamp` | `timestamp` | Time metric was created/ingested. |

### Histogram

Also called a "timer". A `histogram` represents the frequency distribution of a
value. This is commonly used for timings, helping to understand quantiles, max,
min, and other aggregations.

Depending on the downstream service Vector will aggregate histograms internally
(such is the case for the [`prometheus` sink][docs.prometheus_sink]) or forward
them immediately to the service for aggregation.

| Name          | Type        | Description                                      |
|:--------------|:------------|:-------------------------------------------------|
| `name`        | `string`    | Histogram metric name.                           |
| `val`         | `double`    | Specific value.                                  |
| `sample_rate` | `int`       | The bucket/distribution the metric is a part of. |
| `timestamp`   | `timestamp` | Time metric was created/ingested.                |

### Gauge

A gauge represents a point-in-time value that can increase and decrease.
Vector's internal gauge type represents changes to that value. Gauges should be
used to track fluctuations in values, like current memory or CPU usage.

| Name        | Type        | Description                                                                   |
|:------------|:------------|:------------------------------------------------------------------------------|
| `name`      | `string`    | Histogram metric name.                                                        |
| `val`       | `double`    | Specific value.                                                               |
| `direction` | `string`    | The value direction. If it should increase or descrease the aggregated value. |
| `timestamp` | `timestamp` | Time metric was created/ingested.                                             |

### Set

A set represents a count of unique values. The `val` attribute below represents
that unique value.

| Name        | Type        | Description                       |
|:------------|:------------|:----------------------------------|
| `name`      | `string`    | Set metric name.                  |
| `val`       | `string`    | Specific value.                   |
| `timestamp` | `timestamp` | Time metric was created/ingested. |


[docs.configuration]: ../../usage/configuration
[docs.data-model]: ../../about/data-model
[docs.prometheus_sink]: ../../usage/configuration/sinks/prometheus.md
[docs.sinks]: ../../usage/configuration/sinks
[images.data-model-metric]: ../../assets/data-model-metric.svg
[url.event_proto]: https://github.com/timberio/vector/blob/master/proto/event.proto
[url.issue_512]: https://github.com/timberio/vector/issues/512
