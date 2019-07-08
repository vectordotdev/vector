---
description: Ingests data through the Syslog 5424 protocol and outputs `log` events.
---

<!--
     THIS FILE IS AUTOOGENERATED!

     To make changes please edit the template located at:

     scripts/generate/templates/docs/usage/configuration/sources/syslog.md.erb
-->

# syslog source

![][images.syslog_source]


The `syslog` source ingests data through the Syslog 5424 protocol and outputs [`log`][docs.log_event] events.

## Config File

{% code-tabs %}
{% code-tabs-item title="vector.toml (example)" %}
```coffeescript
[sinks.my_syslog_source_id]
  # REQUIRED - General
  type = "syslog" # must be: "syslog"
  
  # OPTIONAL - General
  address = "0.0.0.0:9000" # no default
  max_length = 102400 # default, bytes
  mode = "tcp" # no default, enum: "tcp", "udp", "unix"
  path = "/path/to/socket" # no default
  
  # OPTIONAL - Context
  host_key = "host" # default
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (schema)" %}
```coffeescript
[sinks.<sink-id>]
  # REQUIRED - General
  type = "syslog"

  # OPTIONAL - General
  address = "<string>"
  max_length = <int>
  mode = {"tcp" | "udp" | "unix"}
  path = "<string>"

  # OPTIONAL - Context
  host_key = "<string>"
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (specification)" %}
```coffeescript
[sinks.syslog]
  #
  # General
  #

  # The component type
  # 
  # * required
  # * no default
  # * must be: "syslog"
  type = "syslog"

  # The TCP or UDP address to listen on. Only relevant when `mode` is `tcp` or
  # `udp`.
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

  # The input mode.
  # 
  # * optional
  # * no default
  # * enum: "tcp", "udp", "unix"
  mode = "tcp"
  mode = "udp"
  mode = "unix"

  # The unix socket path. *This should be absolute path.* Only relevant when
  # `mode` is `unix`.
  # 
  # * optional
  # * no default
  path = "/path/to/socket"

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
| `type` | `string` | The component type<br />`required` `enum: "syslog"` |
| **OPTIONAL** - General | | |
| `address` | `string` | The TCP or UDP address to listen on. Only relevant when `mode` is `tcp` or `udp`.<br />`no default` `example: "0.0.0.0:9000"` |
| `max_length` | `int` | The maximum bytes size of incoming messages before they are discarded.<br />`default: 102400` `unit: bytes` |
| `mode` | `string` | The input mode.<br />`no default` `enum: "tcp", "udp", "unix"` |
| `path` | `string` | The unix socket path. *This should be absolute path.* Only relevant when `mode` is `unix`.
<br />`no default` `example: "/path/to/socket"` |
| **OPTIONAL** - Context | | |
| `host_key` | `string` | The key name added to each event representing the current host.<br />`default: "host"` |

## Examples

Given the following input line:

{% code-tabs %}
{% code-tabs-item title="stdin" %}
Given the following input

```
<34>1 2018-10-11T22:14:15.003Z mymachine.example.com su - ID47 - 'su root' failed for lonvick on /dev/pts/8
```
{% endcode-tabs-item %}
{% endcode-tabs %}

A [`log` event][docs.log_event] will be emitted with the following structure:

{% code-tabs %}
{% code-tabs-item title="log" %}
```javascript
{
  "timestamp": <2018-10-11T22:14:15.003Z> # current time,
  "message": "<34>1 2018-10-11T22:14:15.003Z mymachine.example.com su - ID47 - 'su root' failed for lonvick on /dev/pts/8",
  "host": "mymachine.example.com",
  "peer_path": "/path/to/unix/socket" # only relevant if `mode` is `unix`
}
```

Vector only extracts the `"timestamp"` and `"host"` fields and leaves the `"message"` in-tact. You can further parse the `"message"` key with a [transform][docs.transforms], such as the [`regeex` transform][docs.regex_parser_transform].
{% endcode-tabs-item %}
{% endcode-tabs %}

## How It Works

### Delivery Guarantee

Due to the nature of this component, it offers a
[**best effort** delivery guarantee][docs.best_effort_delivery].

### Line Delimiters

Each line is read until a new line delimiter (the `0xA` byte) is found.

### Parsing

Vector will parse messages in the [Syslog 5424][url.syslog_5424] format.

#### Successful parsing

Upon successful parsing, Vector will create a structured event. For example, given this Syslog message:

```
<13>1 2019-02-13T19:48:34+00:00 74794bfb6795 root 8449 - [meta sequenceId="1"] i am foobar
```

Vector will produce an event with this structure.

```javascript
{
  "message": "<13>1 2019-02-13T19:48:34+00:00 74794bfb6795 root 8449 - [meta sequenceId="1"] i am foobar",
  "timestamp": "2019-02-13T19:48:34+00:00",
  "host": "74794bfb6795"
}
```

#### Unsuccessful parsing

Anyone with Syslog experience knows there are often deviations from the Syslog specifications. Vector tries its best to account for these (note the tests here). In the event Vector fails to parse your format, we recommend that you open an issue informing us of this, and then proceed to use the `tcp`, `udp`, or `unix` source coupled with a parser [transform][docs.transforms] transform of your choice.


## Troubleshooting

The best place to start with troubleshooting is to check the
[Vector logs][docs.monitoring_logs]. This is typically located at
`/var/log/vector.log`, then proceed to follow the
[Troubleshooting Guide][docs.troubleshooting].

If the [Troubleshooting Guide][docs.troubleshooting] does not resolve your
issue, please:

1. Check for any [open sink issues][url.syslog_source_issues].
2. [Search the forum][url.search_forum] for any similar issues.
2. Reach out to the [community][url.community] for help.

## Resources

* [**Issues**][url.syslog_source_issues] - [enhancements][url.syslog_source_enhancements] - [bugs][url.syslog_source_bugs]
* [**Source code**][url.syslog_source_source]


[docs.best_effort_delivery]: ../../../about/guarantees.md#best-effort-delivery
[docs.log_event]: ../../../about/data-model.md#log
[docs.monitoring_logs]: ../../../usage/administration/monitoring.md#logs
[docs.regex_parser_transform]: ../../../usage/configuration/transforms/regex_parser.md
[docs.transforms]: ../../../usage/configuration/transforms
[docs.troubleshooting]: ../../../usage/guides/troubleshooting.md
[images.syslog_source]: ../../../assets/syslog-source.svg
[url.community]: https://vector.dev/community
[url.search_forum]: https://forum.vector.dev/search?expanded=true
[url.syslog_5424]: https://tools.ietf.org/html/rfc5424
[url.syslog_source_bugs]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Source%3A+syslog%22+label%3A%22Type%3A+Bug%22
[url.syslog_source_enhancements]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Source%3A+syslog%22+label%3A%22Type%3A+Enhancement%22
[url.syslog_source_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Source%3A+syslog%22
[url.syslog_source_source]: https://github.com/timberio/vector/tree/master/src/sources/syslog.rs
