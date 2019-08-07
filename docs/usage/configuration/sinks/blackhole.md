---
description: Streams `log` and `metric` events to a blackhole that simply discards data, designed for testing and benchmarking purposes.
---

<!--
     THIS FILE IS AUTOOGENERATED!

     To make changes please edit the template located at:

     scripts/generate/templates/docs/usage/configuration/sinks/blackhole.md.erb
-->

# blackhole sink

![][images.blackhole_sink]


The `blackhole` sink [streams](#streaming) [`log`][docs.log_event] and [`metric`][docs.metric_event] events to a blackhole that simply discards data, designed for testing and benchmarking purposes.

## Config File

{% code-tabs %}
{% code-tabs-item title="vector.toml (example)" %}
```coffeescript
[sinks.my_sink_id]
  type = "blackhole" # must be: "blackhole"
  inputs = ["my-source-id"]
  print_amount = 1000
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (schema)" %}
```coffeescript
[sinks.<sink-id>]
  type = "blackhole"
  inputs = ["<string>", ...]
  print_amount = <int>
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (specification)" %}
```coffeescript
[sinks.blackhole_sink]
  # The component type
  # 
  # * required
  # * no default
  # * must be: "blackhole"
  type = "blackhole"

  # A list of upstream source or transform IDs. See Config Composition for more
  # info.
  # 
  # * required
  # * no default
  inputs = ["my-source-id"]

  # The number of events that must be received in order to print a summary of
  # activity.
  # 
  # * required
  # * no default
  print_amount = 1000
```
{% endcode-tabs-item %}
{% endcode-tabs %}

## Options

| Key  | Type  | Description |
|:-----|:-----:|:------------|
| **REQUIRED** | | |
| `type` | `string` | The component type<br />`required` `must be: "blackhole"` |
| `inputs` | `[string]` | A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.config_composition] for more info.<br />`required` `example: ["my-source-id"]` |
| `print_amount` | `int` | The number of events that must be received in order to print a summary of activity.<br />`required` `example: 1000` |

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

The `blackhole` sink streams data on a real-time
event-by-event basis. It does not batch data.

## Troubleshooting

The best place to start with troubleshooting is to check the
[Vector logs][docs.monitoring_logs]. This is typically located at
`/var/log/vector.log`, then proceed to follow the
[Troubleshooting Guide][docs.troubleshooting].

If the [Troubleshooting Guide][docs.troubleshooting] does not resolve your
issue, please:

1. Check for any [open `blackhole_sink` issues][url.blackhole_sink_issues].
2. If encountered a bug, please [file a bug report][url.new_blackhole_sink_bug].
3. If encountered a missing feature, please [file a feature request][url.new_blackhole_sink_enhancement].
4. If you need help, [join our chat/forum community][url.vector_chat]. You can post a question and search previous questions.

## Resources

* [**Issues**][url.blackhole_sink_issues] - [enhancements][url.blackhole_sink_enhancements] - [bugs][url.blackhole_sink_bugs]
* [**Source code**][url.blackhole_sink_source]


[docs.best_effort_delivery]: ../../../about/guarantees.md#best-effort-delivery
[docs.config_composition]: ../../../usage/configuration/README.md#composition
[docs.configuration.environment-variables]: ../../../usage/configuration#environment-variables
[docs.log_event]: ../../../about/data-model/log.md
[docs.metric_event]: ../../../about/data-model/metric.md
[docs.monitoring_logs]: ../../../usage/administration/monitoring.md#logs
[docs.sources]: ../../../usage/configuration/sources
[docs.starting]: ../../../usage/administration/starting.md
[docs.transforms]: ../../../usage/configuration/transforms
[docs.troubleshooting]: ../../../usage/guides/troubleshooting.md
[images.blackhole_sink]: ../../../assets/blackhole-sink.svg
[url.blackhole_sink_bugs]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Sink%3A+blackhole%22+label%3A%22Type%3A+Bug%22
[url.blackhole_sink_enhancements]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Sink%3A+blackhole%22+label%3A%22Type%3A+Enhancement%22
[url.blackhole_sink_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Sink%3A+blackhole%22
[url.blackhole_sink_source]: https://github.com/timberio/vector/tree/master/src/sinks/blackhole.rs
[url.new_blackhole_sink_bug]: https://github.com/timberio/vector/issues/new?labels=Sink%3A+blackhole&labels=Type%3A+Bug
[url.new_blackhole_sink_enhancement]: https://github.com/timberio/vector/issues/new?labels=Sink%3A+blackhole&labels=Type%3A+Enhancement
[url.vector_chat]: https://chat.vector.dev
