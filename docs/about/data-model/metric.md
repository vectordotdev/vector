---
description: 'A deeper look into Vector''s internal metric event data model.'
---

# Metric Event

![][assets.data-model-metric]

As mentioned in the [data model page][docs.data-model], Vector's events must
be one of 2 types: a `log` or a `metric`. This page provides a deeper dive into
Vector's `metric` event type. Understanding this goes a long way in properly
[configuring][docs.configuration] Vector for your use case.

## Schema

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
  map<string, string> tags = 4;
}

message Histogram {
  string name = 1;
  double val = 2;
  uint32 sample_rate = 3;
  google.protobuf.Timestamp timestamp = 4;
  map<string, string> tags = 5;
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
  map<string, string> tags = 5;
}

message Set {
  string name = 1;
  string val = 2;
  google.protobuf.Timestamp timestamp = 3;
  map<string, string> tags = 4;
}
```
{% endcode-tabs-item %}
{% endcode-tabs %}

You can view a complete definition in the [event proto \
definition][urls.event_proto].

### Counters

A `counter` is a single value that can _only_ be incremented, it cannot be
decremented. Your downstream metrics [sink][docs.sinks] will receive this value
and aggregate appropriately.

| Name        | Type        | Description                       |
|:------------|:------------|:----------------------------------|
| `name`      | `string`    | Counter metric name.              |
| `val`       | `double`    | Amount to increment.              |
| `timestamp` | `timestamp` | Time metric was created/ingested. |

### Histograms

Also called a "timer". A `histogram` represents the frequency distribution of a
value. This is commonly used for timings, helping to understand quantiles, max,
min, and other aggregations.

Depending on the downstream service Vector will aggregate histograms internally
(such is the case for the [`prometheus` sink][docs.sinks.prometheus]) or forward
them immediately to the service for aggregation.

| Name          | Type        | Description                                      |
|:--------------|:------------|:-------------------------------------------------|
| `name`        | `string`    | Histogram metric name.                           |
| `val`         | `double`    | Specific value.                                  |
| `sample_rate` | `int`       | The bucket/distribution the metric is a part of. |
| `timestamp`   | `timestamp` | Time metric was created/ingested.                |

### Gauges

A gauge represents a point-in-time value that can increase and decrease.
Vector's internal gauge type represents changes to that value. Gauges should be
used to track fluctuations in values, like current memory or CPU usage.

| Name        | Type        | Description                                                                   |
|:------------|:------------|:------------------------------------------------------------------------------|
| `name`      | `string`    | Histogram metric name.                                                        |
| `val`       | `double`    | Specific value.                                                               |
| `direction` | `string`    | The value direction. If it should increase or descrease the aggregated value. |
| `timestamp` | `timestamp` | Time metric was created/ingested.                                             |

### Sets

A set represents a count of unique values. The `val` attribute below represents
that unique value.

| Name        | Type        | Description                       |
|:------------|:------------|:----------------------------------|
| `name`      | `string`    | Set metric name.                  |
| `val`       | `string`    | Specific value.                   |
| `timestamp` | `timestamp` | Time metric was created/ingested. |

### Tags

You'll notice that each metric type contains a `tags` key. Tags are simple
key/value pairs represented as single-level strings.

## Examples

{% code-tabs %}
{% code-tabs-item title="counter.json" %}
```javascript
{
  "counter": {
    "name": "login.count",
    "val": 2.0,
    "timestamp": "2019-05-02T12:44:21.433184Z", // optional
    "tags": {                                   // optional
      "host": "my.host.com"
    }
  }
}
```
{% endcode-tabs-item %}
{% code-tabs-item title="histogram.json" %}
```javascript
{
  "histogram": {
    "name": "duration_ms",
    "val": 2.0,
    "sample_rate": 1,
    "timestamp": "2019-05-02T12:44:21.433184Z", // optional
    "tags": {                                   // optional
      "host": "my.host.com"
    }
  }
}
```
{% endcode-tabs-item %}
{% code-tabs-item title="gauge.json" %}
```javascript
{
  "gauge": {
    "name": "memory_rss",
    "val": 554222.0,
    "direction": "plus",
    "tags": {                // optional
      "host": "my.host.com"
    }
  }
}
```
{% endcode-tabs-item %}
{% code-tabs-item title="set.json" %}
```javascript
{
  "set": {
    "name": "unique_users",
    "val": "paul_bunyan",
    "tags": {                // optional
      "host": "my.host.com"
    }
  }
}
```
{% endcode-tabs-item %}
{% endcode-tabs}


[assets.data-model-metric]: ../../assets/data-model-metric.svg
[docs.configuration]: ../../usage/configuration
[docs.data-model]: ../../about/data-model
[docs.sinks.prometheus]: ../../usage/configuration/sinks/prometheus.md
[docs.sinks]: ../../usage/configuration/sinks
[urls.event_proto]: https://github.com/timberio/vector/blob/master/proto/event.proto
