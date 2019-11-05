---
delivery_guarantee: "best_effort"
event_types: ["metric"]
issues_url: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22sink%3A+statsd%22
sidebar_label: "statsd|[\"metric\"]"
source_url: https://github.com/timberio/vector/tree/master/src/sinks/statsd.rs
status: "beta"
title: "statsd sink" 
---

The `statsd` sink [streams](#streaming) [`metric`][docs.data-model#metric] events to [StatsD][urls.statsd] metrics service.

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
[sinks.my_sink_id]
  type = "statsd" # example, must be: "statsd"
  inputs = ["my-source-id"] # example
  namespace = "service" # example
```

</TabItem>
<TabItem value="advanced">

<CodeHeader fileName="vector.toml" learnMoreUrl="/setup/configuration" />

```toml
[sinks.my_sink_id]
  # REQUIRED
  type = "statsd" # example, must be: "statsd"
  inputs = ["my-source-id"] # example
  namespace = "service" # example
  
  # OPTIONAL
  address = "127.0.0.1:8125" # default
  healthcheck = true # default
```

</TabItem>

</Tabs>

## Options

import Field from '@site/src/components/Field';
import Fields from '@site/src/components/Fields';

<Fields filters={true}>


<Field
  common={false}
  defaultValue={"127.0.0.1:8125"}
  enumValues={null}
  examples={["127.0.0.1:8125"]}
  name={"address"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  type={"string"}
  unit={null}>

### address

The UDP socket address to send stats to.


</Field>


<Field
  common={false}
  defaultValue={true}
  enumValues={null}
  examples={[true,false]}
  name={"healthcheck"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  type={"bool"}
  unit={null}>

### healthcheck

Enables/disables the sink healthcheck upon start.


</Field>


<Field
  common={true}
  defaultValue={null}
  enumValues={null}
  examples={["service"]}
  name={"namespace"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={true}
  type={"string"}
  unit={null}>

### namespace

A prefix that will be added to all metric names.


</Field>


</Fields>

## Input/Output

The `statsd` sink batches [`metric`][docs.data-model#metric] up to the `batch_size` or `batch_timeout` options. When flushed, metrics will be written in [Multi-metric format][urls.statsd_multi]. For example:

```
gorets:1|c\nglork:320|ms\ngaugor:333|g\nuniques:765|s
```

## How It Works

### Environment Variables

Environment variables are supported through all of Vector's configuration.
Simply add `${MY_ENV_VAR}` in your Vector configuration file and the variable
will be replaced before being evaluated.

You can learn more in the [Environment Variables][docs.configuration#environment-variables]
section.

### Streaming

The `statsd` sink streams data on a real-time
event-by-event basis. It does not batch data.


[docs.configuration#environment-variables]: ../../setup/configuration#environment-variables
[docs.data-model#metric]: ../../about/data-model#metric
[urls.statsd]: https://github.com/statsd/statsd
[urls.statsd_multi]: https://github.com/statsd/statsd/blob/master/docs/metric_types.md#multi-metric-packets
