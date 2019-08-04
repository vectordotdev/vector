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

Vector characterizes a `Metric` event as a defined set of types:

{% code-tabs %}
{% code-tabs-item title="definition" %}
```rust
pub enum Metric {
    Counter {
        name: String,
        val: f32,
    },
    Histogram {
        name: String,
        val: f32,
        sample_rate: u32,
    },
    Gauge {
        name: String,
        val: f32,
        direction: Option<Direction>,
    },
    Set {
        name: String,
        val: String,
    },
}
```
{% endcode-tabs-item %}
{% code-tabs-item title="example" %}
```rust
Metric {
  Counter {
    name: "login.count",
    val: 2.0
  }
}
```
{% endcode-tabs-item %}
{% endcode-tabs %}

You can view a complete definition in the
[metric event source file][url.metric_event_source].

## Types

A vector metric must be one of the following types: `Counter`, `Gauge`,
`Histogram`, `Set`. Each are described below:

### Counter

A counter is a single value that can _only_ be incremented, it cannot be
decremented. For example:

{% code-tabs %}
{% code-tabs-item title="example" %}
```javascript
{
  "counter": {
    "name": "login.invocations",
    "val": 2.0
  }
}
```
{% endcode-tabs-item %}
{% code-tabs-item title="schema" %}
```javascript
{
  "counter": {
    "name": <string>,
    "val": <float> // 32 bit
  }
}
```
{% endcode-tabs-item %}
{% endcode-tabs %}

Your downstream metrics [sink][docs.sinks] will receive this value and
increment appropriately.

### Gauge

A gauge represents a point-in-time value that can increase and decrease.
Vector's internal gauge type represents changes to that value:

{% code-tabs %}
{% code-tabs-item title="example" %}
```javascript
{
  "counter": {
    "name": "memory_rss",
    "val": 222443.0,
    "direction": "plus"
  }
}
```
{% endcode-tabs-item %}
{% code-tabs-item title="schema" %}
```javascript
{
  "counter": {
    "name": "<string>",
    "val": <float> // 32 bit,
    "direction": {"plus"|"minus"}
  }
}
```
{% endcode-tabs-item %}
{% endcode-tabs %}

Gauges should be used to track fluctuations in values, like current memory or
CPU usage.

### Histogram

Also called a "timer". A histogram represents the frequency distribution of a
value. This is commonly used for timings, helping to understand quantiles, max,
min, and other aggregations.

{% code-tabs %}
{% code-tabs-item title="example" %}
```javascript
{
  "histogram": {
    "name": "memory_rss",
    "val": 201.0,
    "sample_rate": 1
  }
}
```
{% endcode-tabs-item %}
{% code-tabs-item title="schema" %}
```javascript
{
  "counter": {
    "name": "<string>",
    "val": <float> // 32 bit,
    "sample_rate": <int> // 32 bit
  }
}
```
{% endcode-tabs-item %}
{% endcode-tabs %}

Depending on the downstream service Vector will aggregate histograms internally
(such is the case for the [`prometheus` sink][docs.prometheus_sink]) or forward
them immediately to the service for aggregation.

### Set

A set represents a count of unique values. The `val` attribute below represents
that unique value.

{% code-tabs %}
{% code-tabs-item title="example" %}
```javascript
{
  "counter": {
    "name": "memory_rss",
    "val": "my_unique_value"
  }
}
```
{% endcode-tabs-item %}
{% code-tabs-item title="schema" %}
```javascript
{
  "counter": {
    "name": "<string>",
    "val": "<string>"
  }
}
```
{% endcode-tabs-item %}
{% endcode-tabs %}


[docs.configuration]: ../../usage/configuration
[docs.data-model]: ../../about/data-model
[docs.prometheus_sink]: ../../usage/configuration/sinks/prometheus.md
[docs.sinks]: ../../usage/configuration/sinks
[images.data-model-metric]: ../../assets/data-model-metric.svg
[url.issue_512]: https://github.com/timberio/vector/issues/512
[url.metric_event_source]: https://github.com/timberio/vector/blob/master/src/event/metric.rs
