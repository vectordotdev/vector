---
description: 'A deeper look at Vector''s data model: the event'
---

# Data Model

![](../.gitbook/assets/data-model.svg)

This page provides a deeper dive into Vector's data model and how data flows through Vector internally. Understanding this goes a long way in properly [configuring](../usage/configuration/) Vector for your use case.

## Event

An "event" represents an individual unit of data that flows through Vector. An event must be one of two types: a [`log`](data-model.md#log) or a [`metric`](data-model.md#metric). Each are described in more detail below.

### Log

Vector characterizes a "log" as an arbitrary flat map. For example:

{% code-tabs %}
{% code-tabs-item title="log event" %}
```javascript
{
    "timestamp": <timestamp:2019-05-02T00:23:22Z>,
    "message": "message"
    "host": "my.host.com",
    "key": "value",
    "parent.child": "value"
}
```
{% endcode-tabs-item %}
{% endcode-tabs %}

Vector takes a completely unopinionated approach to the structure of your events. This allows Vector to work with any schema. This is described in more detail in the [Default Schema](#default-schema) section.

#### Types

Log events support the following value types:

1. `string`
2. `int`
3. `float`
4. `bool`
5. `timestamp`

#### Arrays

For simplicity and performance reasons, Vector represents arrays with indexed keys. This means that when Vector ingests arrays it will flatten the items into keys containing the index. Additionally, when Vector outputs data it will explode the array back into it's original array structure.

For example, if Vector ingests the following data:

{% code-tabs %}
{% code-tabs-item title="input" %}
```javascript
{
    "array": ["item1", "item2", "item3"]
}
```
{% endcode-tabs-item %}
{% endcode-tabs %}

Vector will represent this data internally as a log event:

{% code-tabs %}
{% code-tabs-item title="log event" %}
```javascript
{
    "array.0": "item1",
    "array.1": "item2",
    "array.2": "item3"
}
```
{% endcode-tabs-item %}
{% endcode-tabs %}

And when this event is emitted from a [sink][sinks], it will be exploded back into it's original structure:

{% code-tabs %}
{% code-tabs-item title="output" %}
```javascript
{
    "array": ["item1", "item2", "item3"]
}
```
{% endcode-tabs-item %}
{% endcode-tabs %}

This normalizes the event structure and simplifies data processing throughoout the Vector pipeline. This not only helps with performance, but it helps to avoid type human error when configuring Vector.

#### Nested Keys

For simplicity and performance reasons, Vector represents nested keys with a `.` delimiter. This means that when Vector ingests nested data, it will flatten the keys and delimit hierarchies with a `.` character. Additionally, when Vector outputs data it will explode the map back into it's original nested structure.

For example, if Vector ingests the following data:

{% code-tabs %}
{% code-tabs-item title="input" %}
```javascript
{
    "parent": {
        "child": "..."
    }
}
```
{% endcode-tabs-item %}
{% endcode-tabs %}

Vector will represent this data internally as a log event:

{% code-tabs %}
{% code-tabs-item title="log event" %}
```javascript
{
    "parent.child": "..."
}
```
{% endcode-tabs-item %}
{% endcode-tabs %}

And when this event is emitted from a [sink][sinks], it will be exploded back into it's original structure:

{% code-tabs %}
{% code-tabs-item title="output" %}
```javascript
{
    "parent": {
        "child": "..."
    }
}
```
{% endcode-tabs-item %}
{% endcode-tabs %}

This makes it _much_ easier and faster to work with nested documents within Vector.

#### Special Characters

As described above in the [Nested Keys](#nested-keys) section only `.` is trated as a special character to represent nesting.

#### Default Schema

In all cases where a component must operate on a key, the following schema is used as the default. Each component will provide configuration options to override the keys used, if necessary.

| Name | Type | Description |
| :--- | :--- | :--- |
| `timestamp` | `string` | ISO 8601 timestamp representing when the log was generated. |
| `message` | `string` | String representation the log. This is the key used when ingesting data, and it is the default key used when parsing. |
| `host` | `string` | A string representing the originating host of the log. |

### Metric

{% hint style="warn" %}
The metric data model is in beta and still under development. We are working to address core issues such as supporting [ histograms](https://github.com/timberio/vector/issues/384) and [labels](https://github.com/timberio/vector/issues/512).
{% endhint %}

Vector characterizes a "metric" as an individual measurement with any number of labels. The measure must either be a `counter`, `guage`, `set`, or `timer`.

{% hint style="info" %}
Histgorams are not yet available. See [Issue #384](https://github.com/timberio/vector/issues/384) for more info.
{% endhint %}

For example:

{% code-tabs %}
{% code-tabs-item title="counter" %}
```javascript
{
  "counter": {
    "name": "login.invocations",
    "val": 1
  }
}
```
{% endcode-tabs-item %}
{% code-tabs-item title="gauge" %}
```javascript
{
  "guage": {
    "name": "gas_tank",
    "val": 0.5
  }
}
```
{% endcode-tabs-item %}
{% code-tabs-item title="set" %}
```javascript
{
  "set": {
    "name": "unique_users",
    "val": 1
  }
}
```
{% endcode-tabs-item %}
{% code-tabs-item title="timer" %}
```javascript
{
  "timer": {
    "name": "login.time",
    "val": 22
  }
}
```
{% endcode-tabs-item %}
{% endcode-tabs %}
