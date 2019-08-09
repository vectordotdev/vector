---
description: Streams `log` events to another downstream Vector instance.
---

<!--
     THIS FILE IS AUTOOGENERATED!

     To make changes please edit the template located at:

     scripts/generate/templates/docs/usage/configuration/sinks/vector.md.erb
-->

# vector sink

![][images.vector_sink]


The `vector` sink [streams](#streaming) [`log`][docs.log_event] events to another downstream Vector instance.

## Config File

{% code-tabs %}
{% code-tabs-item title="vector.toml (example)" %}
```coffeescript
[sinks.my_sink_id]
  # REQUIRED - General
  type = "vector" # must be: "vector"
  inputs = ["my-source-id"]
  address = "92.12.333.224:5000"
  
  # OPTIONAL - General
  healthcheck = true # default
  
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
  type = "vector"
  inputs = ["<string>", ...]
  address = "<string>"

  # OPTIONAL - General
  healthcheck = <bool>

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
[sinks.vector_sink]
  #
  # General
  #

  # The component type
  # 
  # * required
  # * no default
  # * must be: "vector"
  type = "vector"

  # A list of upstream source or transform IDs. See Config Composition for more
  # info.
  # 
  # * required
  # * no default
  inputs = ["my-source-id"]

  # The downstream Vector address.
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
  # Buffer
  #

  [sinks.vector_sink.buffer]
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
| `type` | `string` | The component type<br />`required` `must be: "vector"` |
| `inputs` | `[string]` | A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.config_composition] for more info.<br />`required` `example: ["my-source-id"]` |
| `address` | `string` | The downstream Vector address.<br />`required` `example: "92.12.333.224:5000"` |
| **OPTIONAL** - General | | |
| `healthcheck` | `bool` | Enables/disables the sink healthcheck upon start. See [Health Checks](#health-checks) for more info.<br />`default: true` |
| **OPTIONAL** - Buffer | | |
| `buffer.type` | `string` | The buffer's type / location. `disk` buffers are persistent and will be retained between restarts.<br />`default: "memory"` `enum: "memory" or "disk"` |
| `buffer.when_full` | `string` | The behavior when the buffer becomes full.<br />`default: "block"` `enum: "block" or "drop_newest"` |
| `buffer.max_size` | `int` | The maximum size of the buffer on the disk. Only relevant when type = "disk"<br />`no default` `example: 104900000` `unit: bytes` |
| `buffer.num_items` | `int` | The maximum number of [events][docs.event] allowed in the buffer. Only relevant when type = "memory"<br />`default: 500` `unit: events` |

## How It Works

### Delivery Guarantee

Due to the nature of this component, it offers a
[**best effort** delivery guarantee][docs.best_effort_delivery].

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

The `vector` sink streams data on a real-time
event-by-event basis. It does not batch data.

## Troubleshooting

The best place to start with troubleshooting is to check the
[Vector logs][docs.monitoring_logs]. This is typically located at
`/var/log/vector.log`, then proceed to follow the
[Troubleshooting Guide][docs.troubleshooting].

If the [Troubleshooting Guide][docs.troubleshooting] does not resolve your
issue, please:

1. Check for any [open `vector_sink` issues][url.vector_sink_issues].
2. If encountered a bug, please [file a bug report][url.new_vector_sink_bug].
3. If encountered a missing feature, please [file a feature request][url.new_vector_sink_enhancement].
4. If you need help, [join our chat/forum community][url.vector_chat]. You can post a question and search previous questions.

## Resources

* [**Issues**][url.vector_sink_issues] - [enhancements][url.vector_sink_enhancements] - [bugs][url.vector_sink_bugs]
* [**Source code**][url.vector_sink_source]


[docs.best_effort_delivery]: ../../../about/guarantees.md#best-effort-delivery
[docs.config_composition]: ../../../usage/configuration/README.md#composition
[docs.configuration.environment-variables]: ../../../usage/configuration#environment-variables
[docs.event]: ../../../about/data-model/README.md#event
[docs.log_event]: ../../../about/data-model/log.md
[docs.monitoring_logs]: ../../../usage/administration/monitoring.md#logs
[docs.sources]: ../../../usage/configuration/sources
[docs.transforms]: ../../../usage/configuration/transforms
[docs.troubleshooting]: ../../../usage/guides/troubleshooting.md
[images.vector_sink]: ../../../assets/vector-sink.svg
[url.new_vector_sink_bug]: https://github.com/timberio/vector/issues/new?labels=Sink%3A+vector&labels=Type%3A+Bug
[url.new_vector_sink_enhancement]: https://github.com/timberio/vector/issues/new?labels=Sink%3A+vector&labels=Type%3A+Enhancement
[url.vector_chat]: https://chat.vector.dev
[url.vector_sink_bugs]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Sink%3A+vector%22+label%3A%22Type%3A+Bug%22
[url.vector_sink_enhancements]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Sink%3A+vector%22+label%3A%22Type%3A+Enhancement%22
[url.vector_sink_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Sink%3A+vector%22
[url.vector_sink_source]: https://github.com/timberio/vector/tree/master/src/sinks/vector.rs
