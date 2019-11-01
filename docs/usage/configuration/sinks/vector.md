---
title: "vector sink" 
sidebar_label: "vector"
---

The `vector` sink [streams](#streaming) [`log`][docs.data-model.log] events to another downstream Vector instance.

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
  type = "vector" # enum
  inputs = ["my-source-id"]
  address = "92.12.333.224:5000"
```

</TabItem>
<TabItem value="advanced">

```coffeescript
[sinks.my_sink_id]
  # REQUIRED - General
  type = "vector" # enum
  inputs = ["my-source-id"]
  address = "92.12.333.224:5000"
  
  # OPTIONAL - General
  healthcheck = true # default
  
  # OPTIONAL - Buffer
  [sinks.my_sink_id.buffer]
    type = "memory" # default, enum
    max_size = 104900000 # no default, bytes, relevant when type = "disk"
    num_items = 500 # default, events, relevant when type = "memory"
    when_full = "block" # default, enum
```

</TabItem>

</Tabs>

You can learn more

## Options

import Option from '@site/src/components/Option';
import Options from '@site/src/components/Options';

<Options filters={true}>


<Option
  defaultValue={null}
  enumValues={null}
  examples={["92.12.333.224:5000"]}
  name={"address"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={true}
  simple={true}
  type={"string"}
  unit={null}>

### address

The downstream Vector address.


</Option>


<Option
  defaultValue={null}
  enumValues={null}
  examples={[]}
  name={"buffer"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"table"}
  unit={null}>

### buffer

Configures the sink specific buffer.

<Options filters={false}>


<Option
  defaultValue={"memory"}
  enumValues={{"memory":"Stores the sink's buffer in memory. This is more performant (~3x), but less durable. Data will be lost if Vector is restarted abruptly.","disk":"Stores the sink's buffer on disk. This is less performance (~3x),  but durable. Data will not be lost between restarts."}}
  examples={["memory","disk"]}
  name={"type"}
  nullable={false}
  path={"buffer"}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"string"}
  unit={null}>

#### type

The buffer's type / location. `disk` buffers are persistent and will be retained between restarts.


</Option>


<Option
  defaultValue={"block"}
  enumValues={{"block":"Applies back pressure when the buffer is full. This prevents data loss, but will cause data to pile up on the edge.","drop_newest":"Drops new data as it's received. This data is lost. This should be used when performance is the highest priority."}}
  examples={["block","drop_newest"]}
  name={"when_full"}
  nullable={false}
  path={"buffer"}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"string"}
  unit={null}>

#### when_full

The behavior when the buffer becomes full.


</Option>


<Option
  defaultValue={null}
  enumValues={null}
  examples={[104900000]}
  name={"max_size"}
  nullable={true}
  path={"buffer"}
  relevantWhen={{"type":"disk"}}
  required={false}
  simple={false}
  type={"int"}
  unit={"bytes"}>

#### max_size

The maximum size of the buffer on the disk.


</Option>


<Option
  defaultValue={500}
  enumValues={null}
  examples={[500]}
  name={"num_items"}
  nullable={true}
  path={"buffer"}
  relevantWhen={{"type":"memory"}}
  required={false}
  simple={false}
  type={"int"}
  unit={"events"}>

#### num_items

The maximum number of [events][docs.event] allowed in the buffer.


</Option>


</Options>

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

Enables/disables the sink healthcheck upon start. See [Health Checks](#health-checks) for more info.


</Option>


</Options>

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

The `vector` sink streams data on a real-time
event-by-event basis. It does not batch data.

## Troubleshooting

The best place to start with troubleshooting is to check the
[Vector logs][docs.monitoring#logs]. This is typically located at
`/var/log/vector.log`, then proceed to follow the
[Troubleshooting Guide][docs.troubleshooting].

If the [Troubleshooting Guide][docs.troubleshooting] does not resolve your
issue, please:

1. Check for any [open `vector_sink` issues][urls.vector_sink_issues].
2. If encountered a bug, please [file a bug report][urls.new_vector_sink_bug].
3. If encountered a missing feature, please [file a feature request][urls.new_vector_sink_enhancement].
4. If you need help, [join our chat/forum community][urls.vector_chat]. You can post a question and search previous questions.

## Resources

* [**Issues**][urls.vector_sink_issues] - [enhancements][urls.vector_sink_enhancements] - [bugs][urls.vector_sink_bugs]
* [**Source code**][urls.vector_sink_source]


[docs.configuration#environment-variables]: ../../../usage/configuration#environment-variables
[docs.data-model.log]: ../../../about/data-model/log.md
[docs.event]: ../../../setup/getting-started/sending-your-first-event.md
[docs.guarantees#best-effort-delivery]: ../../../about/guarantees.md#best-effort-delivery
[docs.monitoring#logs]: ../../../usage/administration/monitoring.md#logs
[docs.troubleshooting]: ../../../usage/guides/troubleshooting.md
[urls.new_vector_sink_bug]: https://github.com/timberio/vector/issues/new?labels=sink%3A+vector&labels=Type%3A+bug
[urls.new_vector_sink_enhancement]: https://github.com/timberio/vector/issues/new?labels=sink%3A+vector&labels=Type%3A+enhancement
[urls.vector_chat]: https://chat.vector.dev
[urls.vector_sink_bugs]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22sink%3A+vector%22+label%3A%22Type%3A+bug%22
[urls.vector_sink_enhancements]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22sink%3A+vector%22+label%3A%22Type%3A+enhancement%22
[urls.vector_sink_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22sink%3A+vector%22
[urls.vector_sink_source]: https://github.com/timberio/vector/tree/master/src/sinks/vector.rs
