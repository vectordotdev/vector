---
description: 'A deeper look into Vector''s internal log event data model.'
---

# Log Event

![][assets.data-model-log]

As mentioned in the [data model page][docs.data-model], Vector's events must
be one of 2 types: a `log` or a `metric`. This page provides a deeper dive
into Vector's `log` event type. Understanding this goes a long way in properly
[configuring][docs.configuration] Vector for your use case.

## Schema

Vector characterizes a `log` event as a _flat_ map of fields:

{% code-tabs %}
{% code-tabs-item title="log.proto" %}
```coffeescript
message Log {
  map<string, Value> fields = 1;
}

message Value {
  oneof kind {
    bytes raw_bytes = 1;
    google.protobuf.Timestamp timestamp = 2;
    int64 integer = 4;
    double float = 5;
    bool boolean = 6;
  }
  bool explicit = 3;
}
```
{% endcode-tabs-item %}
{% endcode-tabs %}

You can view a complete definition in the [event proto \
definition][urls.event_proto].

You'll notice that Vector does not restrict your schema in any way, you are
free to use whatever fields and shape you like. In places where Vector must
operate on a field, Vector will default to the [default schema](#default-schema)
and provide options to specify custom field names.

### Fields

A "field" represents a [key](#keys)/[value](#values) pair and a `log` event is
comprised of many fields. 

#### Keys

Keys are `string` representations of the field name.

##### Special Characters

`.` is used to denote [field nesting](#nested-fields) and `[`/`]` are used
to denote [arrays](#arrays).

#### Values

A field must contains a value of one of the following types.

##### Strings

Strings are UTF8 compatible and are only bounded by the available system
memory.

##### Ints

Integers are signed integers up to 64 bits.

##### Floats

Floats are signed floats up to 64 bits.

##### Booleans

Booleans represent binary true/false values.

##### Timestamps

Timestamps are represented as [`DateTime` Rust structs][urls.rust_date_time]
stored as UTC.

{% hint style="warning" %}
**A note about timestamps without timezone information:**

If Vector receives a timestamp that does not contain timezone information
Vector assumes the timestamp is in local time, and will convert the timestamp
to UTC from the local time. It is important that the host system contain
time zone data files to properly determine the local time zone. This is
typically installed through the `tzdata` package. See [issue 551][urls.issue_551]
for more info.
{% endhint %}

### Nested fields

For simplicity and performance reasons, Vector represents nested fields with a
`.` delimiter. This means that when Vector ingests nested fields, it will
flatten the fields and delimit hierarchies with a `.` character. Additionally,
when Vector outputs data it will explode the map back into it's original nested
structure.

For example, if Vector ingests the following JSON data:

{% code-tabs %}
{% code-tabs-item title="input.json" %}
```javascript
{
    "parent": {
        "child": "..."
    }
}
```
{% endcode-tabs-item %}
{% endcode-tabs %}

Vector will represent this data internally as a flattened map:

{% code-tabs %}
{% code-tabs-item title="internal.json" %}
```javascript
{
    "parent.child": "..."
}
```
{% endcode-tabs-item %}
{% endcode-tabs %}

And when this internal log is emitted from a [sink][docs.sinks], it will be
exploded back into it's original structure:

{% code-tabs %}
{% code-tabs-item title="output.json" %}
```javascript
{
    "parent": {
        "child": "..."
    }
}
```
{% endcode-tabs-item %}
{% endcode-tabs %}

This makes it _much_ easier to access and operate on nested fields in Vector's
[transforms][docs.transforms].

### Arrays

For simplicity and performance reasons, Vector represents arrays with indexed
fields. This means that when Vector ingests arrays it will flatten the items
into fields containing the index. Additionally, when Vector outputs data it will
explode the array back into it's original array structure.

For example, if Vector ingests the following data:

{% code-tabs %}
{% code-tabs-item title="input.json" %}
```javascript
{
    "array": ["item1", "item2", "item3"]
}
```
{% endcode-tabs-item %}
{% endcode-tabs %}

Vector will represent this data internally as a log event:

{% code-tabs %}
{% code-tabs-item title="internal.json" %}
```javascript
{
    "array[0]": "item1",
    "array[1]": "item2",
    "array[2]": "item3"
}
```
{% endcode-tabs-item %}
{% endcode-tabs %}

And when this event is emitted from a [sink][docs.sinks], it will be exploded
back into it's original structure:

{% code-tabs %}
{% code-tabs-item title="output.json" %}
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

If vector receives flattened array items that contain a missing index during the 
unflatten process it will insert `null` values. For example:

{% code-tabs %}
{% code-tabs-item title="internal.json" %}
```javascript
{
    "array[0]": "item1",
    "array[2]": "item3"
}
```
{% endcode-tabs-item %}
{% endcode-tabs %}

The output will contain a `null` value for `array[1]` like so:

{% code-tabs %}
{% code-tabs-item title="output.json" %}
```javascript
{
    "array": ["item1", null, "item3"]
}
```
{% endcode-tabs-item %}
{% endcode-tabs %}

## Default Schema

In all cases where a component must operate on a key, the following schema is
used as the _default_. Each component will provide configuration options to
override the fields used, if relevant.

| Name        | Type                      | Description                                                                                                                                                                     |
|:------------|:--------------------------|:--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `timestamp` | [`timestamp`](#timestamp) | A normalized [Rust DateTime struct][urls.rust_date_time] in UTC.                                                                                                                 |
| `message`   | [`string`](#string)       | A string representing the log message. This is the key used when ingesting raw string data.                                                                                     |
| `host`      | [`string`](#string)       | A string representing the originating host of the log. This is commonly used in [sources][docs.sources] but can be overridden via the `host_field` option for relevant sources. |

### Deviating from the default schema

As mentioned in the [schema](#schema) section, Vector does not require any
specific fields. You are free to use [transforms][docs.transforms] to add,
remove, or rename fields as desired.

## Examples

{% code-tabs %}
{% code-tabs-item title="log.json" %}
```javascript
{
    "timestamp": "2019-05-02T00:23:22Z",
    "parent.child": "...",
    "message": "message",
    "host": "my.host.com",
    "key": "value",
    "parent.child": "value"
}
```
{% endcode-tabs-item %}
{% endcode-tabs %}

[assets.data-model-log]: ../../assets/data-model-log.svg
[docs.configuration]: ../../usage/configuration
[docs.data-model]: ../../about/data-model
[docs.sinks]: ../../usage/configuration/sinks
[docs.sources]: ../../usage/configuration/sources
[docs.transforms]: ../../usage/configuration/transforms
[urls.event_proto]: https://github.com/timberio/vector/blob/master/proto/event.proto
[urls.issue_551]: https://github.com/timberio/vector/issues/551
[urls.rust_date_time]: https://docs.rs/chrono/0.4.0/chrono/struct.DateTime.html
