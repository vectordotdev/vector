---
event_types: ["log"]
issues_url: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22source%3A+tcp%22
output_types: ["log"]
sidebar_label: "tcp|[\"log\"]"
source_url: https://github.com/timberio/vector/tree/master/src/sources/tcp.rs
status: "prod-ready"
title: "tcp source" 
---

The `tcp` source ingests data through the TCP protocol and outputs [`log`][docs.data-model.log] events.

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
  type = "tcp" # example, must be: "tcp"
  address = "0.0.0.0:9000" # example
```

</TabItem>
<TabItem value="advanced">

<CodeHeader fileName="vector.toml" learnMoreUrl="/usage/configuration" />

```toml
[sources.my_source_id]
  # REQUIRED - General
  type = "tcp" # example, must be: "tcp"
  address = "0.0.0.0:9000" # example
  
  # OPTIONAL - General
  max_length = 102400 # default, bytes
  shutdown_timeout_secs = 30 # default, seconds
  
  # OPTIONAL - Context
  host_key = "host" # default
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
  examples={["0.0.0.0:9000","systemd","systemd#3"]}
  name={"address"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={true}
  type={"string"}
  unit={null}>

### address

The address to listen for connections on, or "systemd#N" to use the Nth socket passed by systemd socket activation. 


</Option>


<Option
  common={false}
  defaultValue={"host"}
  enumValues={null}
  examples={["host"]}
  name={"host_key"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  type={"string"}
  unit={null}>

### host_key

The key name added to each event representing the current host. See [Context](#context) for more info.


</Option>


<Option
  common={false}
  defaultValue={102400}
  enumValues={null}
  examples={[102400]}
  name={"max_length"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  type={"int"}
  unit={"bytes"}>

### max_length

The maximum bytes size of incoming messages before they are discarded.


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

## Input/Output

Given the following input line:

{% code-tabs %}
{% code-tabs-item title="stdin" %}
```
2019-02-13T19:48:34+00:00 [info] Started GET "/" for 127.0.0.1
```
{% endcode-tabs-item %}
{% endcode-tabs %}

A [`log` event][docs.data-model.log] will be output with the following structure:

{% code-tabs %}
{% code-tabs-item title="log" %}
```javascript
{
  "timestamp": <timestamp> # current time,
  "message": "2019-02-13T19:48:34+00:00 [info] Started GET "/" for 127.0.0.1",
  "host": "10.2.22.122" # current nostname
}
```

The "timestamp" and `"host"` keys were automatically added as context. You can
further parse the `"message"` key with a [transform][docs.transforms], such as
the [`regex_parser` transform][docs.transforms.regex_parser].
{% endcode-tabs-item %}
{% endcode-tabs %}

## How It Works

### Context

By default, the `tcp` source will add context
keys to your events via the `host_key`
options.

### Environment Variables

Environment variables are supported through all of Vector's configuration.
Simply add `${MY_ENV_VAR}` in your Vector configuration file and the variable
will be replaced before being evaluated.

You can learn more in the [Environment Variables][docs.configuration#environment-variables]
section.

### Line Delimiters

Each line is read until a new line delimiter (the `0xA` byte) is found.


[docs.configuration#environment-variables]: ../../../usage/configuration#environment-variables
[docs.data-model.log]: ../../../about/data-model/log.md
[docs.transforms.regex_parser]: ../../../usage/configuration/transforms/regex_parser.md
[docs.transforms]: ../../../usage/configuration/transforms
