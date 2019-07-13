---
description: Streams `log` events to a TCP connection.
---

<!--
     THIS FILE IS AUTOOGENERATED!

     To make changes please edit the template located at:

     scripts/generate/templates/docs/usage/configuration/sinks/tcp.md.erb
-->

# tcp sink

![][images.tcp_sink]


The `tcp` sink streams [`log`][docs.log_event] events to a TCP connection.

## Config File

{% code-tabs %}
{% code-tabs-item title="vector.toml (example)" %}
```coffeescript
[sinks.my_tcp_sink_id]
  # REQUIRED - General
  type = "tcp" # must be: "tcp"
  inputs = ["my-source-id"]
  address = "92.12.333.224:5000"
  
  # OPTIONAL - Requests
  encoding = "json" # default, enum: "json", "text"
  
  # OPTIONAL - Buffer
  [sinks.my_tcp_sink_id.buffer]
    type = "memory" # default, enum: "memory", "disk"
    when_full = "block" # default, enum: "block", "drop_newest"
    max_size = 104900000 # no default, relevant when type = "disk"
    num_items = 500 # default, relevant when type = "memory"
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (schema)" %}
```coffeescript
[sinks.<sink-id>]
  # REQUIRED - General
  type = "tcp"
  inputs = ["<string>", ...]
  address = "<string>"

  # OPTIONAL - Requests
  encoding = {"json" | "text"}

  # OPTIONAL - Buffer
  [sinks.<sink-id>.buffer]
    type = {"memory" | "disk"}
    when_full = {"block" | "drop_newest"}
    max_size = <int>
    num_items = <int>
```
{% endcode-tabs-item %}
{% endcode-tabs %}

## Options

| Key  | Type  | Description |
|:-----|:-----:|:------------|
| **REQUIRED** - General | | |
| `type` | `string` | The component type<br />`required` `enum: "tcp"` |
| `inputs` | `[string]` | A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.config_composition] for more info.<br />`required` `example: ["my-source-id"]` |
| `address` | `string` | The TCP address.<br />`required` `example: "92.12.333.224:5000"` |
| **OPTIONAL** - Requests | | |
| `encoding` | `string` | The encoding format used to serialize the events before flushing. See [Encodings](#encodings) for more info.<br />`default: "dynamic"` `enum: "json", "text"` |
| **OPTIONAL** - Buffer | | |
| `buffer.type` | `string` | The buffer's type / location. `disk` buffers are persistent and will be retained between restarts.<br />`default: "memory"` `enum: "memory", "disk"` |
| `buffer.when_full` | `string` | The behavior when the buffer becomes full.<br />`default: "block"` `enum: "block", "drop_newest"` |
| `buffer.max_size` | `int` | The maximum size of the buffer on the disk. Only relevant when type = "disk"<br />`no default` `example: 104900000` |
| `buffer.num_items` | `int` | The maximum number of [events][docs.event] allowed in the buffer. Only relevant when type = "memory"<br />`default: 500` |

## How It Works

### Delivery Guarantee

Due to the nature of this component, it offers a
[**best effort** delivery guarantee][docs.best_effort_delivery].

### Encodings

The `tcp` sink encodes events before writing
them downstream. This is controlled via the `encoding` option which accepts
the following options:

| Encoding | Description |
| :------- | :---------- |
| `json` | The payload will be encoded as a single JSON payload. |
| `text` | The payload will be encoded as new line delimited text, each line representing the value of the `"message"` key. |

### Environment Variables

Environment variables are supported through all of Vector's configuration.
Simply add `${MY_ENV_VAR}` or `$MY_ENV_VAR` in your Vector configuration file
and the variable will be replaced before being evaluated.

You can learn more in the [Environment Variables][docs.configuration.environment-variables]
section.

### Health Checks

Upon [starting][docs.starting], Vector will perform a simple health check
against this sink. The ensures that the downstream service is healthy and
reachable. By default, if the health check fails an error will be logged and
Vector will proceed to restart. Vector will continually check the health of
the service on an interval until healthy.

If you'd like to exit immediately when a service is unhealthy you can pass
the `--require-healthy` flag:

```bash
vector --config /etc/vector/vector.toml --require-healthy
```

Be careful when doing this if you have multiple sinks configured, as it will
prevent Vector from starting is one sink is unhealthy, preventing the other
healthy sinks from receiving data.

## Troubleshooting

The best place to start with troubleshooting is to check the
[Vector logs][docs.monitoring_logs]. This is typically located at
`/var/log/vector.log`, then proceed to follow the
[Troubleshooting Guide][docs.troubleshooting].

If the [Troubleshooting Guide][docs.troubleshooting] does not resolve your
issue, please:

1. Check for any [open sink issues][url.tcp_sink_issues].
2. [Search the forum][url.search_forum] for any similar issues.
2. Reach out to the [community][url.community] for help.

## Resources

* [**Issues**][url.tcp_sink_issues] - [enhancements][url.tcp_sink_enhancements] - [bugs][url.tcp_sink_bugs]
* [**Source code**][url.tcp_sink_source]


[docs.best_effort_delivery]: ../../../about/guarantees.md#best-effort-delivery
[docs.config_composition]: ../../../usage/configuration/README.md#composition
[docs.configuration.environment-variables]: ../../../usage/configuration#environment-variables
[docs.event]: ../../../about/data-model.md#event
[docs.log_event]: ../../../about/data-model.md#log
[docs.monitoring_logs]: ../../../usage/administration/monitoring.md#logs
[docs.sources]: ../../../usage/configuration/sources
[docs.starting]: ../../../usage/administration/starting.md
[docs.transforms]: ../../../usage/configuration/transforms
[docs.troubleshooting]: ../../../usage/guides/troubleshooting.md
[images.tcp_sink]: ../../../assets/tcp-sink.svg
[url.community]: https://vector.dev/community
[url.search_forum]: https://forum.vector.dev/search?expanded=true
[url.tcp_sink_bugs]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Sink%3A+tcp%22+label%3A%22Type%3A+Bug%22
[url.tcp_sink_enhancements]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Sink%3A+tcp%22+label%3A%22Type%3A+Enhancement%22
[url.tcp_sink_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Sink%3A+tcp%22
[url.tcp_sink_source]: https://github.com/timberio/vector/tree/master/src/sinks/tcp.rs
