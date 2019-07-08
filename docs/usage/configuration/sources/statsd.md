

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
The `statsd` sink is in beta. Please see the current
[enhancements][url.statsd_source_enhancements] and
[bugs][url.statsd_source_bugs] for known issues.
We kindly ask that you [add any missing issues][url.new_statsd_source_issues]
as it will help shape the roadmap of this component.
{% endhint %}

The `statsd` source ingests data through the StatsD UDP protocol and outputs [`log`][docs.log_event] events.

## Config File

{% code-tabs %}
{% code-tabs-item title="vector.toml (example)" %}
```toml
[sinks.my_statsd_source_id]
  # REQUIRED
  type = "statsd" # must be: "statsd"
  address = "127.0.0.1:8126"
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (schema)" %}
```toml
[sinks.<sink-id>]
  # REQUIRED
  type = "statsd"
  address = "<string>"
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (specification)" %}
```toml
[sinks.statsd]
  #
  # General
  #

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
| `type` | `string` | The component type<br />`required` `enum: "statsd"` |
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

## Troubleshooting

The best place to start with troubleshooting is to check the
[Vector logs][docs.monitoring_logs]. This is typically located at
`/var/log/vector.log`, then proceed to follow the
[Troubleshooting Guide][docs.troubleshooting].

If the [Troubleshooting Guide][docs.troubleshooting] does not resolve your
issue, please:

1. Check for any [open sink issues][url.statsd_source_issues].
2. [Search the forum][url.search_forum] for any similar issues.
2. Reach out to the [community][url.community] for help.

## Resources

* [**Issues**][url.statsd_source_issues] - [enhancements][url.statsd_source_enhancements] - [bugs][url.statsd_source_bugs]
* [**Source code**][url.statsd_source_source]