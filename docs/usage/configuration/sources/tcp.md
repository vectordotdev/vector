<!--
     THIS FILE IS AUTOOGENERATED!

     To make changes please edit the template located at:

     scripts/generate/templates/docs/usage/configuration/sources/tcp.md.erb
-->

---
description: Ingests data through the TCP protocol and outputs `log` events.
---

# tcp source

![][images.tcp_source]


The `tcp` source ingests data through the TCP protocol and outputs [`log`][docs.log_event] events.

## Config File

{% code-tabs %}
{% code-tabs-item title="vector.toml (example)" %}
```toml
[sinks.my_tcp_source_id]
  # REQUIRED - General
  type = "tcp" # must be: "tcp"

  # OPTIONAL - General
  address = "0.0.0.0:9000" # no default
  max_length = 102400 # default, bytes
  shutdown_timeout_secs = 30 # default, seconds

  # OPTIONAL - Context
  host_key = "host" # default
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (schema)" %}
```toml
[sinks.<sink-id>]
  # REQUIRED - General
  type = "tcp"

  # OPTIONAL - General
  address = "<string>"
  max_length = <int>
  shutdown_timeout_secs = <int>

  # OPTIONAL - Context
  host_key = "<string>"
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (specification)" %}
```toml
[sinks.tcp]
  #
  # General
  #

  # The component type
  # 
  # * required
  # * no default
  # * must be: "tcp"
  type = "tcp"

  # The address to bind the socket to.
  # 
  # * optional
  # * no default
  address = "0.0.0.0:9000"

  # The maximum bytes size of incoming messages before they are discarded.
  # 
  # * optional
  # * default: 102400
  # * unit: bytes
  max_length = 102400

  # The timeout before a connection is forcefully closed during shutdown.
  # 
  # * optional
  # * default: 30
  # * unit: seconds
  shutdown_timeout_secs = 30

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
| `type` | `string` | The component type<br />`required` `enum: "tcp"` |
| **OPTIONAL** - General | | |
| `address` | `string` | The address to bind the socket to.<br />`no default` `example: "0.0.0.0:9000"` |
| `max_length` | `int` | The maximum bytes size of incoming messages before they are discarded.<br />`default: 102400` `unit: bytes` |
| `shutdown_timeout_secs` | `int` | The timeout before a connection is forcefully closed during shutdown.<br />`default: 30` `unit: seconds` |
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
  "host": "10.2.22.122" # current nostname
}
```

The "timestamp" and `"host"` keys were automatically added as context. You can further parse the `"message"` key with a [transform][docs.transforms], such as the [`regeex` transform][docs.regex_parser_transform].
{% endcode-tabs-item %}
{% endcode-tabs %}

## How It Works

### Delivery Guarantee

Due to the nature of this component, it offers a
[**best effort** delivery guarantee][docs.best_effort_delivery].

### Line Delimiters

Each line is read until a new line delimiter (the `0xA` byte) is found.


## Troubleshooting

The best place to start with troubleshooting is to check the
[Vector logs][docs.monitoring_logs]. This is typically located at
`/var/log/vector.log`, then proceed to follow the
[Troubleshooting Guide][docs.troubleshooting].

If the [Troubleshooting Guide][docs.troubleshooting] does not resolve your
issue, please:

1. Check for any [open sink issues][url.tcp_source_issues].
2. [Search the forum][url.search_forum] for any similar issues.
2. Reach out to the [community][url.community] for help.

## Resources

* [**Issues**][url.tcp_source_issues] - [enhancements][url.tcp_source_enhancements] - [bugs][url.tcp_source_bugs]
* [**Source code**][url.tcp_source_source]


[docs.best_effort_delivery]: ../../../about/guarantees.md#best-effort-delivery
[docs.log_event]: ../../../about/data-model.md#log
[docs.monitoring_logs]: ../../../usage/administration/monitoring.md#logs
[docs.regex_parser_transform]: ../../../usage/configuration/transforms/regex_parser.md
[docs.transforms]: ../../../usage/configuration/transforms
[docs.troubleshooting]: ../../../usage/guides/troubleshooting.md
[images.tcp_source]: ../../../assets/tcp-source.svg
[url.community]: https://vector.dev/community
[url.search_forum]: https://forum.vector.dev/search?expanded=true
[url.tcp_source_bugs]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Source%3A+tcp%22+label%3A%22Type%3A+Bugs%22
[url.tcp_source_enhancements]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Source%3A+tcp%22+label%3A%22Type%3A+Enhancements%22
[url.tcp_source_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Source%3A+tcp%22
[url.tcp_source_source]: https://github.com/timberio/vector/tree/master/src/sources/tcp.rs
