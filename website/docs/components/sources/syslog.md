---
delivery_guarantee: "best_effort"
event_types: ["log"]
issues_url: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22source%3A+syslog%22
sidebar_label: "syslog|[\"log\"]"
source_url: https://github.com/timberio/vector/tree/master/src/sources/syslog.rs
status: "prod-ready"
title: "syslog source" 
---

The `syslog` source ingests data through the Syslog 5424 protocol and outputs [`log`][docs.data-model#log] events.

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
[sources.my_source_id]
  # REQUIRED
  type = "syslog" # example, must be: "syslog"
  mode = "tcp" # example, enum
  
  # OPTIONAL
  address = "0.0.0.0:9000" # example, no default, relevant when mode = "tcp" or mode = "udp"
  path = "/path/to/socket" # example, no default, relevant when mode = "unix"
```

</TabItem>
<TabItem value="advanced">

<CodeHeader fileName="vector.toml" learnMoreUrl="/setup/configuration" />

```toml
[sources.my_source_id]
  # REQUIRED - General
  type = "syslog" # example, must be: "syslog"
  mode = "tcp" # example, enum
  
  # OPTIONAL - General
  address = "0.0.0.0:9000" # example, no default, relevant when mode = "tcp" or mode = "udp"
  max_length = 102400 # default, bytes
  path = "/path/to/socket" # example, no default, relevant when mode = "unix"
  
  # OPTIONAL - Context
  host_key = "host" # default
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
  examples={["0.0.0.0:9000","systemd","systemd#2"]}
  name={"address"}
  nullable={true}
  path={null}
  relevantWhen={{"mode":["tcp","udp"]}}
  required={false}
  type={"string"}
  unit={null}>

### address

The TCP or UDP address to listen for connections on, or "systemd#N" to use the Nth socket passed by systemd socket activation. 


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
  type={"string"}
  unit={null}>

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
  type={"int"}
  unit={"bytes"}>

### max_length

The maximum bytes size of incoming messages before they are discarded.


</Field>


<Field
  common={true}
  defaultValue={null}
  enumValues={{"tcp":"Read incoming Syslog data over the TCP protocol.","udp":"Read incoming Syslog data over the UDP protocol.","unix":"Read uncoming Syslog data through a Unix socker."}}
  examples={["tcp","udp","unix"]}
  name={"mode"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={true}
  type={"string"}
  unit={null}>

### mode

The input mode.


</Field>


<Field
  common={true}
  defaultValue={null}
  enumValues={null}
  examples={["/path/to/socket"]}
  name={"path"}
  nullable={true}
  path={null}
  relevantWhen={{"mode":"unix"}}
  required={false}
  type={"string"}
  unit={null}>

### path

The unix socket path. *This should be absolute path.*



</Field>


</Fields>

## Input/Output

Given the following input line:

{% code-tabs %}
{% code-tabs-item title="stdin" %}
Given the following input

```
<34>1 2018-10-11T22:14:15.003Z mymachine.example.com su - ID47 - 'su root' failed for lonvick on /dev/pts/8
```
{% endcode-tabs-item %}
{% endcode-tabs %}

A [`log` event][docs.data-model#log] will be output with the following structure:

{% code-tabs %}
{% code-tabs-item title="log" %}
```javascript
{
  "timestamp": <2018-10-11T22:14:15.003Z> # current time,
  "message": "<34>1 2018-10-11T22:14:15.003Z mymachine.example.com su - ID47 - 'su root' failed for lonvick on /dev/pts/8",
  "host": "mymachine.example.com",
  "peer_path": "/path/to/unix/socket" # only relevant if `mode` is `unix`
}
```

Vector only extracts the `"timestamp"` and `"host"` fields and leaves the
`"message"` in-tact. You can further parse the `"message"` key with a
[transform][docs.transforms], such as the
[`regex_parser` transform][docs.transforms.regex_parser].
{% endcode-tabs-item %}
{% endcode-tabs %}

## How It Works

### Context

By default, the `syslog` source will add context
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

### Parsing

Vector will parse messages in the [Syslog 5424][urls.syslog_5424] format.

#### Successful parsing

Upon successful parsing, Vector will create a structured event. For example, given this Syslog message:

```
<13>1 2019-02-13T19:48:34+00:00 74794bfb6795 root 8449 - [meta sequenceId="1"] i am foobar
```

Vector will produce an event with this structure.

```javascript
{
  "message": "<13>1 2019-02-13T19:48:34+00:00 74794bfb6795 root 8449 - [meta sequenceId="1"] i am foobar",
  "timestamp": "2019-02-13T19:48:34+00:00",
  "host": "74794bfb6795"
}
```

#### Unsuccessful parsing

Anyone with Syslog experience knows there are often deviations from the Syslog specifications. Vector tries its best to account for these (note the tests here). In the event Vector fails to parse your format, we recommend that you open an issue informing us of this, and then proceed to use the `tcp`, `udp`, or `unix` source coupled with a parser [transform][docs.transforms] transform of your choice.


[docs.configuration#environment-variables]: ../../setup/configuration#environment-variables
[docs.data-model#log]: ../../about/data-model#log
[docs.transforms.regex_parser]: ../../components/transforms/regex_parser
[docs.transforms]: ../../components/transforms
[urls.syslog_5424]: https://tools.ietf.org/html/rfc5424
