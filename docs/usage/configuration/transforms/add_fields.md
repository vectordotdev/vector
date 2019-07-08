---
description: Accepts `log` events and allows you to add one or more fields.
---

<!--
     THIS FILE IS AUTOOGENERATED!

     To make changes please edit the template located at:

     scripts/generate/templates/docs/usage/configuration/transforms/add_fields.md.erb
-->

# add_fields transform

![][images.add_fields_transform]


The `add_fields` transform accepts [`log`][docs.log_event] events and allows you to add one or more fields.

## Config File

{% code-tabs %}
{% code-tabs-item title="vector.toml (example)" %}
```coffeescript
[sinks.my_add_fields_transform_id]
  # REQUIRED - General
  type = "add_fields" # must be: "add_fields"
  inputs = ["my-source-id"]
  
  # REQUIRED - Fields
  [sinks.my_add_fields_transform_id.fields]
    # OPTIONAL
  my_string_field = "string value"
  my_env_var_field = "${ENV_VAR}"
  my_int_field = 1
  my_float_field = 1.2
  my_bool_field = true
  my_timestamp_field = 1979-05-27T00:32:00.999998-07:00
  my_nested_fields = {key1 = "value1", key2 = "value2"}
  my_list = ["first", "second", "third"]
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (schema)" %}
```coffeescript
[sinks.<sink-id>]
  # REQUIRED - General
  type = "add_fields"
  inputs = ["<string>", ...]

  # REQUIRED - Fields
  [sinks.<sink-id>.fields]
    * = {"<string>" | <int> | <float> | <bool> | <timestamp>}
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (specification)" %}
```coffeescript
[sinks.add_fields]
  #
  # General
  #

  # The component type
  # 
  # * required
  # * no default
  # * must be: "add_fields"
  type = "add_fields"

  # A list of upstream source or transform IDs. See Config Composition for more
  # info.
  # 
  # * required
  # * no default
  inputs = ["my-source-id"]

  #
  # Fields
  #

  [sinks.add_fields.fields]
    # A key/value pair representing the new field to be added. Accepts all
    # supported types. Use `.` for adding nested fields.
    # 
    # * optional
    # * no default
    my_string_field = "string value"
    my_env_var_field = "${ENV_VAR}"
    my_int_field = 1
    my_float_field = 1.2
    my_bool_field = true
    my_timestamp_field = 1979-05-27T00:32:00.999998-07:00
    my_nested_fields = {key1 = "value1", key2 = "value2"}
    my_list = ["first", "second", "third"]
```
{% endcode-tabs-item %}
{% endcode-tabs %}

## Options

| Key  | Type  | Description |
|:-----|:-----:|:------------|
| **REQUIRED** - General | | |
| `type` | `string` | The component type<br />`required` `enum: "add_fields"` |
| `inputs` | `[string]` | A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.config_composition] for more info.<br />`required` `example: ["my-source-id"]` |
| **REQUIRED** - Fields | | |
| `fields.*` | `*` | A key/value pair representing the new field to be added. Accepts all [supported types][docs.config_value_types]. Use `.` for adding nested fields.<br />`no default` `example: (see above)` |

## Examples

Given the following configuration:

{% code-tabs %}
{% code-tabs-item title="vector.toml" %}
```coffeescript
[transforms.my_transform]
  type = "add_fields"
  inputs = [...]

  [transforms.my_transform.fields]
    field1 = "string value"
    field2 = 1
    field3 = 2.0
    field4 = true
    field5 = 2019-05-27T07:32:00Z
    field6 = ["item 1", "item 2"]
    field7.nested = "nested value",
    field8 = "#{HOSTNAME}"
```
{% endcode-tabs-item %}
{% endcode-tabs %}

A [`log` event][docs.log_event] will be emitted with the following structure:

{% code-tabs %}
{% code-tabs-item title="log" %}
```javascript
{
  // ... existing fields
  "field1": "string value",
  "field2": 1,
  "field3": 2.0,
  "field4": true,
  "field5": <timestamp:2019-05-27T07:32:00Z>,
  "field6": ["item1", "item2"],
  "field7.nested": "nested value",
  "field8": "my.hostname.com"
}
```
{% endcode-tabs-item %}
{% endcode-tabs %}

While unrealistic, this example demonstrates the various accepted [types][docs.config_value_types].

## How It Works



### Arrays

The `add_fields` transform will support [TOML arrays][url.toml_array]. Keep in mind that the values must be simple type (not tables), and each value must the same type. You cannot mix types:

