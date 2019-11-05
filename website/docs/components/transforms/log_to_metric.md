---

event_types: ["log","metric"]
issues_url: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22transform%3A+log_to_metric%22
sidebar_label: "log_to_metric|[\"log\",\"metric\"]"
source_url: https://github.com/timberio/vector/tree/master/src/transforms/log_to_metric.rs
status: "prod-ready"
title: "log_to_metric transform" 
---

The `log_to_metric` transform accepts [`log`][docs.data-model#log] events and allows you to convert logs into one or more metrics.

## Configuration

import CodeHeader from '@site/src/components/CodeHeader';
import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

<Tabs
  defaultValue="common"
  values={[
    { label: 'Common', value: 'common', },
    { label: 'Advanced', value: 'advanced', },
  ]
}>
<TabItem value="common">

<CodeHeader fileName="vector.toml" learnMoreUrl="/setup/configuration"/ >

```toml
[transforms.my_transform_id]
  # REQUIRED - General
  type = "log_to_metric" # example, must be: "log_to_metric"
  inputs = ["my-source-id"] # example
  
  # REQUIRED - Metrics
  [[transforms.my_transform_id.metrics]]
    # REQUIRED
    type = "counter" # example, enum
    field = "duration" # example
    name = "duration_total" # example
    
    # OPTIONAL
    [transforms.my_transform_id.metrics.tags]
      host = "${HOSTNAME}" # example
      region = "us-east-1" # example
      status = "{{status}}" # example
```

</TabItem>
<TabItem value="advanced">

<CodeHeader fileName="vector.toml" learnMoreUrl="/setup/configuration" />

```toml
[transforms.my_transform_id]
  # REQUIRED - General
  type = "log_to_metric" # example, must be: "log_to_metric"
  inputs = ["my-source-id"] # example
  
  # REQUIRED - Metrics
  [[transforms.my_transform_id.metrics]]
    # REQUIRED
    type = "counter" # example, enum
    field = "duration" # example
    name = "duration_total" # example
    
    # OPTIONAL
    increment_by_value = true # default, relevant when type = "counter"
    [transforms.my_transform_id.metrics.tags]
      host = "${HOSTNAME}" # example
      region = "us-east-1" # example
      status = "{{status}}" # example
```

</TabItem>

</Tabs>

## Options

import Field from '@site/src/components/Field';
import Fields from '@site/src/components/Fields';

<Fields filters={true}>


<Field
  common={true}
  defaultValue={null}
  enumValues={null}
  examples={[]}
  name={"metrics"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={true}
  type={"[table]"}
  unit={null}>

### metrics

A table of key/value pairs representing the keys to be added to the event.

<Fields filters={false}>


<Field
  common={true}
  defaultValue={null}
  enumValues={{"counter":"A [counter metric type][docs.data-model#counters].","gauge":"A [gauge metric type][docs.data-model#gauges].","histogram":"A [histogram metric type][docs.data-model#histograms].","set":"A [set metric type][docs.data-model#sets]."}}
  examples={["counter","gauge","histogram","set"]}
  name={"type"}
  nullable={false}
  path={"metrics"}
  relevantWhen={null}
  required={true}
  type={"string"}
  unit={null}>

#### type

The metric type.


</Field>


<Field
  common={true}
  defaultValue={null}
  enumValues={null}
  examples={["duration"]}
  name={"field"}
  nullable={false}
  path={"metrics"}
  relevantWhen={null}
  required={true}
  type={"string"}
  unit={null}>

#### field

The log field to use as the metric. See [Null Fields](#null-fields) for more info.


</Field>


<Field
  common={false}
  defaultValue={false}
  enumValues={null}
  examples={[true,false]}
  name={"increment_by_value"}
  nullable={false}
  path={"metrics"}
  relevantWhen={{"type":"counter"}}
  required={false}
  type={"bool"}
  unit={null}>

#### increment_by_value

If `true` the metric will be incremented by the `field` value. If `false` the metric will be incremented by 1 regardless of the `field` value.


</Field>


<Field
  common={true}
  defaultValue={null}
  enumValues={null}
  examples={["duration_total"]}
  name={"name"}
  nullable={false}
  path={"metrics"}
  relevantWhen={null}
  required={true}
  type={"string"}
  unit={null}>

#### name

The name of the metric. Defaults to `<field>_total` for `counter` and `<field>` for `gauge`.


</Field>


<Field
  common={true}
  defaultValue={null}
  enumValues={null}
  examples={[]}
  name={"tags"}
  nullable={true}
  path={"metrics"}
  relevantWhen={null}
  required={false}
  type={"table"}
  unit={null}>

#### tags

Key/value pairs representing [metric tags][docs.data-model#tags].

<Fields filters={false}>


<Field
  common={true}
  defaultValue={null}
  enumValues={null}
  examples={[{"name":"host","value":"${HOSTNAME}"},{"name":"region","value":"us-east-1"},{"name":"status","value":"{{status}}"}]}
  name={"*"}
  nullable={false}
  path={"metrics.tags"}
  relevantWhen={null}
  required={true}
  type={"string"}
  unit={null}>

##### *

Key/value pairs representing [metric tags][docs.data-model#tags]. Environment variables and field interpolation is allowed.


</Field>


</Fields>

</Field>


</Fields>

</Field>


</Fields>

## Input/Output

{% tabs %}
{% tab title="Timings" %}
This example demonstrates capturing timings in your logs.

{% code-tabs %}
{% code-tabs-item title="log" %}
```json
{
  "host": "10.22.11.222",
  "message": "Sent 200 in 54.2ms",
  "status": 200,
  "time": 54.2,
}
```
{% endcode-tabs-item %}
{% endcode-tabs %}

You can convert the `time` field into a `histogram` metric:

{% code-tabs %}
{% code-tabs-item title="vector.toml" %}
```toml
[transforms.log_to_metric]
  type = "log_to_metric"
  
  [[transforms.log_to_metric.metrics]]
    type = "histogram"
    field = "time"
    name = "time_ms" # optional
    tags.status = "{{status}}" # optional
    tags.host = "{{host}}" # optional
```
{% endcode-tabs-item %}
{% endcode-tabs %}

A [`metric` event][docs.data-model#metric] will be output with the following
structure:

```javascript
{
  "histogram": {
    "name": "time_ms",
    "val": 52.2,
    "smaple_rate": 1,
    "tags": {
      "status": "200",
      "host": "10.22.11.222"
    }
  }
}
```

This metric will then proceed down the pipeline, and depending on the sink,
will be aggregated in Vector (such is the case for the [`prometheus` \
sink][docs.sinks.prometheus]) or will be aggregated in the store itself.

{% endtab %}
{% tab title="Counting" %}
This example demonstrates counting HTTP status codes.

Given the following log line:

{% code-tabs %}
{% code-tabs-item title="log" %}
```json
{
  "host": "10.22.11.222",
  "message": "Sent 200 in 54.2ms",
  "status": 200
}
```
{% endcode-tabs-item %}
{% endcode-tabs %}

You can count the number of responses by status code:

{% code-tabs %}
{% code-tabs-item title="vector.toml" %}
```toml
[transforms.log_to_metric]
  type = "log_to_metric"
  
  [[transforms.log_to_metric.metrics]]
    type = "counter"
    field = "status"
    name = "response_total" # optional
    tags.status = "{{status}}"
    tags.host = "{{host}}"
```
{% endcode-tabs-item %}
{% endcode-tabs %}

A [`metric` event][docs.data-model#metric] will be output with the following
structure:

```javascript
{
  "counter": {
    "name": "response_total",
    "val": 1.0,
    "tags": {
      "status": "200",
      "host": "10.22.11.222"
    }
  }
}
```

This metric will then proceed down the pipeline, and depending on the sink,
will be aggregated in Vector (such is the case for the [`prometheus` \
sink][docs.sinks.prometheus]) or will be aggregated in the store itself.
{% endtab %}
{% tab title="Summing" %}
In this example we'll demonstrate computing a sum. The scenario we've chosen
is to compute the total of orders placed.

Given the following log line:

{% code-tabs %}
{% code-tabs-item title="log" %}
```json
{
  "host": "10.22.11.222",
  "message": "Order placed for $122.20",
  "total": 122.2
}
```
{% endcode-tabs-item %}
{% endcode-tabs %}

You can reduce this log into a `counter` metric that increases by the
field's value:

{% code-tabs %}
{% code-tabs-item title="vector.toml" %}
```toml
[transforms.log_to_metric]
  type = "log_to_metric"
  
  [[transforms.log_to_metric.metrics]]
    type = "counter"
    field = "total"
    name = "order_total" # optional
    increment_by_value = true # optional
    tags.host = "{{host}}" # optional
```
{% endcode-tabs-item %}
{% endcode-tabs %}

A [`metric` event][docs.data-model#metric] will be output with the following
structure:

```javascript
{
  "counter": {
    "name": "order_total",
    "val": 122.20,
    "tags": {
      "host": "10.22.11.222"
    }
  }
}
```

This metric will then proceed down the pipeline, and depending on the sink,
will be aggregated in Vector (such is the case for the [`prometheus` \
sink][docs.sinks.prometheus]) or will be aggregated in the store itself.
{% endtab %}
{% tab title="Gauges" %}
In this example we'll demonstrate creating a gauge that represents the current
CPU load verages.

Given the following log line:

{% code-tabs %}
{% code-tabs-item title="log" %}
```json
{
  "host": "10.22.11.222",
  "message": "CPU activity sample",
  "1m_load_avg": 78.2,
  "5m_load_avg": 56.2,
  "15m_load_avg": 48.7
}
```
{% endcode-tabs-item %}
{% endcode-tabs %}

You can reduce this logs into multiple `gauge` metrics:

{% code-tabs %}
{% code-tabs-item title="vector.toml" %}
```toml
[transforms.log_to_metric]
  type = "log_to_metric"
  
  [[transforms.log_to_metric.metrics]]
    type = "gauge"
    field = "1m_load_avg"
    tags.host = "{{host}}" # optional

  [[transforms.log_to_metric.metrics]]
    type = "gauge"
    field = "5m_load_avg"
    tags.host = "{{host}}" # optional

  [[transforms.log_to_metric.metrics]]
    type = "gauge"
    field = "15m_load_avg"
    tags.host = "{{host}}" # optional
```
{% endcode-tabs-item %}
{% endcode-tabs %}

Multiple [`metric` events][docs.data-model#metric] will be output with the following
structure:

```javascript
[
  {
    "gauge": {
      "name": "1m_load_avg",
      "val": 78.2,
      "tags": {
        "host": "10.22.11.222"
      }
    }
  },
  {
    "gauge": {
      "name": "5m_load_avg",
      "val": 56.2,
      "tags": {
        "host": "10.22.11.222"
      }
    }
  },
  {
    "gauge": {
      "name": "15m_load_avg",
      "val": 48.7,
      "tags": {
        "host": "10.22.11.222"
      }
    }
  }
]
```

This metric will then proceed down the pipeline, and depending on the sink,
will be aggregated in Vector (such is the case for the [`prometheus` \
sink][docs.sinks.prometheus]) or will be aggregated in the store itself.
{% endtab %}
{% tab title="Sets" %}
In this example we'll demonstrate how to use sets. Sets are primarly a Statsd
concept that represent the number of unique values seens for a given metric.
The idea is that you pass the unique/high-cardinality value as the metric value
and the metric store will count the number of unique values seen.

For example, given the following log line:

{% code-tabs %}
{% code-tabs-item title="log" %}
```json
{
  "host": "10.22.11.222",
  "message": "Sent 200 in 54.2ms",
  "remote_addr": "233.221.232.22"
}
```
{% endcode-tabs-item %}
{% endcode-tabs %}

You can count the number of unique `remote_addr` values by using a set:

{% code-tabs %}
{% code-tabs-item title="vector.toml" %}
```toml
[transforms.log_to_metric]
  type = "log_to_metric"
  
  [[transforms.log_to_metric.metrics]]
    type = "set"
    field = "remote_addr"
    tags.host = "{{host}}" # optional
```
{% endcode-tabs-item %}
{% endcode-tabs %}

A [`metric` event][docs.data-model#metric] will be output with the following
structure:

```javascript
{
  "set": {
    "name": "remote_addr",
    "val": "233.221.232.22",
    "tags": {
      "host": "10.22.11.222"
    }
  }
}
```

This metric will then proceed down the pipeline, and depending on the sink,
will be aggregated in Vector (such is the case for the [`prometheus` \
sink][docs.sinks.prometheus]) or will be aggregated in the store itself.
{% endtab %}
{% endtabs %}

## How It Works

### Environment Variables

Environment variables are supported through all of Vector's configuration.
Simply add `${MY_ENV_VAR}` in your Vector configuration file and the variable
will be replaced before being evaluated.

You can learn more in the [Environment Variables][docs.configuration#environment-variables]
section.

### Multiple Metrics

For clarification, when you convert a single `log` event into multiple `metric`
events, the `metric` events are not emitted as a single array. They are emitted
individually, and the downstream components treat them as individual events.
Downstream components are not aware they were derived from a single log event.

### Null Fields

If the target log `field` contains a `null` value it will ignored, and a metric
will not be emitted.

### Reducing

It's important to understand that this transform does not reduce multiple logs
into a single metric. Instead, this transform converts logs into granular
individual metrics that can then be reduced at the edge. Where the reduction
happens depends on your metrics storage. For example, the
[`prometheus` sink][docs.sinks.prometheus] will reduce logs in the sink itself
for the next scrape, while other metrics sinks will proceed to forward the
individual metrics for reduction in the metrics storage itself.


[docs.configuration#environment-variables]: ../../setup/configuration#environment-variables
[docs.data-model#counters]: ../../about/data-model#counters
[docs.data-model#gauges]: ../../about/data-model#gauges
[docs.data-model#histograms]: ../../about/data-model#histograms
[docs.data-model#log]: ../../about/data-model#log
[docs.data-model#metric]: ../../about/data-model#metric
[docs.data-model#sets]: ../../about/data-model#sets
[docs.data-model#tags]: ../../about/data-model#tags
[docs.sinks.prometheus]: ../../components/sinks/prometheus
