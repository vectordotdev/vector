---
event_types: ["log","metric"]
issues_url: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22source%3A+vector%22
output_types: ["log","metric"]
sidebar_label: "vector|[\"log\",\"metric\"]"
source_url: https://github.com/timberio/vector/tree/master/src/sources/vector.rs
status: "beta"
title: "vector source" 
---

The `vector` source ingests data through another upstream Vector instance and outputs [`log`][docs.data-model.log] and [`metric`][docs.data-model.metric] events.

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
[sources.my_source_id]
  type = "vector" # example, must be: "vector"
  address = "0.0.0.0:9000" # example
```

</TabItem>
<TabItem value="advanced">

<CodeHeader fileName="vector.toml" learnMoreUrl="/usage/configuration" />

```toml
[sources.my_source_id]
  # REQUIRED
  type = "vector" # example, must be: "vector"
  address = "0.0.0.0:9000" # example
  
  # OPTIONAL
  shutdown_timeout_secs = 30 # default, seconds
```

</TabItem>

</Tabs>

## Options

import Option from '@site/src/components/Option';
import Options from '@site/src/components/Options';

<Options filters={true}>


<Option
  common={true}
  defaultValue={null}
  enumValues={null}
  examples={["0.0.0.0:9000","systemd","systemd#1"]}
  name={"address"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={true}
  type={"string"}
  unit={null}>

### address

The TCP address to listen for connections on, or "systemd#N" to use the Nth socket passed by systemd socket activation. 


</Option>


<Option
  common={false}
  defaultValue={30}
  enumValues={null}
  examples={[30]}
  name={"shutdown_timeout_secs"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  type={"int"}
  unit={"seconds"}>

### shutdown_timeout_secs

The timeout before a connection is forcefully closed during shutdown.


</Option>


</Options>

## How It Works

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


[docs.configuration#environment-variables]: ../../../usage/configuration#environment-variables
[docs.data-model.log]: ../../../about/data-model/log.md
[docs.data-model.metric]: ../../../about/data-model/metric.md
[urls.event_proto]: https://github.com/timberio/vector/blob/master/proto/event.proto
