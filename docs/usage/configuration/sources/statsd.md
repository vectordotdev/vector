---
title: "statsd source" 
sidebar_label: "statsd"
---

The `statsd` source ingests data through the StatsD UDP protocol and outputs [`metric`][docs.data-model.metric] events.

## Example

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';


```coffeescript
[sources.my_source_id]
  type = "statsd" # enum
  address = "127.0.0.1:8126"
```



You can learn more

## Options

import Option from '@site/src/components/Option';
import Options from '@site/src/components/Options';

<Options filters={true}>


<Option
  defaultValue={null}
  enumValues={null}
  examples={["127.0.0.1:8126"]}
  name={"address"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={true}
  simple={true}
  type={"string"}
  unit={null}>

### address

UDP socket address to bind to.


</Option>


</Options>

## Input/Output

{% tabs %}
{% tab title="Counter" %}
Given the following Statsd counter:

```
login.invocations:1|c
```

A [`metric` event][docs.data-model.metric] will be output with the following structure:

{% code-tabs %}
{% code-tabs-item title="metric" %}
```javascript
{
  "counter": {
    "name": "login.invocations",
    "val": 1,
    "timestamp": "2019-05-02T12:22:46.658503Z" // current time / time ingested
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

A [`metric` event][docs.data-model.metric] will be output with the following structure:

{% code-tabs %}
{% code-tabs-item title="metric" %}
```javascript
{
  "gauge": {
    "name": "gas_tank",
    "val": 0.5,
    "timestamp": "2019-05-02T12:22:46.658503Z" // current time / time ingested
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

A [`metric` event][docs.data-model.metric] will be output with the following structure:

{% code-tabs %}
{% code-tabs-item title="metric" %}
```javascript
{
  "set": {
    "name": "unique_users",
    "val": 1,
    "timestamp": "2019-05-02T12:22:46.658503Z" // current time / time ingested
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

A [`metric` event][docs.data-model.metric] will be output with the following structure:

{% code-tabs %}
{% code-tabs-item title="metric" %}
```javascript
{
  "timer": {
    "name": "login.time",
    "val": 22,
    "timestamp": "2019-05-02T12:22:46.658503Z" // current time / time ingested
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
[**best effort** delivery guarantee][docs.guarantees#best-effort-delivery].

### Environment Variables

Environment variables are supported through all of Vector's configuration.
Simply add `${MY_ENV_VAR}` in your Vector configuration file and the variable
will be replaced before being evaluated.

You can learn more in the [Environment Variables][docs.configuration#environment-variables]
section.

### Timestamp

You'll notice that each metric contains a `timestamp` field. This is an optional
descriptive field that represents when the metric was received. It helps to
more closely represent the metric's time in situations here it can be used. See
the [metric][docs.data-model.metric] data model page for more info.

## Troubleshooting

The best place to start with troubleshooting is to check the
[Vector logs][docs.monitoring#logs]. This is typically located at
`/var/log/vector.log`, then proceed to follow the
[Troubleshooting Guide][docs.troubleshooting].

If the [Troubleshooting Guide][docs.troubleshooting] does not resolve your
issue, please:

1. Check for any [open `statsd_source` issues][urls.statsd_source_issues].
2. If encountered a bug, please [file a bug report][urls.new_statsd_source_bug].
3. If encountered a missing feature, please [file a feature request][urls.new_statsd_source_enhancement].
4. If you need help, [join our chat/forum community][urls.vector_chat]. You can post a question and search previous questions.

## Resources

* [**Issues**][urls.statsd_source_issues] - [enhancements][urls.statsd_source_enhancements] - [bugs][urls.statsd_source_bugs]
* [**Source code**][urls.statsd_source_source]


[docs.configuration#environment-variables]: ../../../usage/configuration#environment-variables
[docs.data-model.metric]: ../../../about/data-model/metric.md
[docs.guarantees#best-effort-delivery]: ../../../about/guarantees.md#best-effort-delivery
[docs.monitoring#logs]: ../../../usage/administration/monitoring.md#logs
[docs.troubleshooting]: ../../../usage/guides/troubleshooting.md
[urls.new_statsd_source_bug]: https://github.com/timberio/vector/issues/new?labels=source%3A+statsd&labels=Type%3A+bug
[urls.new_statsd_source_enhancement]: https://github.com/timberio/vector/issues/new?labels=source%3A+statsd&labels=Type%3A+enhancement
[urls.statsd_source_bugs]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22source%3A+statsd%22+label%3A%22Type%3A+bug%22
[urls.statsd_source_enhancements]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22source%3A+statsd%22+label%3A%22Type%3A+enhancement%22
[urls.statsd_source_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22source%3A+statsd%22
[urls.statsd_source_source]: https://github.com/timberio/vector/tree/master/src/sources/statsd/mod.rs
[urls.vector_chat]: https://chat.vector.dev
