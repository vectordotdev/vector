---
description: Streams `log` and `metric` events to the console, `STDOUT` or `STDERR`.
---

<!--
     THIS FILE IS AUTOOGENERATED!

     To make changes please edit the template located at:

     scripts/generate/templates/docs/usage/configuration/sinks/console.md.erb
-->

# console sink

![][images.console_sink]


The `console` sink [streams](#streaming) [`log`][docs.log_event] and [`metric`][docs.metric_event] events to the console, `STDOUT` or `STDERR`.

## Config File

{% code-tabs %}
{% code-tabs-item title="vector.toml (example)" %}
```coffeescript
[sinks.my_sink_id]
  type = "console" # must be: "console"
  inputs = ["my-source-id"]
  target = "stdout" # enum: "stdout" or "stderr"
  
  encoding = "json" # no default, enum: "json" or "text"
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (schema)" %}
```coffeescript
[sinks.<sink-id>]
  type = "console"
  inputs = ["<string>", ...]
  target = {"stdout" | "stderr"}
  encoding = {"json" | "text"}
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (specification)" %}
```coffeescript
[sinks.console_sink]
  # The component type
  # 
  # * required
  # * no default
  # * must be: "console"
  type = "console"

  # A list of upstream source or transform IDs. See Config Composition for more
  # info.
  # 
  # * required
  # * no default
  inputs = ["my-source-id"]

  # The standard stream to write to.
  # 
  # * required
  # * no default
  # * enum: "stdout" or "stderr"
  target = "stdout"
  target = "stderr"

  # The encoding format used to serialize the events before flushing. The default
  # is dynamic based on if the event is structured or not.
  # 
  # * optional
  # * no default
  # * enum: "json" or "text"
  encoding = "json"
  encoding = "text"
```
{% endcode-tabs-item %}
{% endcode-tabs %}

## Options

| Key  | Type  | Description |
|:-----|:-----:|:------------|
| **REQUIRED** | | |
| `type` | `string` | The component type<br />`required` `must be: "console"` |
| `inputs` | `[string]` | A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.config_composition] for more info.<br />`required` `example: ["my-source-id"]` |
| `target` | `string` | The [standard stream][url.standard_streams] to write to.<br />`required` `enum: "stdout" or "stderr"` |
| **OPTIONAL** | | |
| `encoding` | `string` | The encoding format used to serialize the events before flushing. The default is dynamic based on if the event is structured or not. See [Encodings](#encodings) for more info.<br />`no default` `enum: "json" or "text"` |

## How It Works

### Delivery Guarantee

Due to the nature of this component, it offers a
[**best effort** delivery guarantee][docs.best_effort_delivery].

### Encodings

The `console` sink encodes events before writing
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
it directly to the `console` sink. It is less
surprising that the outgoing data reflects the incoming data exactly since it
was not explicitly structured.

### Environment Variables

Environment variables are supported through all of Vector's configuration.
Simply add `${MY_ENV_VAR}` in your Vector configuration file and the variable
will be replaced before being evaluated.

You can learn more in the [Environment Variables][docs.configuration.environment-variables]
section.

### Health Checks

Upon [starting][docs.starting], Vector will perform a simple health check
against this sink. The ensures that the downstream service is healthy and
reachable.
By default, if the health check fails an error will be logged and
Vector will proceed to start. If you'd like to exit immediately upomn healt
check failure, you can pass the `--require-healthy` flag:

```bash
vector --config /etc/vector/vector.toml --require-healthy
```

Be careful when doing this, one unhealthy sink can prevent other healthy sinks
from processing data at all.

### Streaming

The `console` sink streams data on a real-time
event-by-event basis. It does not batch data.

## Troubleshooting

The best place to start with troubleshooting is to check the
[Vector logs][docs.monitoring_logs]. This is typically located at
`/var/log/vector.log`, then proceed to follow the
[Troubleshooting Guide][docs.troubleshooting].

If the [Troubleshooting Guide][docs.troubleshooting] does not resolve your
issue, please:

1. Check for any [open `console_sink` issues][url.console_sink_issues].
2. If encountered a bug, please [file a bug report][url.new_console_sink_bug].
3. If encountered a missing feature, please [file a feature request][url.new_console_sink_enhancement].
4. If you need help, [join our chat/forum community][url.vector_chat]. You can post a question and search previous questions.

## Resources

* [**Issues**][url.console_sink_issues] - [enhancements][url.console_sink_enhancements] - [bugs][url.console_sink_bugs]
* [**Source code**][url.console_sink_source]


[docs.best_effort_delivery]: ../../../about/guarantees.md#best-effort-delivery
[docs.config_composition]: ../../../usage/configuration/README.md#composition
[docs.configuration.environment-variables]: ../../../usage/configuration#environment-variables
[docs.log_event]: ../../../about/data-model/log.md
[docs.metric_event]: ../../../about/data-model/metric.md
[docs.monitoring_logs]: ../../../usage/administration/monitoring.md#logs
[docs.sources]: ../../../usage/configuration/sources
[docs.starting]: ../../../usage/administration/starting.md
[docs.tcp_source]: ../../../usage/configuration/sources/tcp.md
[docs.transforms]: ../../../usage/configuration/transforms
[docs.troubleshooting]: ../../../usage/guides/troubleshooting.md
[images.console_sink]: ../../../assets/console-sink.svg
[url.console_sink_bugs]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Sink%3A+console%22+label%3A%22Type%3A+Bug%22
[url.console_sink_enhancements]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Sink%3A+console%22+label%3A%22Type%3A+Enhancement%22
[url.console_sink_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Sink%3A+console%22
[url.console_sink_source]: https://github.com/timberio/vector/tree/master/src/sinks/console.rs
[url.new_console_sink_bug]: https://github.com/timberio/vector/issues/new?labels=Sink%3A+console&labels=Type%3A+Bug
[url.new_console_sink_enhancement]: https://github.com/timberio/vector/issues/new?labels=Sink%3A+console&labels=Type%3A+Enhancement
[url.standard_streams]: https://en.wikipedia.org/wiki/Standard_streams
[url.vector_chat]: https://chat.vector.dev
