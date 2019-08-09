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


The `tcp` sink [streams](#streaming) [`log`][docs.log_event] events to a TCP connection.

## Config File

{% code-tabs %}
{% code-tabs-item title="vector.toml (example)" %}
```coffeescript
[sinks.my_sink_id]
  # REQUIRED - General
  type = "tcp" # must be: "tcp"
  inputs = ["my-source-id"]
  address = "92.12.333.224:5000"
  
  # OPTIONAL - General
  healthcheck = true # default
  
  # OPTIONAL - Requests
  encoding = "json" # no default, enum: "json" or "text"
  
  # OPTIONAL - Buffer
  [sinks.my_sink_id.buffer]
    type = "memory" # default, enum: "memory" or "disk"
    when_full = "block" # default, enum: "block" or "drop_newest"
    max_size = 104900000 # no default, bytes, relevant when type = "disk"
    num_items = 500 # default, events, relevant when type = "memory"
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (schema)" %}
```coffeescript
[sinks.<sink-id>]
  # REQUIRED - General
  type = "tcp"
  inputs = ["<string>", ...]
  address = "<string>"

  # OPTIONAL - General
  healthcheck = <bool>

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
{% code-tabs-item title="vector.toml (specification)" %}
```coffeescript
[sinks.tcp_sink]
  #
  # General
  #

  # The component type
  # 
  # * required
  # * no default
  # * must be: "tcp"
  type = "tcp"

  # A list of upstream source or transform IDs. See Config Composition for more
  # info.
  # 
  # * required
  # * no default
  inputs = ["my-source-id"]

  # The TCP address.
  # 
  # * required
  # * no default
  address = "92.12.333.224:5000"

  # Enables/disables the sink healthcheck upon start.
  # 
  # * optional
  # * default: true
  healthcheck = true

  #
  # Requests
  #

  # The encoding format used to serialize the events before flushing. The default
  # is dynamic based on if the event is structured or not.
  # 
  # * optional
  # * no default
  # * enum: "json" or "text"
  encoding = "json"
  encoding = "text"

  #
  # Buffer
  #

  [sinks.tcp_sink.buffer]
    # The buffer's type / location. `disk` buffers are persistent and will be
    # retained between restarts.
    # 
    # * optional
    # * default: "memory"
    # * enum: "memory" or "disk"
    type = "memory"
    type = "disk"

    # The behavior when the buffer becomes full.
    # 
    # * optional
    # * default: "block"
    # * enum: "block" or "drop_newest"
    when_full = "block"
    when_full = "drop_newest"

    # The maximum size of the buffer on the disk.
    # 
    # * optional
    # * no default
    # * unit: bytes
    max_size = 104900000

    # The maximum number of events allowed in the buffer.
    # 
    # * optional
    # * default: 500
    # * unit: events
    num_items = 500
```
{% endcode-tabs-item %}
{% endcode-tabs %}

## Options

| Key  | Type  | Description |
|:-----|:-----:|:------------|
| **REQUIRED** - General | | |
| `type` | `string` | The component type<br />`required` `must be: "tcp"` |
| `inputs` | `[string]` | A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.config_composition] for more info.<br />`required` `example: ["my-source-id"]` |
| `address` | `string` | The TCP address.<br />`required` `example: "92.12.333.224:5000"` |
| **OPTIONAL** - General | | |
| `healthcheck` | `bool` | Enables/disables the sink healthcheck upon start. See [Health Checks](#health-checks) for more info.<br />`default: true` |
| **OPTIONAL** - Requests | | |
| `encoding` | `string` | The encoding format used to serialize the events before flushing. The default is dynamic based on if the event is structured or not. See [Encodings](#encodings) for more info.<br />`no default` `enum: "json" or "text"` |
| **OPTIONAL** - Buffer | | |
| `buffer.type` | `string` | The buffer's type / location. `disk` buffers are persistent and will be retained between restarts.<br />`default: "memory"` `enum: "memory" or "disk"` |
| `buffer.when_full` | `string` | The behavior when the buffer becomes full.<br />`default: "block"` `enum: "block" or "drop_newest"` |
| `buffer.max_size` | `int` | The maximum size of the buffer on the disk. Only relevant when type = "disk"<br />`no default` `example: 104900000` `unit: bytes` |
| `buffer.num_items` | `int` | The maximum number of [events][docs.event] allowed in the buffer. Only relevant when type = "memory"<br />`default: 500` `unit: events` |

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

#### Dynamic encoding

By default, the `encoding` chosen is dynamic based on the explicit/implcit
nature of the event's structure. For example, if this event is parsed (explicit
structuring), Vector will use `json` to encode the structured data. If the event
was not explicitly structured, the `text` encoding will be used.

To further explain why Vector adopts this default, take the simple example of
accepting data over the [`tcp` source][docs.tcp_source] and then connecting
it directly to the `tcp` sink. It is less
surprising that the outgoing data reflects the incoming data exactly since it
was not explicitly structured.

### Environment Variables

Environment variables are supported through all of Vector's configuration.
Simply add `${MY_ENV_VAR}` in your Vector configuration file and the variable
will be replaced before being evaluated.

You can learn more in the [Environment Variables][docs.configuration.environment-variables]
section.

### Health Checks

Health checks ensure that the downstream service is accessible and ready to
accept data. This check is performed upon sink initialization.

If the health check fails an error will be logged and Vector will proceed to
start. If you'd like to exit immediately upon health check failure, you can
pass the `--require-healthy` flag:

```bash
vector --config /etc/vector/vector.toml --require-healthy
```

And finally, if you'd like to disable health checks entirely for this sink
you can set the `healthcheck` option to `false`.

### Streaming

The `tcp` sink streams data on a real-time
event-by-event basis. It does not batch data.

## Troubleshooting

The best place to start with troubleshooting is to check the
[Vector logs][docs.monitoring_logs]. This is typically located at
`/var/log/vector.log`, then proceed to follow the
[Troubleshooting Guide][docs.troubleshooting].

If the [Troubleshooting Guide][docs.troubleshooting] does not resolve your
issue, please:

1. Check for any [open `tcp_sink` issues][url.tcp_sink_issues].
2. If encountered a bug, please [file a bug report][url.new_tcp_sink_bug].
3. If encountered a missing feature, please [file a feature request][url.new_tcp_sink_enhancement].
4. If you need help, [join our chat/forum community][url.vector_chat]. You can post a question and search previous questions.

## Resources

* [**Issues**][url.tcp_sink_issues] - [enhancements][url.tcp_sink_enhancements] - [bugs][url.tcp_sink_bugs]
* [**Source code**][url.tcp_sink_source]


[docs.best_effort_delivery]: ../../../about/guarantees.md#best-effort-delivery
[docs.config_composition]: ../../../usage/configuration/README.md#composition
[docs.configuration.environment-variables]: ../../../usage/configuration#environment-variables
[docs.event]: ../../../about/data-model/README.md#event
[docs.log_event]: ../../../about/data-model/log.md
[docs.monitoring_logs]: ../../../usage/administration/monitoring.md#logs
[docs.sources]: ../../../usage/configuration/sources
[docs.tcp_source]: ../../../usage/configuration/sources/tcp.md
[docs.transforms]: ../../../usage/configuration/transforms
[docs.troubleshooting]: ../../../usage/guides/troubleshooting.md
[images.tcp_sink]: ../../../assets/tcp-sink.svg
[url.new_tcp_sink_bug]: https://github.com/timberio/vector/issues/new?labels=Sink%3A+tcp&labels=Type%3A+Bug
[url.new_tcp_sink_enhancement]: https://github.com/timberio/vector/issues/new?labels=Sink%3A+tcp&labels=Type%3A+Enhancement
[url.tcp_sink_bugs]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Sink%3A+tcp%22+label%3A%22Type%3A+Bug%22
[url.tcp_sink_enhancements]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Sink%3A+tcp%22+label%3A%22Type%3A+Enhancement%22
[url.tcp_sink_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Sink%3A+tcp%22
[url.tcp_sink_source]: https://github.com/timberio/vector/tree/master/src/sinks/tcp.rs
[url.vector_chat]: https://chat.vector.dev
