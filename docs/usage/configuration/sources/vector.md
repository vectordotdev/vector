---
title: "vector source" 
sidebar_label: "vector"
---

The `vector` source ingests data through another upstream Vector instance and outputs [`log`][docs.data-model.log] and [`metric`][docs.data-model.metric] events.

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
[sources.my_source_id]
  type = "vector" # enum
  address = "0.0.0.0:9000"
```

</TabItem>
<TabItem value="advanced">

```coffeescript
[sources.my_source_id]
  # REQUIRED
  type = "vector" # enum
  address = "0.0.0.0:9000"
  
  # OPTIONAL
  shutdown_timeout_secs = 30 # default, seconds
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
  examples={["0.0.0.0:9000","systemd","systemd#1"]}
  name={"address"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={true}
  simple={true}
  type={"string"}
  unit={null}>

### address

The TCP address to listen for connections on, or "systemd#N" to use the Nth socket passed by systemd socket activation. 


</Option>


<Option
  defaultValue={30}
  enumValues={null}
  examples={[30]}
  name={"shutdown_timeout_secs"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"int"}
  unit={"seconds"}>

### shutdown_timeout_secs

The timeout before a connection is forcefully closed during shutdown.


</Option>


</Options>

## How It Works

### Delivery Guarantee

Due to the nature of this component, it offers a
[**best effort** delivery guarantee][docs.guarantees#best-effort-delivery].

### Encoding

Data is encoded via Vector's [event protobuf][urls.event_proto] before it is sent over the wire.

### Environment Variables

Environment variables are supported through all of Vector's configuration.
Simply add `${MY_ENV_VAR}` in your Vector configuration file and the variable
will be replaced before being evaluated.

You can learn more in the [Environment Variables][docs.configuration#environment-variables]
section.

### Message Acking

Currently, Vector does not perform any application level message acknowledgement. While rare, this means the individual message could be lost.

### TCP Protocol

Upstream Vector instances forward data to downstream Vector instances via the TCP protocol.

## Troubleshooting

The best place to start with troubleshooting is to check the
[Vector logs][docs.monitoring#logs]. This is typically located at
`/var/log/vector.log`, then proceed to follow the
[Troubleshooting Guide][docs.troubleshooting].

If the [Troubleshooting Guide][docs.troubleshooting] does not resolve your
issue, please:

1. Check for any [open `vector_source` issues][urls.vector_source_issues].
2. If encountered a bug, please [file a bug report][urls.new_vector_source_bug].
3. If encountered a missing feature, please [file a feature request][urls.new_vector_source_enhancement].
4. If you need help, [join our chat/forum community][urls.vector_chat]. You can post a question and search previous questions.

## Resources

* [**Issues**][urls.vector_source_issues] - [enhancements][urls.vector_source_enhancements] - [bugs][urls.vector_source_bugs]
* [**Source code**][urls.vector_source_source]


[docs.configuration#environment-variables]: ../../../usage/configuration#environment-variables
[docs.data-model.log]: ../../../about/data-model/log.md
[docs.data-model.metric]: ../../../about/data-model/metric.md
[docs.guarantees#best-effort-delivery]: ../../../about/guarantees.md#best-effort-delivery
[docs.monitoring#logs]: ../../../usage/administration/monitoring.md#logs
[docs.troubleshooting]: ../../../usage/guides/troubleshooting.md
[urls.event_proto]: https://github.com/timberio/vector/blob/master/proto/event.proto
[urls.new_vector_source_bug]: https://github.com/timberio/vector/issues/new?labels=source%3A+vector&labels=Type%3A+bug
[urls.new_vector_source_enhancement]: https://github.com/timberio/vector/issues/new?labels=source%3A+vector&labels=Type%3A+enhancement
[urls.vector_chat]: https://chat.vector.dev
[urls.vector_source_bugs]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22source%3A+vector%22+label%3A%22Type%3A+bug%22
[urls.vector_source_enhancements]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22source%3A+vector%22+label%3A%22Type%3A+enhancement%22
[urls.vector_source_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22source%3A+vector%22
[urls.vector_source_source]: https://github.com/timberio/vector/tree/master/src/sources/vector.rs
