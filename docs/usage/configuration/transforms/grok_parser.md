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
[sinks.my_grok_parser_transform_id]
  # REQUIRED
  type = "grok_parser" # must be: "grok_parser"
  inputs = ["my-source-id"]
  pattern = "%{TIMESTAMP_ISO8601:timestamp} %{LOGLEVEL:level} %{GREEDYDATA:message}"
  
  # OPTIONAL
  drop_field = true # default
  field = "message" # default
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (schema)" %}
```coffeescript
[sinks.<sink-id>]
  type = "grok_parser"
  inputs = ["<string>", ...]
  pattern = "<string>"
  drop_field = <bool>
  field = "<string>"
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (specification)" %}
```coffeescript
[sinks.grok_parser]
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
```
{% endcode-tabs-item %}
{% endcode-tabs %}

## Options

| Key  | Type  | Description |
|:-----|:-----:|:------------|
| **REQUIRED** | | |
| `type` | `string` | The component type<br />`required` `enum: "grok_parser"` |
| `inputs` | `[string]` | A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.config_composition] for more info.<br />`required` `example: ["my-source-id"]` |
| `pattern` | `string` | The [Grok pattern][url.grok_patterns]<br />`required` `example: (see above)` |
| **OPTIONAL** | | |
| `drop_field` | `bool` | If `true` will drop the `field` after parsing.<br />`default: true` |
| `field` | `string` | The field to execute the `pattern` against. Must be a `string` value.<br />`default: "message"` |

## How It Works



### Debugging

We recommend the [Grok debugger][url.grok_debugger] for Grok testing.

### Patterns

Vector uses the Rust [`grok` library][url.rust_grok_library]. All patterns
[listed here][url.grok_patterns] are supported. It is recommended to use any
maintained patterns when possible.

### Performance

Grok is approximately 50% slower than it's Regex counterpart. We plan to add a
[performance test][docs.performance] for this in the future. While this is
still plenty fast for most use cases we recommend using the [`regex_parser`
transform][docs.regex_parser_transform] if you are experiencing performance
issues.

## Troubleshooting

The best place to start with troubleshooting is to check the
[Vector logs][docs.monitoring_logs]. This is typically located at
`/var/log/vector.log`, then proceed to follow the
[Troubleshooting Guide][docs.troubleshooting].

If the [Troubleshooting Guide][docs.troubleshooting] does not resolve your
issue, please:

1. Check for any [open sink issues][url.grok_parser_transform_issues].
2. [Search the forum][url.search_forum] for any similar issues.
2. Reach out to the [community][url.community] for help.


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


[docs.config_composition]: https://docs.vector.dev/usage/configuration/README#composition
[docs.log_event]: https://docs.vector.dev/about/data-model#log
[docs.lua_transform]: https://docs.vector.dev/usage/configuration/transforms/lua
[docs.monitoring_logs]: https://docs.vector.dev/usage/administration/monitoring#logs
[docs.performance]: https://docs.vector.dev/performance
[docs.regex_parser_transform]: https://docs.vector.dev/usage/configuration/transforms/regex_parser
[docs.sources]: https://docs.vector.dev/usage/configuration/sources
[docs.tokenizer_transform]: https://docs.vector.dev/usage/configuration/transforms/tokenizer
[docs.transforms]: https://docs.vector.dev/usage/configuration/transforms
[docs.troubleshooting]: https://docs.vector.dev/usage/guides/troubleshooting
[images.grok_parser_transform]: https://docs.vector.dev/assets/grok_parser-transform.svg
[url.community]: https://vector.dev/community
[url.grok]: http://grokdebug.herokuapp.com/
[url.grok_debugger]: http://grokdebug.herokuapp.com/
[url.grok_parser_transform_bugs]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Transform%3A+grok_parser%22+label%3A%22Type%3A+Bug%22
[url.grok_parser_transform_enhancements]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Transform%3A+grok_parser%22+label%3A%22Type%3A+Enhancement%22
[url.grok_parser_transform_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Transform%3A+grok_parser%22
[url.grok_parser_transform_source]: https://github.com/timberio/vector/tree/master/src/transforms/grok_parser.rs
[url.grok_patterns]: https://github.com/daschl/grok/tree/master/patterns
[url.rust_grok_library]: https://github.com/daschl/grok
[url.search_forum]: https://forum.vector.dev/search?expanded=true