```
[transforms.<transform-id>]
  # ...
  
  [transforms.<transform-id>.fields]
    my_array = ["first", "second", "third"]
```

### Complex Transforming

The `add_fields` transform is designed for simple key additions. If you need more complex transforming then we recommend using a more versatile transform like the [`lua` transform][docs.lua_transform].

### Environment Variables

As described in the [Configuration document][docs.configuration], Vector will interpolate environment variables in your configuration file. This can be helpful when adding fields, such as adding a `"host"` field as shown in the example.

### Key Conflicts

Keys specified in this transform will replace existing keys.

### Nested Fields

The `add_fields` transform will support dotted keys or [TOML tables][url.toml_table]. We recommend the dotted key syntax since it is less verbose for this usecase:

```
[transforms.<transform-id>]
  # ...
  
  [transforms.<transform-id>.fields]
    parent.child.grandchild = "<value>"
```

### Removing Fields

See the [`remove_fields` transform][docs.remove_fields_transform].

### Special Characters

Aside from the [special characters][docs.event_key_special_characters] listed in the [Data Model][docs.data_model] doc, Vector does not restrict the characters allowed in keys. You can wrap key names in `" "` quote to preserve spaces and use `\` to escape quotes.

### Types

All supported [configuration value types][docs.config_value_types] are accepted. For convenience, here's what that means:

#### Primitive Types

All [primitive TOML types (`string`, `int`, `float`, `boolean`,) are supported as shown in the [Config file example](#config-file).

#### List Types

[TOML arrays][url.toml_array]. Keep in mind that the values must be simple type (not tables), and each value must the same type. You cannot mix types:

```
[transforms.<transform-id>]
  # ...
  
  [transforms.<transform-id>.fields]
    my_array = ["first", "second", "third"]
```

## Troubleshooting

The best place to start with troubleshooting is to check the
[Vector logs][docs.monitoring_logs]. This is typically located at
`/var/log/vector.log`, then proceed to follow the
[Troubleshooting Guide][docs.troubleshooting].

If the [Troubleshooting Guide][docs.troubleshooting] does not resolve your
issue, please:

1. Check for any [open sink issues][url.add_fields_transform_issues].
2. [Search the forum][url.search_forum] for any similar issues.
2. Reach out to the [community][url.community] for help.


### Alternatives

Finally, consider the following alternatives:


* [`remove_fields` transform][docs.remove_fields_transform]

## Resources

* [**Issues**][url.add_fields_transform_issues] - [enhancements][url.add_fields_transform_enhancements] - [bugs][url.add_fields_transform_bugs]
* [**Source code**][url.add_fields_transform_source]


[docs.config_composition]: https://docs.vector.dev/usage/configuration/README#composition
[docs.config_value_types]: https://docs.vector.dev/usage/configuration/README#value-types
[docs.configuration]: https://docs.vector.dev/usage/configuration
[docs.data_model]: https://docs.vector.dev/about/data-model
[docs.event_key_special_characters]: https://docs.vector.dev/about/data-model#special-characters
[docs.log_event]: https://docs.vector.dev/about/data-model#log
[docs.lua_transform]: https://docs.vector.dev/usage/configuration/transforms/lua
[docs.monitoring_logs]: https://docs.vector.dev/usage/administration/monitoring#logs
[docs.remove_fields_transform]: https://docs.vector.dev/usage/configuration/transforms/remove_fields
[docs.sources]: https://docs.vector.dev/usage/configuration/sources
[docs.transforms]: https://docs.vector.dev/usage/configuration/transforms
[docs.troubleshooting]: https://docs.vector.dev/usage/guides/troubleshooting
[images.add_fields_transform]: https://docs.vector.dev/assets/add_fields-transform.svg
[url.add_fields_transform_bugs]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Transform%3A+add_fields%22+label%3A%22Type%3A+Bug%22
[url.add_fields_transform_enhancements]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Transform%3A+add_fields%22+label%3A%22Type%3A+Enhancement%22
[url.add_fields_transform_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Transform%3A+add_fields%22
[url.add_fields_transform_source]: https://github.com/timberio/vector/tree/master/src/transforms/add_fields.rs
[url.community]: https://vector.dev/community
[url.search_forum]: https://forum.vector.dev/search?expanded=true
[url.toml_array]: https://github.com/toml-lang/toml#array
[url.toml_table]: https://github.com/toml-lang/toml#table
