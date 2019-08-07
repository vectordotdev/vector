---
description: Accepts `log` events and allows you to parse a field value with Grok.
---

<!--
     THIS FILE IS AUTOOGENERATED!

     To make changes please edit the template located at:

     scripts/generate/templates/docs/usage/configuration/transforms/grok_parser.md.erb
-->

# grok_parser transform

![][images.grok_parser_transform]


The `grok_parser` transform accepts [`log`][docs.log_event] events and allows you to parse a field value with [Grok][url.grok].

## Config File

{% code-tabs %}
{% code-tabs-item title="vector.toml (example)" %}
```coffeescript
[transforms.my_transform_id]
  # REQUIRED - General
  type = "grok_parser" # must be: "grok_parser"
  inputs = ["my-source-id"]
  pattern = "%{TIMESTAMP_ISO8601:timestamp} %{LOGLEVEL:level} %{GREEDYDATA:message}"
  
  # OPTIONAL - General
  drop_field = true # default
  field = "message" # default
  
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
  type = "grok_parser"
  inputs = ["<string>", ...]
  pattern = "<string>"

  # OPTIONAL - General
  drop_field = <bool>
  field = "<string>"

  # OPTIONAL - Types
  [transforms.<transform-id>.types]
    * = {"string" | "int" | "float" | "bool" | "timestamp|strftime"}
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (specification)" %}
```coffeescript
[transforms.grok_parser_transform]
  #
  # General
  #

  # The component type
  # 
  # * required
  # * no default
  # * must be: "grok_parser"
  type = "grok_parser"

  # A list of upstream source or transform IDs. See Config Composition for more
  # info.
  # 
  # * required
  # * no default
  inputs = ["my-source-id"]

  # The Grok pattern
  # 
  # * required
  # * no default
  pattern = "%{TIMESTAMP_ISO8601:timestamp} %{LOGLEVEL:level} %{GREEDYDATA:message}"

  # If `true` will drop the `field` after parsing.
  # 
  # * optional
  # * default: true
  drop_field = true

  # The field to execute the `pattern` against. Must be a `string` value.
  # 
  # * optional
  # * default: "message"
  field = "message"

  #
  # Types
  #

  [transforms.grok_parser_transform.types]
    # A definition of mapped field types. They key is the field name and the value
    # is the type. `strftime` specifiers are supported for the `timestamp` type.
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
| `type` | `string` | The component type<br />`required` `must be: "grok_parser"` |
| `inputs` | `[string]` | A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.config_composition] for more info.<br />`required` `example: ["my-source-id"]` |
| `pattern` | `string` | The [Grok pattern][url.grok_patterns]<br />`required` `example: (see above)` |
| **OPTIONAL** - General | | |
| `drop_field` | `bool` | If `true` will drop the `field` after parsing.<br />`default: true` |
| `field` | `string` | The field to execute the `pattern` against. Must be a `string` value.<br />`default: "message"` |
| **OPTIONAL** - Types | | |
| `types.*` | `string` | A definition of mapped field types. They key is the field name and the value is the type. [`strftime` specifiers][url.strftime_specifiers] are supported for the `timestamp` type.<br />`required` `enum: "string", "int", "float", "bool", and "timestamp\|strftime"` |

## How It Works

### Available Patterns

Vector uses the Rust [`grok` library][url.rust_grok_library]. All patterns
[listed here][url.grok_patterns] are supported. It is recommended to use
maintained patterns when possible since they can be improved over time by
the community.

### Debugging

We recommend the [Grok debugger][url.grok_debugger] for Grok testing.

### Environment Variables

Environment variables are supported through all of Vector's configuration.
Simply add `${MY_ENV_VAR}` in your Vector configuration file and the variable
will be replaced before being evaluated.

You can learn more in the [Environment Variables][docs.configuration.environment-variables]
section.

### Performance

Grok is approximately 50% slower than the [`regex_parser` transform][docs.regex_parser_transform].
We plan to add a [performance test][docs.performance] for this in the future.
While this is still plenty fast for most use cases we recommend using the
[`regex_parser` transform][docs.regex_parser_transform] if you are experiencing
performance issues.

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

1. Check for any [open `grok_parser_transform` issues][url.grok_parser_transform_issues].
2. If encountered a bug, please [file a bug report][url.new_grok_parser_transform_bug].
3. If encountered a missing feature, please [file a feature request][url.new_grok_parser_transform_enhancement].
4. If you need help, [join our chat/forum community][url.vector_chat]. You can post a question and search previous questions.


### Alternatives

Finally, consider the following alternatives:

* [`lua` transform][docs.lua_transform]
* [`regex_parser` transform][docs.regex_parser_transform]
* [`tokenizer` transform][docs.tokenizer_transform]

## Resources

* [**Issues**][url.grok_parser_transform_issues] - [enhancements][url.grok_parser_transform_enhancements] - [bugs][url.grok_parser_transform_bugs]
* [**Source code**][url.grok_parser_transform_source]
* [**Grok Debugger**][url.grok_debugger]
* [**Grok Patterns**][url.grok_patterns]


[docs.config_composition]: ../../../usage/configuration/README.md#composition
[docs.configuration.environment-variables]: ../../../usage/configuration#environment-variables
[docs.log_event]: ../../../about/data-model/log.md
[docs.lua_transform]: ../../../usage/configuration/transforms/lua.md
[docs.monitoring_logs]: ../../../usage/administration/monitoring.md#logs
[docs.performance]: ../../../performance.md
[docs.regex_parser_transform]: ../../../usage/configuration/transforms/regex_parser.md
[docs.sources]: ../../../usage/configuration/sources
[docs.tokenizer_transform]: ../../../usage/configuration/transforms/tokenizer.md
[docs.transforms]: ../../../usage/configuration/transforms
[docs.troubleshooting]: ../../../usage/guides/troubleshooting.md
[images.grok_parser_transform]: ../../../assets/grok_parser-transform.svg
[url.grok]: http://grokdebug.herokuapp.com/
[url.grok_debugger]: http://grokdebug.herokuapp.com/
[url.grok_parser_transform_bugs]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Transform%3A+grok_parser%22+label%3A%22Type%3A+Bug%22
[url.grok_parser_transform_enhancements]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Transform%3A+grok_parser%22+label%3A%22Type%3A+Enhancement%22
[url.grok_parser_transform_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Transform%3A+grok_parser%22
[url.grok_parser_transform_source]: https://github.com/timberio/vector/tree/master/src/transforms/grok_parser.rs
[url.grok_patterns]: https://github.com/daschl/grok/tree/master/patterns
[url.new_grok_parser_transform_bug]: https://github.com/timberio/vector/issues/new?labels=Transform%3A+grok_parser&labels=Type%3A+Bug
[url.new_grok_parser_transform_enhancement]: https://github.com/timberio/vector/issues/new?labels=Transform%3A+grok_parser&labels=Type%3A+Enhancement
[url.rust_grok_library]: https://github.com/daschl/grok
[url.strftime_specifiers]: https://docs.rs/chrono/0.3.1/chrono/format/strftime/index.html
[url.vector_chat]: https://chat.vector.dev
