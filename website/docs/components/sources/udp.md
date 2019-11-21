---
delivery_guarantee: "best_effort"
event_types: ["log"]
issues_url: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22source%3A+udp%22
sidebar_label: "udp|[\"log\"]"
source_url: https://github.com/timberio/vector/tree/master/src/sources/udp.rs
status: "prod-ready"
title: "udp source" 
---

The `udp` source ingests data through the UDP protocol and outputs [`log`][docs.data-model#log] events.

## Configuration

import Tabs from '@theme/Tabs';

<Tabs
  block={true}
  defaultValue="common"
  values={[
    { label: 'Common', value: 'common', },
    { label: 'Advanced', value: 'advanced', },
  ]
}>

import TabItem from '@theme/TabItem';

<TabItem value="common">

import CodeHeader from '@site/src/components/CodeHeader';

<CodeHeader fileName="vector.toml" learnMoreUrl="/docs/setup/configuration"/ >

```toml
[sources.my_source_id]
  type = "udp" # example, must be: "udp"
  address = "0.0.0.0:9000" # example
```

</TabItem>
<TabItem value="advanced">

<CodeHeader fileName="vector.toml" learnMoreUrl="/docs/setup/configuration" />

```toml
[sources.my_source_id]
  # REQUIRED - General
  type = "udp" # example, must be: "udp"
  address = "0.0.0.0:9000" # example
  
  # OPTIONAL - General
  max_length = 102400 # default, bytes
  
  # OPTIONAL - Context
  host_key = "host" # default
```

</TabItem>

</Tabs>

## Options

import Fields from '@site/src/components/Fields';

import Field from '@site/src/components/Field';

<Fields filters={true}>


<Field
  common={true}
  defaultValue={null}
  enumValues={null}
  examples={["0.0.0.0:9000"]}
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

The address to bind the socket to.


</Field>


<Field
  common={false}
  defaultValue={"host"}
  enumValues={null}
  examples={["host"]}
  name={"host_key"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  templateable={false}
  type={"string"}
  unit={null}
  >

### host_key

The key name added to each event representing the current host. See [Context](#context) for more info.


</Field>


<Field
  common={false}
  defaultValue={102400}
  enumValues={null}
  examples={[102400]}
  name={"max_length"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  templateable={false}
  type={"int"}
  unit={"bytes"}
  >

### max_length

The maximum bytes size of incoming messages before they are discarded.


</Field>


</Fields>

## Output

This component outputs [`log` events][docs.data-model.log].

Given the following input:

```
2019-02-13T19:48:34+00:00 [info] Started GET "/" for 127.0.0.1
```

A [`log` event][docs.data-model.log] will be output with the
following structure:

```json
{
  "timestamp": <current_timestamp>,
  "message": "2019-02-13T19:48:34+00:00 [info] Started GET "/" for 127.0.0.1",
  "host": "<upstream_hostname>"
}

```

More detail on the output schema is below.

<Fields filters={true}>


<Field
  enumValues={null}
  examples={["my.host.com"]}
  name={"host"}
  path={null}
  required={true}
  type={"string"}
  >

### host

The upstream hostname.



</Field>


<Field
  enumValues={null}
  examples={["Started GET / for 127.0.0.1 at 2012-03-10 14:28:14 +0100"]}
  name={"message"}
  path={null}
  required={true}
  type={"string"}
  >

### message

The raw message, unaltered.



</Field>


<Field
  enumValues={null}
  examples={["2019-11-01T21:15:47+00:00"]}
  name={"timestamp"}
  path={null}
  required={true}
  type={"timestamp"}
  >

### timestamp

The exact time the event was ingested.



</Field>


</Fields>

## How It Works

### Context

By default, the `udp` source will add context
keys to your events via the[`host_key`](#host_key)
options.

### Environment Variables

Environment variables are supported through all of Vector's configuration.
Simply add `${MY_ENV_VAR}` in your Vector configuration file and the variable
will be replaced before being evaluated.

You can learn more in the [Environment Variables][docs.configuration#environment-variables]
section.

### Line Delimiters

Each line is read until a new line delimiter (the `0xA` byte) is found.


[docs.configuration#environment-variables]: /docs/setup/configuration#environment-variables
[docs.data-model#log]: /docs/about/data-model#log
[docs.data-model.log]: /docs/about/data-model/log
