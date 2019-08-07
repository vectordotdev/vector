---
description: Ingests data through another upstream Vector instance and outputs `log` events.
---

<!--
     THIS FILE IS AUTOOGENERATED!

     To make changes please edit the template located at:

     scripts/generate/templates/docs/usage/configuration/sources/vector.md.erb
-->

# vector source

![][images.vector_source]

{% hint style="warning" %}
The `vector` source is in beta. Please see the current
[enhancements][url.vector_source_enhancements] and
[bugs][url.vector_source_bugs] for known issues.
We kindly ask that you [add any missing issues][url.new_vector_source_issue]
as it will help shape the roadmap of this component.
{% endhint %}

The `vector` source ingests data through another upstream Vector instance and outputs [`log`][docs.log_event] events.

## Config File

{% code-tabs %}
{% code-tabs-item title="vector.toml (example)" %}
```coffeescript
[sources.my_source_id]
  type = "vector" # must be: "vector"
  address = "0.0.0.0:9000"
  
  shutdown_timeout_secs = 30 # default, seconds
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (schema)" %}
```coffeescript
[sources.<source-id>]
  type = "vector"
  address = "<string>"
  shutdown_timeout_secs = <int>
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (specification)" %}
```coffeescript
[sources.vector_source]
  # The component type
  # 
  # * required
  # * no default
  # * must be: "vector"
  type = "vector"

  # The TCP address to bind to.
  # 
  # * required
  # * no default
  address = "0.0.0.0:9000"

  # The timeout before a connection is forcefully closed during shutdown.
  # 
  # * optional
  # * default: 30
  # * unit: seconds
  shutdown_timeout_secs = 30
```
{% endcode-tabs-item %}
{% endcode-tabs %}

## Options

| Key  | Type  | Description |
|:-----|:-----:|:------------|
| **REQUIRED** | | |
| `type` | `string` | The component type<br />`required` `must be: "vector"` |
| `address` | `string` | The TCP address to bind to.<br />`required` `example: "0.0.0.0:9000"` |
| **OPTIONAL** | | |
| `shutdown_timeout_secs` | `int` | The timeout before a connection is forcefully closed during shutdown.<br />`default: 30` `unit: seconds` |

## How It Works

### Delivery Guarantee

Due to the nature of this component, it offers a
[**best effort** delivery guarantee][docs.best_effort_delivery].

### Encoding

Data is encoded via Vector's [event protobuf][url.event_proto] before it is sent over the wire.

### Environment Variables

Environment variables are supported through all of Vector's configuration.
Simply add `${MY_ENV_VAR}` in your Vector configuration file and the variable
will be replaced before being evaluated.

You can learn more in the [Environment Variables][docs.configuration.environment-variables]
section.

### Message Acking

Currently, Vector does not perform any application level message acknowledgement. While rare, this means the individual message could be lost.

### TCP Protocol

Upstream Vector instances forward data to downstream Vector instances via the TCP protocol.

## Troubleshooting

The best place to start with troubleshooting is to check the
[Vector logs][docs.monitoring_logs]. This is typically located at
`/var/log/vector.log`, then proceed to follow the
[Troubleshooting Guide][docs.troubleshooting].

If the [Troubleshooting Guide][docs.troubleshooting] does not resolve your
issue, please:

1. Check for any [open `vector_source` issues][url.vector_source_issues].
2. If encountered a bug, please [file a bug report][url.new_vector_source_bug].
3. If encountered a missing feature, please [file a feature request][url.new_vector_source_enhancement].
4. If you need help, [join our chat/forum community][url.vector_chat]. You can post a question and search previous questions.

## Resources

* [**Issues**][url.vector_source_issues] - [enhancements][url.vector_source_enhancements] - [bugs][url.vector_source_bugs]
* [**Source code**][url.vector_source_source]


[docs.best_effort_delivery]: ../../../about/guarantees.md#best-effort-delivery
[docs.configuration.environment-variables]: ../../../usage/configuration#environment-variables
[docs.log_event]: ../../../about/data-model/log.md
[docs.monitoring_logs]: ../../../usage/administration/monitoring.md#logs
[docs.troubleshooting]: ../../../usage/guides/troubleshooting.md
[images.vector_source]: ../../../assets/vector-source.svg
[url.event_proto]: https://github.com/timberio/vector/blob/master/proto/event.proto
[url.new_vector_source_bug]: https://github.com/timberio/vector/issues/new?labels=Source%3A+vector&labels=Type%3A+Bug
[url.new_vector_source_enhancement]: https://github.com/timberio/vector/issues/new?labels=Source%3A+vector&labels=Type%3A+Enhancement
[url.new_vector_source_issue]: https://github.com/timberio/vector/issues/new?labels=Source%3A+vector
[url.vector_chat]: https://chat.vector.dev
[url.vector_source_bugs]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Source%3A+vector%22+label%3A%22Type%3A+Bug%22
[url.vector_source_enhancements]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Source%3A+vector%22+label%3A%22Type%3A+Enhancement%22
[url.vector_source_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Source%3A+vector%22
[url.vector_source_source]: https://github.com/timberio/vector/tree/master/src/sources/vector.rs
