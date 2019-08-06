---
description: Ingests data through the StatsD UDP protocol and outputs `log` events.
---

<!--
     THIS FILE IS AUTOOGENERATED!

     To make changes please edit the template located at:

     scripts/generate/templates/docs/usage/configuration/sources/statsd.md.erb
-->

# statsd source

![][images.statsd_source]

{% hint style="warning" %}
The `statsd` source is in beta. Please see the current
[enhancements][url.statsd_source_enhancements] and
[bugs][url.statsd_source_bugs] for known issues.
We kindly ask that you [add any missing issues][url.new_statsd_source_issue]
as it will help shape the roadmap of this component.
{% endhint %}

The `statsd` source ingests data through the StatsD UDP protocol and outputs [`log`][docs.log_event] events.

## Config File

{% code-tabs %}
{% code-tabs-item title="vector.toml (example)" %}
```coffeescript
[sources.my_source_id]
  type = "statsd" # must be: "statsd"
  address = "127.0.0.1:8126"
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (schema)" %}
```coffeescript
[sources.<source-id>]
  type = "statsd"
  address = "<string>"
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (specification)" %}
```coffeescript
[sources.statsd_source]
  # The component type
  # 
  # * required
  # * no default
  # * must be: "statsd"
  type = "statsd"

  # UDP socket address to bind to.
  # 
  # * required
  # * no default
  address = "127.0.0.1:8126"
```
{% endcode-tabs-item %}
{% endcode-tabs %}

## Options

| Key  | Type  | Description |
|:-----|:-----:|:------------|
| **REQUIRED** | | |
| `type` | `string` | The component type<br />`required` `must be: "statsd"` |
| `address` | `string` | UDP socket address to bind to.<br />`required` `example: "127.0.0.1:8126"` |

## Examples

{% tabs %}
{% tab title="Counter" %}
Given the following Statsd counter:

```
login.invocations:1|c
```

A [`metric` event][docs.metric_event] will be emitted with the following structure:

{% code-tabs %}
{% code-tabs-item title="metric" %}
```javascript
{
  "counter": {
    "name": "login.invocations",
    "val": 1
  }
}
```
{% endcode-tabs-item %}
{% endcode-tabs %}

{% endtab %}
{% tab title="Gauge" %}
Given the following Statsd gauge:

```
gas_tank:0.50|g
```

A [`metric` event][docs.metric_event] will be emitted with the following structure:

{% code-tabs %}
{% code-tabs-item title="metric" %}
```javascript
{
  "gauge": {
    "name": "gas_tank",
    "val": 0.5
  }
}
```
{% endcode-tabs-item %}
{% endcode-tabs %}

{% endtab %}
{% tab title="Set" %}
Given the following Statsd set:

```
unique_users:foo|s
```

A [`metric` event][docs.metric_event] will be emitted with the following structure:

{% code-tabs %}
{% code-tabs-item title="metric" %}
```javascript
{
  "set": {
    "name": "unique_users",
    "val": 1
  }
}
```
{% endcode-tabs-item %}
{% endcode-tabs %}

{% endtab %}
{% tab title="Timer" %}
Given the following Statsd timer:

```
login.time:22|ms 
```

A [`metric` event][docs.metric_event] will be emitted with the following structure:

{% code-tabs %}
{% code-tabs-item title="metric" %}
```javascript
{
  "timer": {
    "name": "login.time",
    "val": 22
  }
}
```
{% endcode-tabs-item %}
{% endcode-tabs %}

{% endtab %}
{% endtabs %}

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

## Troubleshooting

The best place to start with troubleshooting is to check the
[Vector logs][docs.monitoring_logs]. This is typically located at
`/var/log/vector.log`, then proceed to follow the
[Troubleshooting Guide][docs.troubleshooting].

If the [Troubleshooting Guide][docs.troubleshooting] does not resolve your
issue, please:

1. Check for any [open `statsd_source` issues][url.statsd_source_issues].
2. If encountered a bug, please [file a bug report][url.new_statsd_source_bug].
3. If encountered a missing feature, please [file a feature request][url.new_statsd_source_enhancement].
4. If you need help, [join our chat/forum community][url.vector_chat]. You can post a question and search previous questions.

## Resources

* [**Issues**][url.statsd_source_issues] - [enhancements][url.statsd_source_enhancements] - [bugs][url.statsd_source_bugs]
* [**Source code**][url.statsd_source_source]


[docs.best_effort_delivery]: ../../../about/guarantees.md#best-effort-delivery
[docs.configuration.environment-variables]: ../../../usage/configuration#environment-variables
[docs.log_event]: ../../../about/data-model/log.md
[docs.metric_event]: ../../../about/data-model/metric.md
[docs.monitoring_logs]: ../../../usage/administration/monitoring.md#logs
[docs.troubleshooting]: ../../../usage/guides/troubleshooting.md
[images.statsd_source]: ../../../assets/statsd-source.svg
[url.new_statsd_source_bug]: https://github.com/timberio/vector/issues/new?labels=Source%3A+statsd&labels=Type%3A+Bug
[url.new_statsd_source_enhancement]: https://github.com/timberio/vector/issues/new?labels=Source%3A+statsd&labels=Type%3A+Enhancement
[url.new_statsd_source_issue]: https://github.com/timberio/vector/issues/new?labels=Source%3A+statsd
[url.statsd_source_bugs]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Source%3A+statsd%22+label%3A%22Type%3A+Bug%22
[url.statsd_source_enhancements]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Source%3A+statsd%22+label%3A%22Type%3A+Enhancement%22
[url.statsd_source_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Source%3A+statsd%22
[url.statsd_source_source]: https://github.com/timberio/vector/tree/master/src/sources/statsd/mod.rs
[url.vector_chat]: https://chat.vector.dev
