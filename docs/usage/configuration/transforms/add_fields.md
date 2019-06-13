---
description: Add one or more fields to a log or metric event
---

# add\_fields transform

![](../../../.gitbook/assets/add-fields-transformer.svg)

The `add_fields` transform allows you to add one or more fields to a [`log`](../../../about/data-model.md#log) or [`metric`](../../../about/data-model.md#metric) event.

## Example

{% code-tabs %}
{% code-tabs-item title="vector.toml" %}
```coffeescript
[transforms.<transform-id>]
    # REQUIRED
    inputs = ["{<source-id> | <transform-id>}", [ ... ]]
    type   = "add_fields"
    
    [transforms.<transform-id>.fields]
        <key-name> = {"<string>" | <int> | <float> | <boolean>}
        # ...
```
{% endcode-tabs-item %}
{% endcode-tabs %}

## Options

| Key | Type | Description |
| :--- | :---: | :--- |
| **Required** |  |  |
| `fields` | `table` | A table of key/value pairs representing the keys to be added to the event. |

## Input

Both [`log`](../../../about/data-model.md#log) and [`metric`](../../../about/data-model.md#metric) events are accepted as input.

## Output

The output type coincides with the input type.

## How It Works

### Complex Transforming

The `add_fields` transform is designed for simple key additions. If you need more complex transforming then we recommend using a more versatile transform like the [`lua` transform](lua.md).

### Environment Variables

As described in the [Configuration document](../#environment-variables), Vector will interpolate environment variables in your configuration file. This can be helping when adding fields, such as adding a `"host"` field:

{% code-tabs %}
{% code-tabs-item title="vector.toml" %}
```coffeescript
[transforms.<transform-id>]
    # REQUIRED
    type = "add_fields"
    
    [transforms.<transform-id>.fields]
        host = "${HOSTNAME}"
```
{% endcode-tabs-item %}
{% endcode-tabs %}

### Special Characters

Vector does not restrict the characters allowed in keys. You can wrap key names in `" "` characters to preserve spaces and use `\` to escape quotes.

#### Special characters

As described in the [Nested Fields](add_fields.md#nested-fields) section, as well as the [Data Model](../../../about/data-model.md) document, the `.` character is used to denote nesting. This is the only special character within key names.

### Key Conflicts

Keys specified in this transform will replace existing keys.

### Nested Fields

As described in the [Data Model document](../../../about/data-model.md), Vector represents [events](../../../about/concepts.md#events) as flat maps, and nested fields are denoted by a `.` character in the key name. Therefore, adding nested fields is as simple as adding a `.` to your key name:

{% code-tabs %}
{% code-tabs-item title="vector.toml" %}
```coffeescript
[transforms.<transform-id>]
    # REQUIRED
    type = "add_fields"
    
    [transforms.<transform-id>.fields]
        "parent.child.grandchild" = "<value>"
```
{% endcode-tabs-item %}
{% endcode-tabs %}

Please see the [Nested Keys section](../../../about/data-model.md#nested-keys) in the [Data Model document](../../../about/data-model.md) for a more detailed description.

### Removing Fields

See the [`remove_fields` transform](remove_fields.md).

### Value Types

All [supported configuration value types](../#value-types) are accepted.

## Resources

* [Source code](https://github.com/timberio/vector/blob/master/src/transforms/add_fields.rs)
* [Issues](https://github.com/timberio/vector/labels/Transform%3A%20Add%20Fields)

### 



