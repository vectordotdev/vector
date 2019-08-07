---
description: Accepts `log` events and allows you to coerce event fields into fixed types.
---

<!--
     THIS FILE IS AUTOOGENERATED!

     To make changes please edit the template located at:

     scripts/generate/templates/docs/usage/configuration/transforms/coercer.md.erb
-->

# coercer transform

![][images.coercer_transform]


The `coercer` transform accepts [`log`][docs.log_event] events and allows you to coerce event fields into fixed types.

## Config File

{% code-tabs %}
{% code-tabs-item title="vector.toml (example)" %}
```coffeescript
[transforms.my_transform_id]
  # REQUIRED - General
  type = "coercer" # must be: "coercer"
  inputs = ["my-source-id"]
  
  # OPTIONAL - Types
  [transforms.my_transform_id.types]
    status = "int"
    duration = "float"
    success = "bool"
    timestamp = "timestamp|%s"
    timestamp = "timestamp|%+"
    timestamp = "timestamp|%F"
    timestamp = "timestamp|%a %b %e %T %Y"
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (schema)" %}
```coffeescript
[transforms.<transform-id>]
  # REQUIRED - General
  type = "coercer"
  inputs = ["<string>", ...]

  # OPTIONAL - Types
  [transforms.<transform-id>.types]
    * = {"string" | "int" | "float" | "bool" | "timestamp|strftime"}
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (specification)" %}
```coffeescript
[transforms.coercer_transform]
  #
  # General
  #

  # The component type
  # 
  # * required
  # * no default
  # * must be: "coercer"
  type = "coercer"

  # A list of upstream source or transform IDs. See Config Composition for more
  # info.
  # 
  # * required
  # * no default
  inputs = ["my-source-id"]

  #
  # Types
  #

  [transforms.coercer_transform.types]
    # A definition of field type conversions. They key is the field name and the
    # value is the type. `strftime` specifiers are supported for the `timestamp`
    # type.
    # 
    # * required
    # * no default
    # * enum: "string", "int", "float", "bool", and "timestamp|strftime"
    status = "int"
    duration = "float"
    success = "bool"
    timestamp = "timestamp|%s"
    timestamp = "timestamp|%+"
    timestamp = "timestamp|%F"
    timestamp = "timestamp|%a %b %e %T %Y"
```
{% endcode-tabs-item %}
{% endcode-tabs %}

## Options

| Key  | Type  | Description |
|:-----|:-----:|:------------|
| **REQUIRED** - General | | |
| `type` | `string` | The component type<br />`required` `must be: "coercer"` |
| `inputs` | `[string]` | A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.config_composition] for more info.<br />`required` `example: ["my-source-id"]` |
| **OPTIONAL** - Types | | |
| `types.*` | `string` | A definition of field type conversions. They key is the field name and the value is the type. [`strftime` specifiers][url.strftime_specifiers] are supported for the `timestamp` type.<br />`required` `enum: "string", "int", "float", "bool", and "timestamp\|strftime"` |

## Examples

Given the following input event:

{% code-tabs %}
{% code-tabs-item title="log" %}
```json
{
  // ... existing fields
  "bytes_in": "5667",
  "bytes_out": "20574",
  "host": "5.86.210.12",
  "message": "GET /embrace/supply-chains/dynamic/vertical",
  "status": "201",
  "timestamp": "19/06/2019:17:20:49 -0400",
  "user_id": "zieme4647"
}
```
{% endcode-tabs-item %}
{% endcode-tabs %}

And the following configuration:

{% code-tabs %}
{% code-tabs-item title="vector.toml" %}
```coffeescript
[transforms.<transform-id>]
  type = "coercer"

[transforms.<transform-id>.types]
  bytes_in = "int"
  bytes_out = "int"
  timestamp = "timestamp|%m/%d/%Y:%H:%M:%S %z"
  status = "int"
```
{% endcode-tabs-item %}
{% endcode-tabs %}

A [`log` event][docs.log_event] will be emitted with the following structure:

