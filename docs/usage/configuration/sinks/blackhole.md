---
event_types: ["log","metric"]
issues_url: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22sink%3A+blackhole%22

sidebar_label: "blackhole|[\"log\",\"metric\"]"
source_url: https://github.com/timberio/vector/tree/master/src/sinks/blackhole.rs
status: "prod-ready"
title: "blackhole sink" 
---

The `blackhole` sink [streams](#streaming) [`log`][docs.data-model.log] and [`metric`][docs.data-model.metric] events to a blackhole that simply discards data, designed for testing and benchmarking purposes.

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

<CodeHeader fileName="vector.toml" learnMoreUrl="/usage/configuration"/ >

```toml
[sinks.my_sink_id]
  type = "blackhole" # example, must be: "blackhole"
  inputs = ["my-source-id"] # example
  print_amount = 1000 # example
```

</TabItem>
<TabItem value="advanced">

<CodeHeader fileName="vector.toml" learnMoreUrl="/usage/configuration" />

```toml
[sinks.my_sink_id]
  # REQUIRED
  type = "blackhole" # example, must be: "blackhole"
  inputs = ["my-source-id"] # example
  print_amount = 1000 # example
  
  # OPTIONAL
  healthcheck = true # default
```

</TabItem>

</Tabs>

## Options

import Option from '@site/src/components/Option';
import Options from '@site/src/components/Options';

<Options filters={true}>


<Option
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

Enables/disables the sink healthcheck upon start. See [Health Checks](#health-checks) for more info.


</Option>


<Option
  common={true}
  defaultValue={null}
  enumValues={null}
  examples={[1000]}
  name={"print_amount"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={true}
  type={"int"}
  unit={null}>

### print_amount

The number of events that must be received in order to print a summary of activity.


</Option>


</Options>

## How It Works

### Environment Variables

Environment variables are supported through all of Vector's configuration.
Simply add `${MY_ENV_VAR}` in your Vector configuration file and the variable
will be replaced before being evaluated.

You can learn more in the [Environment Variables][docs.configuration#environment-variables]
section.

### Health Checks

Health checks ensure that the downstream service is accessible and ready to
accept data. This check is performed upon sink initialization.

If the health check fails an error will be logged and Vector will proceed to
start. If you'd like to exit immediately upon health check failure, you can
pass the `--require-healthy` flag:

```bash
vector --config /etc/vector/vector.toml --require-healthy
```

And finally, if you'd like to disable health checks entirely for this sink
you can set the `healthcheck` option to `false`.

### Streaming

The `blackhole` sink streams data on a real-time
event-by-event basis. It does not batch data.


[docs.configuration#environment-variables]: ../../../usage/configuration#environment-variables
[docs.data-model.log]: ../../../about/data-model/log.md
[docs.data-model.metric]: ../../../about/data-model/metric.md
