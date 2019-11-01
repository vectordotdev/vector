---
title: "statsd sink" 
sidebar_label: "statsd"
---

The `statsd` sink [streams](#streaming) [`metric`][docs.data-model.metric] events to [StatsD][urls.statsd] metrics service.

## Example

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

<Tabs
  defaultValue="simple"
  values={[
    { label: 'Simple', value: 'simple', },
    { label: 'Advanced', value: 'advanced', },
  ]
}>
<TabItem value="simple">

```coffeescript
[sinks.my_sink_id]
  type = "statsd" # enum
  inputs = ["my-source-id"]
  namespace = "service"
```

</TabItem>
<TabItem value="advanced">

```coffeescript
[sinks.my_sink_id]
  # REQUIRED
  type = "statsd" # enum
  inputs = ["my-source-id"]
  namespace = "service"
  
  # OPTIONAL
  address = "127.0.0.1:8125" # default
  healthcheck = true # default
```

</TabItem>

</Tabs>

You can learn more

## Options

import Option from '@site/src/components/Option';
import Options from '@site/src/components/Options';

<Options filters={true}>


<Option
  defaultValue={"127.0.0.1:8125"}
  enumValues={null}
  examples={["127.0.0.1:8125"]}
  name={"address"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"string"}
  unit={null}>

### address

The UDP socket address to send stats to.


</Option>


<Option
  defaultValue={true}
  enumValues={null}
  examples={[true,false]}
  name={"healthcheck"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"bool"}
  unit={null}>

### healthcheck

Enables/disables the sink healthcheck upon start.


</Option>


<Option
  defaultValue={null}
  enumValues={null}
  examples={["service"]}
  name={"namespace"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={true}
  simple={true}
  type={"string"}
  unit={null}>

### namespace

A prefix that will be added to all metric names.


</Option>


</Options>

## Input/Output

The `statsd` sink batches [`metric`][docs.data-model.metric] up to the `batch_size` or `batch_timeout` options. When flushed, metrics will be written in [Multi-metric format][urls.statsd_multi]. For example:

```
gorets:1|c\nglork:320|ms\ngaugor:333|g\nuniques:765|s
```

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

### Streaming

The `statsd` sink streams data on a real-time
event-by-event basis. It does not batch data.

## Troubleshooting

The best place to start with troubleshooting is to check the
[Vector logs][docs.monitoring#logs]. This is typically located at
`/var/log/vector.log`, then proceed to follow the
[Troubleshooting Guide][docs.troubleshooting].

If the [Troubleshooting Guide][docs.troubleshooting] does not resolve your
issue, please:

1. Check for any [open `statsd_sink` issues][urls.statsd_sink_issues].
2. If encountered a bug, please [file a bug report][urls.new_statsd_sink_bug].
3. If encountered a missing feature, please [file a feature request][urls.new_statsd_sink_enhancement].
4. If you need help, [join our chat/forum community][urls.vector_chat]. You can post a question and search previous questions.

## Resources

* [**Issues**][urls.statsd_sink_issues] - [enhancements][urls.statsd_sink_enhancements] - [bugs][urls.statsd_sink_bugs]
* [**Source code**][urls.statsd_sink_source]


[docs.configuration#environment-variables]: ../../../usage/configuration#environment-variables
[docs.data-model.metric]: ../../../about/data-model/metric.md
[docs.guarantees#best-effort-delivery]: ../../../about/guarantees.md#best-effort-delivery
[docs.monitoring#logs]: ../../../usage/administration/monitoring.md#logs
[docs.troubleshooting]: ../../../usage/guides/troubleshooting.md
[urls.new_statsd_sink_bug]: https://github.com/timberio/vector/issues/new?labels=sink%3A+statsd&labels=Type%3A+bug
[urls.new_statsd_sink_enhancement]: https://github.com/timberio/vector/issues/new?labels=sink%3A+statsd&labels=Type%3A+enhancement
[urls.statsd]: https://github.com/statsd/statsd
[urls.statsd_multi]: https://github.com/statsd/statsd/blob/master/docs/metric_types.md#multi-metric-packets
[urls.statsd_sink_bugs]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22sink%3A+statsd%22+label%3A%22Type%3A+bug%22
[urls.statsd_sink_enhancements]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22sink%3A+statsd%22+label%3A%22Type%3A+enhancement%22
[urls.statsd_sink_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22sink%3A+statsd%22
[urls.statsd_sink_source]: https://github.com/timberio/vector/tree/master/src/sinks/statsd.rs
[urls.vector_chat]: https://chat.vector.dev