```javascript
{
  // ... existing fields
  "bytes_in": 5667,
  "bytes_out": 20574,
  "host": "5.86.210.12",
  "message": "GET /embrace/supply-chains/dynamic/vertical",
  "status": 201,
  "timestamp": <19/06/2019:17:20:49 -0400>,
  "user_id": "zieme4647"
}
```

## How It Works

### Environment Variables

Environment variables are supported through all of Vector's configuration.
Simply add `${MY_ENV_VAR}` in your Vector configuration file and the variable
will be replaced before being evaluated.

You can learn more in the [Environment Variables][docs.configuration.environment-variables]
section.

### Types

By default, extracted (parsed) fields all contain `string` values. You can
coerce these values into types via the `types` table as shown in the
[Config File](#config-file) example above. For example:

```coffeescript
[transforms.my_transform_id]
  # ...

  # OPTIONAL - Types
  [transforms.my_transform_id.types]
    status = "int"
    duration = "float"
    success = "bool"
    timestamp = "timestamp|%s"
    timestamp = "timestamp|%+"
    timestamp = "timestamp|%F"
    timestamp = "timestamp|%a %b %e %T %Y"
```

The available types are:

| Type        | Desription                                                                                                          |
|:------------|:--------------------------------------------------------------------------------------------------------------------|
| `bool`      | Coerces to a `true`/`false` boolean. The `1`/`0` and `t`/`f` values are also coerced.                               |
| `float`     | Coerce to 64 bit floats.                                                                                            |
| `int`       | Coerce to a 64 bit integer.                                                                                         |
| `string`    | Coerces to a string. Generally not necessary since values are extracted as strings.                                 |
| `timestamp` | Coerces to a Vector timestamp. [`strftime` specificiers][url.strftime_specifiers] must be used to parse the string. |

## Troubleshooting

The best place to start with troubleshooting is to check the
[Vector logs][docs.monitoring_logs]. This is typically located at
`/var/log/vector.log`, then proceed to follow the
[Troubleshooting Guide][docs.troubleshooting].

If the [Troubleshooting Guide][docs.troubleshooting] does not resolve your
issue, please:

1. Check for any [open `coercer_transform` issues][url.coercer_transform_issues].
2. If encountered a bug, please [file a bug report][url.new_coercer_transform_bug].
3. If encountered a missing feature, please [file a feature request][url.new_coercer_transform_enhancement].
4. If you need help, [join our chat/forum community][url.vector_chat]. You can post a question and search previous questions.


### Alternatives

Finally, consider the following alternatives:

* [`lua` transform][docs.lua_transform]

## Resources

* [**Issues**][url.coercer_transform_issues] - [enhancements][url.coercer_transform_enhancements] - [bugs][url.coercer_transform_bugs]
* [**Source code**][url.coercer_transform_source]


[docs.config_composition]: ../../../usage/configuration/README.md#composition
[docs.configuration.environment-variables]: ../../../usage/configuration#environment-variables
[docs.log_event]: ../../../about/data-model/log.md
[docs.lua_transform]: ../../../usage/configuration/transforms/lua.md
[docs.monitoring_logs]: ../../../usage/administration/monitoring.md#logs
[docs.sources]: ../../../usage/configuration/sources
[docs.transforms]: ../../../usage/configuration/transforms
[docs.troubleshooting]: ../../../usage/guides/troubleshooting.md
[images.coercer_transform]: ../../../assets/coercer-transform.svg
[url.coercer_transform_bugs]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Transform%3A+coercer%22+label%3A%22Type%3A+Bug%22
[url.coercer_transform_enhancements]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Transform%3A+coercer%22+label%3A%22Type%3A+Enhancement%22
[url.coercer_transform_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Transform%3A+coercer%22
[url.coercer_transform_source]: https://github.com/timberio/vector/tree/master/src/transforms/coercer.rs
[url.new_coercer_transform_bug]: https://github.com/timberio/vector/issues/new?labels=Transform%3A+coercer&labels=Type%3A+Bug
[url.new_coercer_transform_enhancement]: https://github.com/timberio/vector/issues/new?labels=Transform%3A+coercer&labels=Type%3A+Enhancement
[url.strftime_specifiers]: https://docs.rs/chrono/0.3.1/chrono/format/strftime/index.html
[url.vector_chat]: https://chat.vector.dev
