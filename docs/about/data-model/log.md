---
description: 'A deeper look into Vector''s internal log event.'
---

# Log Event

![][images.data-model-log]

As mentioned in the [data model page][docs.data-model], Vector's events must
be one of 2 types: a `log` or a `metric`. This page provides a deeper dive into
Vector's `log` event type and how they flow through Vector internally.
Understanding this goes a long way in properly [configuring][docs.configuration]
Vector for your use case.

## Structure

Vector characterizes a `log` event as a _flat_ map of arbitrary attributes.
For example:

{% code-tabs %}
{% code-tabs-item title="log example" %}
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

While there are generally common keys (see [the default schema section below](#default-schema)),
Vector does not restrict you in any way as it relates to the structure of your
events; you are free to use any keys, with any names that suit you. This makes
Vector easy to integrate into existing environments, providing the ability to
improve your schema over time.

### Nested Keys

For simplicity and performance reasons, Vector represents nested keys with a
`.` delimiter. This means that when Vector ingests nested data, it will
flatten the keys and delimit hierarchies with a `.` character. Additionally,
when Vector outputs data it will explode the map back into it's original nested
structure.

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

And when this event is emitted from a [sink][docs.sinks], it will be exploded
back into it's original structure:

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

This makes it _much_ easier and faster to work with nested documents within
Vector.

### Special Characters

As described above in the [Nested Keys](#nested-keys) section only `.` is
treated as a special character to represent nesting.

### Default Schema

In all cases where a component must operate on a key, the following schema is
used as the default. Each component will provide configuration options to
override the keys used, if necessary.

| Name | Type | Description |
| :--- | :--- | :--- |
| `timestamp` | `string` | ISO 8601 timestamp representing when the log was generated. |
| `message` | `string` | String representation the log. This is the key used when ingesting data, and it is the default key used when parsing. |
| `host` | `string` | A string representing the originating host of the log. |

## Types

Externally, Vector supports all [JSON types][url.json_types] and
[TOML types][url.toml_types]. These types are mapped to Vector's internal
types which are described below.

### String

Strings are UTF8 compatible and are only bounded by the available system
memory.

### Int

Integers are signed integers up to 64 bits.

### Float

Floats are signed floats up to 64 bits.

### Boolean

Booleans represent binary true/false values.

### Timestamp

Timestamps are represented as [`DateTime` Rust structs][url.rust_date_time]
stored as UTC.

{% hint style="warning" %}
**A nete about timestamps without timezone information:**

If Vector receives a timestamp that does not contain timezone information
Vector assumes the timestamp is in local time, and will convert the timestamp
to UTC from the local time. It is important that the host system contain
time zone data files, typically installed through the `tzdata` package. See
[issue 551][url.issue_551] for more info.
{% endhint %}

### Array

For simplicity and performance reasons, Vector represents arrays with indexed
keys. This means that when Vector ingests arrays it will flatten the items
into keys containing the index. Additionally, when Vector outputs data it will
explode the array back into it's original array structure.

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

And when this event is emitted from a [sink][docs.sinks], it will be exploded
back into it's original structure:

{% code-tabs %}
{% code-tabs-item title="output" %}
```javascript
{
    "array": ["item1", "item2", "item3"]
}
```
{% endcode-tabs-item %}
{% endcode-tabs %}

This normalizes the event structure and simplifies data processing throughout
the Vector pipeline. This not only helps with performance, but it helps to
avoid type human error when configuring Vector.


[docs.configuration]: ../../usage/configuration
[docs.data-model]: ../../about/data-model
[docs.sinks]: ../../usage/configuration/sinks
[images.data-model-log]: ../../assets/data-model-log.svg
[url.issue_551]: https://github.com/timberio/vector/issues/551
[url.json_types]: https://en.wikipedia.org/wiki/JSON#Data_types_and_syntax
[url.rust_date_time]: https://docs.rs/chrono/0.4.0/chrono/struct.DateTime.html
[url.toml_types]: https://github.com/toml-lang/toml#table-of-contents
