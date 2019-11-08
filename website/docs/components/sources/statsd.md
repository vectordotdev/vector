---
delivery_guarantee: "best_effort"
event_types: ["metric"]
issues_url: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22source%3A+statsd%22
sidebar_label: "statsd|[\"metric\"]"
source_url: https://github.com/timberio/vector/tree/master/src/sources/statsd/mod.rs
status: "beta"
title: "statsd source" 
---

The `statsd` source ingests data through the StatsD UDP protocol and outputs [`metric`][docs.data-model#metric] events.

## Configuration

import CodeHeader from '@site/src/components/CodeHeader';

<CodeHeader fileName="vector.toml" learnMoreUrl="/setup/configuration"/ >

```toml
[sources.my_source_id]
  type = "statsd" # example, must be: "statsd"
  address = "127.0.0.1:8126" # example
```

## Options

import Fields from '@site/src/components/Fields';

import Field from '@site/src/components/Field';

<Fields filters={true}>


<Field
  common={true}
  defaultValue={null}
  enumValues={null}
  examples={["127.0.0.1:8126"]}
  name={"address"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={true}
  templateable={false}
  type={"string"}
  unit={null}
  >

### address

UDP socket address to bind to.


</Field>


</Fields>

## Output

This component does not automatically add any fields.

## Input/Output

{% tabs %}
{% tab title="Counter" %}
Given the following Statsd counter:

```
login.invocations:1|c
```

A [`metric` event][docs.data-model#metric] will be output with the following structure:

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

A [`metric` event][docs.data-model#metric] will be output with the following structure:

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

A [`metric` event][docs.data-model#metric] will be output with the following structure:

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

A [`metric` event][docs.data-model#metric] will be output with the following structure:

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
the [metric][docs.data-model#metric] data model page for more info.


[docs.configuration#environment-variables]: ../../setup/configuration#environment-variables
[docs.data-model#metric]: ../../about/data-model#metric
