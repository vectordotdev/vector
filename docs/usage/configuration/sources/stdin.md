

---
description: Ingests data through standard input (STDIN) and outputs `log` events.
---

<!--
     THIS FILE IS AUTOOGENERATED!

     To make changes please edit the template located at:

     scripts/generate/templates/docs/usage/configuration/sources/stdin.md.erb
-->

# stdin source

![][images.stdin_source]


The `stdin` source ingests data through standard input (STDIN) and outputs [`log`][docs.log_event] events.

## Config File

{% code-tabs %}
{% code-tabs-item title="vector.toml (example)" %}
```toml
[sinks.my_stdin_source_id]
  # REQUIRED - General
  type = "stdin" # must be: "stdin"

  # OPTIONAL - General
  max_length = 102400 # default, bytes

  # OPTIONAL - Context
  host_key = "host" # default
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (schema)" %}
```toml
[sinks.<sink-id>]
  # REQUIRED - General
  type = "stdin"

  # OPTIONAL - General
  max_length = <int>

  # OPTIONAL - Context
  host_key = "<string>"
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (specification)" %}
```toml
[sinks.stdin]
  #
  # General
  #

  # The component type
  # 
  # * required
  # * no default
  # * must be: "stdin"
  type = "stdin"

  # The maxiumum bytes size of a message before it is discarded.
  # 
  # * optional
  # * default: 102400
  # * unit: bytes
  max_length = 102400

  #
  # Context
  #

  # The key name added to each event representing the current host.
  # 
  # * optional
  # * default: "host"
  host_key = "host"
```
{% endcode-tabs-item %}
{% endcode-tabs %}

## Options

| Key  | Type  | Description |
|:-----|:-----:|:------------|
| **REQUIRED** - General | | |
| `type` | `string` | The component type<br />`required` `enum: "stdin"` |
| **OPTIONAL** - General | | |
| `max_length` | `int` | The maxiumum bytes size of a message before it is discarded.<br />`default: 102400` `unit: bytes` |
| **OPTIONAL** - Context | | |
| `host_key` | `string` | The key name added to each event representing the current host.<br />`default: "host"` |

## Examples

Given the following input line:

{% code-tabs %}
{% code-tabs-item title="stdin" %}
```
2019-02-13T19:48:34+00:00 [info] Started GET "/" for 127.0.0.1
```
{% endcode-tabs-item %}
{% endcode-tabs %}

A [`log` event][docs.log_event] will be emitted with the following structure:

{% code-tabs %}
{% code-tabs-item title="log" %}
```javascript
{
  "timestamp": <timestamp> # current time,
  "message": "2019-02-13T19:48:34+00:00 [info] Started GET "/" for 127.0.0.1",
  "host": "10.2.22.122" # current hostname
}
```

The "timestamp" and `"host"` keys were automatically added as context. You can further parse the `"message"` key with a [transform][docs.transforms], such as the [`regeex` transform][docs.regex_parser_transform].
{% endcode-tabs-item %}
{% endcode-tabs %}

## How It Works

### Delivery Guarantee

This component offers an [**at least once** delivery guarantee][docs.at_least_once_delivery]
if your [pipeline is configured to achieve this][docs.at_least_once_delivery].

### Line Delimiters

Each line is read until a new line delimiter (the `0xA` byte) is found.

## Troubleshooting

The best place to start with troubleshooting is to check the
[Vector logs][docs.monitoring_logs]. This is typically located at
`/var/log/vector.log`, then proceed to follow the
[Troubleshooting Guide][docs.troubleshooting].

If the [Troubleshooting Guide][docs.troubleshooting] does not resolve your
issue, please:

1. Check for any [open sink issues][url.stdin_source_issues].
2. [Search the forum][url.search_forum] for any similar issues.
2. Reach out to the [community][url.community] for help.

## Resources

* [**Issues**][url.stdin_source_issues] - [enhancements][url.stdin_source_enhancements] - [bugs][url.stdin_source_bugs]
* [**Source code**][url.stdin_source_source]