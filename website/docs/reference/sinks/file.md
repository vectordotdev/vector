---
delivery_guarantee: "best_effort"
event_types: ["log"]
issues_url: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22sink%3A+file%22
sidebar_label: "file|[\"log\"]"
source_url: https://github.com/timberio/vector/blob/master/src/sinks/file/mod.rs
status: "prod-ready"
title: "file sink" 
---

The `file` sink [streams](#streaming) [`log`][docs.data-model#log] events to a file.

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
[sinks.my_sink_id]
  # REQUIRED - General
  type = "file" # example, must be: "file"
  inputs = ["my-source-id"] # example
  path = "vector-%Y-%m-%d.log" # example
  
  # REQUIRED - requests
  encoding = "ndjson" # example, enum
```

</TabItem>
<TabItem value="advanced">

<CodeHeader fileName="vector.toml" learnMoreUrl="/docs/setup/configuration" />

```toml
[sinks.my_sink_id]
  # REQUIRED - General
  type = "file" # example, must be: "file"
  inputs = ["my-source-id"] # example
  path = "vector-%Y-%m-%d.log" # example
  
  # REQUIRED - requests
  encoding = "ndjson" # example, enum
  
  # OPTIONAL - General
  healthcheck = true # default
  idle_timeout_secs = "30" # default
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
  enumValues={{"ndjson":"Each event is encoded into JSON and the payload is new line delimited.","text":"Each event is encoded into text via the `message` key and the payload is new line delimited."}}
  examples={["ndjson","text"]}
  name={"encoding"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={true}
  templateable={false}
  type={"string"}
  unit={null}
  >

### encoding

The encoding format used to serialize the events before outputting.


</Field>


<Field
  common={false}
  defaultValue={true}
  enumValues={null}
  examples={[true,false]}
  name={"healthcheck"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  templateable={false}
  type={"bool"}
  unit={null}
  >

### healthcheck

Enables/disables the sink healthcheck upon start.


</Field>


<Field
  common={false}
  defaultValue={"30"}
  enumValues={null}
  examples={["30"]}
  name={"idle_timeout_secs"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  templateable={false}
  type={"int"}
  unit={null}
  >

### idle_timeout_secs

The amount of time a file can be idle  and stay open. After not receiving any events for this timeout, the file will be flushed and closed.



</Field>


<Field
  common={true}
  defaultValue={null}
  enumValues={null}
  examples={["vector-%Y-%m-%d.log","application-{{ application_id }}-%Y-%m-%d.log"]}
  name={"path"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={true}
  templateable={true}
  type={"string"}
  unit={null}
  >

### path

File name to write events to. See [Template Syntax](#template-syntax) for more info.


</Field>


</Fields>

## Output

The `file` sink [streams](#streaming) [`log`][docs.data-model#log] events to a file.

## How It Works

### Dynamic file and directory creation

Vector will attempt to create the entire directory structure and the file when
emitting events to the file sink. This requires that the Vector agent have
the correct permissions to create and write to files in the specified directories.

### Environment Variables

Environment variables are supported through all of Vector's configuration.
Simply add `${MY_ENV_VAR}` in your Vector configuration file and the variable
will be replaced before being evaluated.

You can learn more in the [Environment Variables][docs.configuration#environment-variables]
section.

### Streaming

The `file` sink streams data on a real-time
event-by-event basis. It does not batch data.

### Template Syntax

The[`path`](#path) options
support [Vector's template syntax][docs.configuration#template-syntax],
enabling dynamic values derived from the event's data. This syntax accepts
[strptime specifiers][urls.strptime_specifiers] as well as the
`{{ field_name }}` syntax for accessing event fields. For example:

<CodeHeader fileName="vector.toml" />

```toml
[sinks.my_file_sink_id]
  # ...
  path = "vector-%Y-%m-%d.log"
  path = "application-{{ application_id }}-%Y-%m-%d.log"
  # ...
```

You can read more about the complete syntax in the
[template syntax section][docs.configuration#template-syntax].


[docs.configuration#environment-variables]: /docs/setup/configuration#environment-variables
[docs.configuration#template-syntax]: /docs/setup/configuration#template-syntax
[docs.data-model#log]: /docs/about/data-model#log
[urls.strptime_specifiers]: https://docs.rs/chrono/0.3.1/chrono/format/strftime/index.html
