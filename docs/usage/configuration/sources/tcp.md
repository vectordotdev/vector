---
title: "tcp source" 
sidebar_label: "tcp"
---

The `tcp` source ingests data through the TCP protocol and outputs [`log`][docs.data-model.log] events.

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
  type = "tcp" # enum
  address = "0.0.0.0:9000"
```

</TabItem>
<TabItem value="advanced">

```coffeescript
[sources.my_source_id]
  # REQUIRED - General
  type = "tcp" # enum
  address = "0.0.0.0:9000"
  
  # OPTIONAL - General
  max_length = 102400 # default, bytes
  shutdown_timeout_secs = 30 # default, seconds
  
  # OPTIONAL - Context
  host_key = "host" # default
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
  examples={["0.0.0.0:9000","systemd","systemd#3"]}
  name={"address"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={true}
  simple={true}
  type={"string"}
  unit={null}>

### address

The address to listen for connections on, or "systemd#N" to use the Nth socket passed by systemd socket activation. 


</Option>


<Option
  defaultValue={"host"}
  enumValues={null}
  examples={["host"]}
  name={"host_key"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"string"}
  unit={null}>

### host_key

The key name added to each event representing the current host. See [Context](#context) for more info.


</Option>


<Option
  defaultValue={102400}
  enumValues={null}
  examples={[102400]}
  name={"max_length"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"int"}
  unit={"bytes"}>

### max_length

The maximum bytes size of incoming messages before they are discarded.


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

### Delivery Guarantee

Due to the nature of this component, it offers a
[**best effort** delivery guarantee][docs.guarantees#best-effort-delivery].

### Environment Variables

Environment variables are supported through all of Vector's configuration.
Simply add `${MY_ENV_VAR}` in your Vector configuration file and the variable
will be replaced before being evaluated.

You can learn more in the [Environment Variables][docs.configuration#environment-variables]
section.

### Line Delimiters

Each line is read until a new line delimiter (the `0xA` byte) is found.

## Troubleshooting

The best place to start with troubleshooting is to check the
[Vector logs][docs.monitoring#logs]. This is typically located at
`/var/log/vector.log`, then proceed to follow the
[Troubleshooting Guide][docs.troubleshooting].

If the [Troubleshooting Guide][docs.troubleshooting] does not resolve your
issue, please:

1. Check for any [open `tcp_source` issues][urls.tcp_source_issues].
2. If encountered a bug, please [file a bug report][urls.new_tcp_source_bug].
3. If encountered a missing feature, please [file a feature request][urls.new_tcp_source_enhancement].
4. If you need help, [join our chat/forum community][urls.vector_chat]. You can post a question and search previous questions.

## Resources

* [**Issues**][urls.tcp_source_issues] - [enhancements][urls.tcp_source_enhancements] - [bugs][urls.tcp_source_bugs]
* [**Source code**][urls.tcp_source_source]


[docs.configuration#environment-variables]: ../../../usage/configuration#environment-variables
[docs.data-model.log]: ../../../about/data-model/log.md
[docs.guarantees#best-effort-delivery]: ../../../about/guarantees.md#best-effort-delivery
[docs.monitoring#logs]: ../../../usage/administration/monitoring.md#logs
[docs.transforms.regex_parser]: ../../../usage/configuration/transforms/regex_parser.md
[docs.transforms]: ../../../usage/configuration/transforms
[docs.troubleshooting]: ../../../usage/guides/troubleshooting.md
[urls.new_tcp_source_bug]: https://github.com/timberio/vector/issues/new?labels=source%3A+tcp&labels=Type%3A+bug
[urls.new_tcp_source_enhancement]: https://github.com/timberio/vector/issues/new?labels=source%3A+tcp&labels=Type%3A+enhancement
[urls.tcp_source_bugs]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22source%3A+tcp%22+label%3A%22Type%3A+bug%22
[urls.tcp_source_enhancements]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22source%3A+tcp%22+label%3A%22Type%3A+enhancement%22
[urls.tcp_source_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22source%3A+tcp%22
[urls.tcp_source_source]: https://github.com/timberio/vector/tree/master/src/sources/tcp.rs
[urls.vector_chat]: https://chat.vector.dev
