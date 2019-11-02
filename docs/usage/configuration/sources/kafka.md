---
event_types: ["log"]
issues_url: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22source%3A+kafka%22
output_types: ["log"]
sidebar_label: "kafka|[\"log\"]"
source_url: https://github.com/timberio/vector/tree/master/src/sources/kafka.rs
status: "beta"
title: "kafka source" 
---

The `kafka` source ingests data through Kafka 0.9 or later and outputs [`log`][docs.data-model.log] events.

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
  type = "kafka" # example, must be: "kafka"
  bootstrap_servers = "10.14.22.123:9092,10.14.23.332:9092" # example
  group_id = "consumer-group-name" # example
  topics = ["topic-1", "topic-2", "^(prefix1|prefix2)-.+"] # example
```

</TabItem>
<TabItem value="advanced">

<CodeHeader fileName="vector.toml" learnMoreUrl="/usage/configuration" />

```toml
[sources.my_source_id]
  # REQUIRED
  type = "kafka" # example, must be: "kafka"
  bootstrap_servers = "10.14.22.123:9092,10.14.23.332:9092" # example
  group_id = "consumer-group-name" # example
  topics = ["topic-1", "topic-2", "^(prefix1|prefix2)-.+"] # example
  
  # OPTIONAL
  auto_offset_reset = "smallest" # default
  key_field = "user_id" # example, no default
  session_timeout_ms = 5000 # default, milliseconds
```

</TabItem>

</Tabs>

## Options

import Option from '@site/src/components/Option';
import Options from '@site/src/components/Options';

<Options filters={true}>


<Option
  common={false}
  defaultValue={"largest"}
  enumValues={null}
  examples={["smallest","earliest","beginning","largest","latest","end","error"]}
  name={"auto_offset_reset"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  type={"string"}
  unit={null}>

### auto_offset_reset

If offsets for consumer group do not exist, set them using this strategy. [librdkafka documentation][urls.lib_rdkafka_config] for `auto.offset.reset` option for explanation.


</Option>


<Option
  common={true}
  defaultValue={null}
  enumValues={null}
  examples={["10.14.22.123:9092,10.14.23.332:9092"]}
  name={"bootstrap_servers"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={true}
  type={"string"}
  unit={null}>

### bootstrap_servers

A comma-separated list of host and port pairs that are the addresses of the Kafka brokers in a "bootstrap" Kafka cluster that a Kafka client connects to initially to bootstrap itself.


</Option>


<Option
  common={true}
  defaultValue={null}
  enumValues={null}
  examples={["consumer-group-name"]}
  name={"group_id"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={true}
  type={"string"}
  unit={null}>

### group_id

The consumer group name to be used to consume events from Kafka.



</Option>


<Option
  common={false}
  defaultValue={null}
  enumValues={null}
  examples={["user_id"]}
  name={"key_field"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  type={"string"}
  unit={null}>

### key_field

The log field name to use for the topic key. If unspecified, the key would not be added to the log event. If the message has null key, then this field would not be added to the log event.


</Option>


<Option
  common={false}
  defaultValue={10000}
  enumValues={null}
  examples={[5000,10000]}
  name={"session_timeout_ms"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  type={"int"}
  unit={"milliseconds"}>

### session_timeout_ms

The Kafka session timeout in milliseconds.



</Option>


<Option
  common={true}
  defaultValue={null}
  enumValues={null}
  examples={[["topic-1","topic-2","^(prefix1|prefix2)-.+"]]}
  name={"topics"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={true}
  type={"[string]"}
  unit={null}>

### topics

The Kafka topics names to read events from. Regex is supported if the topic begins with `^`.



</Option>


</Options>

## Input/Output

Given the following message in a Kafka topic:

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

### Environment Variables

Environment variables are supported through all of Vector's configuration.
Simply add `${MY_ENV_VAR}` in your Vector configuration file and the variable
will be replaced before being evaluated.

You can learn more in the [Environment Variables][docs.configuration#environment-variables]
section.


[docs.configuration#environment-variables]: ../../../usage/configuration#environment-variables
[docs.data-model.log]: ../../../about/data-model/log.md
[docs.transforms.regex_parser]: ../../../usage/configuration/transforms/regex_parser.md
[docs.transforms]: ../../../usage/configuration/transforms
[urls.lib_rdkafka_config]: https://github.com/edenhill/librdkafka/blob/master/CONFIGURATION.md
