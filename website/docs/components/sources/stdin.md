---
delivery_guarantee: "at_least_once"
event_types: ["log"]
issues_url: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22source%3A+stdin%22
sidebar_label: "stdin|[\"log\"]"
source_url: https://github.com/timberio/vector/tree/master/src/sources/stdin.rs
status: "prod-ready"
title: "stdin source" 
---

The `stdin` source ingests data through standard input (STDIN) and outputs [`log`][docs.data-model#log] events.

## Configuration

import Tabs from '@theme/Tabs';

<Tabs
  defaultValue="common"
  values={[
    { label: 'Common', value: 'common', },
    { label: 'Advanced', value: 'advanced', },
  ]
}>

import TabItem from '@theme/TabItem';

<TabItem value="common">

import CodeHeader from '@site/src/components/CodeHeader';

<CodeHeader fileName="vector.toml" learnMoreUrl="/setup/configuration"/ >

```toml
[sources.my_source_id]
  type = "stdin" # example, must be: "stdin"
```

</TabItem>
<TabItem value="advanced">

<CodeHeader fileName="vector.toml" learnMoreUrl="/setup/configuration" />

```toml
[sources.my_source_id]
  # REQUIRED - General
  type = "stdin" # example, must be: "stdin"
  
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

The maxiumum bytes size of a message before it is discarded.


</Field>


</Fields>

## Output (log)

This component outputs [`log` events][docs.data-model.log].
For example:

```javascript
{
  "message": "Started GET / for 127.0.0.1 at 2012-03-10 14:28:14 +0100",
  "timestamp": "2019-11-01T21:15:47+00:00"
}
```
More detail on the output schema is below.

<Fields filters={true}>


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

## Output

Given the following input line:

{% code-tabs %}
{% code-tabs-item title="stdin" %}
```
2019-02-13T19:48:34+00:00 [info] Started GET "/" for 127.0.0.1
```
{% endcode-tabs-item %}
{% endcode-tabs %}

A [`log` event][docs.data-model#log] will be output with the following structure:

{% code-tabs %}
{% code-tabs-item title="log" %}
```javascript
{
  "timestamp": <timestamp> # current time,
  "message": "2019-02-13T19:48:34+00:00 [info] Started GET "/" for 127.0.0.1",
  "host": "10.2.22.122" # current hostname
}
```

The "timestamp" and `"host"` keys were automatically added as context. You can
further parse the `"message"` key with a [transform][docs.transforms], such as
the [`regex_parser` transform][docs.transforms.regex_parser].
{% endcode-tabs-item %}
{% endcode-tabs %}

## How It Works

### Context

By default, the `stdin` source will add context
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
[docs.transforms.regex_parser]: /docs/components/transforms/regex_parser
[docs.transforms]: /docs/components/transforms
