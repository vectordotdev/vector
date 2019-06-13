---
description: Remove one or more fields from a log or metric event
---

# remove\_fields transform

The `remove_fields` transform allows you to remove one or more fields from a [`log`](../../../about/data-model.md#log) or [`metric`](../../../about/data-model.md#metric) event. Test.

## Example

{% code-tabs %}
{% code-tabs-item title="vector.toml" %}
```coffeescript
[transforms.<transform-id>]
    # REQUIRED
    inputs = ["{<source-id> | <transform-id>}", [ ... ]]
    type   = "remove_fields"
    fields = ["<key-name>", [ ... ]]
```
{% endcode-tabs-item %}
{% endcode-tabs %}

## Options

| Key | Type | Description |
| :--- | :---: | :--- |
| **Required** |  |  |
| `fields` | `[string]` | An array of string key names. |

## Input

Both [`log`](../../../about/data-model.md#log) and [`metric`](../../../about/data-model.md#metric) events are accepted as input.

## Output

The output type coincides with the input type.

## How It Works

### Adding Fields

See the [`add_fields` transform](add_fields.md).

### Complex Transforming

The `remove_fields` transform is designed for simple key deletions. If you need more complex transforming then we recommend using a more versatile transform like the [`lua` transform](lua.md).

### Special Characters

Vector does not restrict the characters allowed in keys. You can wrap key names in `" "` characters to preserve spaces and use `\` to escape quotes.

#### Special characters

As described in the [Nested Fields](add_fields.md#nested-fields) section, as well as the [Data Model](../../../about/data-model.md) document, the `.` character is used to denote nesting. This is the only special character within key names.

### Nested Fields

As described in the [Data Model document](../../../about/data-model.md), Vector represents [events](../../../about/concepts.md#events) as flat maps, and nested fields are denoted by a `.` character in the key name. Therefore, removing nested fields is as simple as adding a `.` to your key name:

{% code-tabs %}
{% code-tabs-item title="vector.toml" %}
```coffeescript
[transforms.<transform-id>]
    # REQUIRED
    type = "remove_fields"
    fields = ["parent.child.grandchild"]
```
{% endcode-tabs-item %}
{% endcode-tabs %}

Please see the [Nested Keys section](../../../about/data-model.md#nested-keys) in the [Data Model document](../../../about/data-model.md) for a more detailed description.

## Resources

* [Source code](https://github.com/timberio/vector/blob/master/src/transforms/remove_fields.rs)
* [Issues](https://github.com/timberio/vector/labels/Transform%3A%20Remove%20Fields)

