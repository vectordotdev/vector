---

event_types: ["log"]
issues_url: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22transform%3A+json_parser%22
operating_systems: ["linux","macos","windows"]
sidebar_label: "json_parser|[\"log\"]"
source_url: https://github.com/timberio/vector/tree/master/src/transforms/json_parser.rs
status: "prod-ready"
title: "json_parser transform"
unsupported_operating_systems: []
---

The `json_parser` transform accepts [`log`][docs.data-model#log] events and allows you to parse a log field value as JSON.

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
[transforms.my_transform_id]
  # REQUIRED
  type = "json_parser" # example, must be: "json_parser"
  inputs = ["my-source-id"] # example
  drop_invalid = true # example
  
  # OPTIONAL
  field = "message" # default
```

</TabItem>
<TabItem value="advanced">

<CodeHeader fileName="vector.toml" learnMoreUrl="/docs/setup/configuration" />

```toml
[transforms.my_transform_id]
  # REQUIRED
  type = "json_parser" # example, must be: "json_parser"
  inputs = ["my-source-id"] # example
  drop_invalid = true # example
  
  # OPTIONAL
  field = "message" # default
  overwrite_target = true # default
  target_field = "target" # example, no default
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
  examples={[true]}
  name={"drop_invalid"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={true}
  templateable={false}
  type={"bool"}
  unit={null}
  >

### drop_invalid

If `true` events with invalid JSON will be dropped, otherwise the event will be kept and passed through. See [Invalid JSON](#invalid-json) for more info.


</Field>


<Field
  common={true}
  defaultValue={"message"}
  enumValues={null}
  examples={["message"]}
  name={"field"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  templateable={false}
  type={"string"}
  unit={null}
  >

### field

The log field to decode as JSON. Must be a `string` value type. See [Invalid JSON](#invalid-json) for more info.


</Field>


<Field
  common={false}
  defaultValue={"false"}
  enumValues={null}
  examples={[true,false]}
  name={"overwrite_target"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  templateable={false}
  type={"bool"}
  unit={null}
  >

### overwrite_target

If[`target_field`](#target_field) is set and the log contains a field of the same name as the target, it will only be overwritten if this is set to `true`.


</Field>


<Field
  common={false}
  defaultValue={null}
  enumValues={null}
  examples={["target"]}
  name={"target_field"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  templateable={false}
  type={"string"}
  unit={null}
  >

### target_field

If this setting is present, the parsed JSON will be inserted into the log as a sub-object with this name. If a field with the same name already exists, the parser will fail and produce an error.


</Field>


</Fields>

## Output

<Tabs
  block={true}
  defaultValue="simple"
  values={[
    { label: 'Simple', value: 'simple', },
    { label: 'Wrapped', value: 'wrapped', },
  ]
}>

<TabItem value="simple">

Given the following log event:

```
{
  "message": "{"key": "value"}"
}
```

You can parse the JSON with:

```toml
[transforms.json]
  inputs = ["<source_id>"]
  type   = "json_parser"
  field  = "message"
```

This would produce the following event as output:

```javascript
{
  "key": "value"
}
```

By default, Vector drops fields after parsing them via the `drop_field`
option.

</TabItem>
<TabItem value="wrapped">

It is possible to chain `json_parser` transforms to effectively "unwrap"
nested JSON documents. For example, give this log event:

```
{
  "message": "{"parent": "{\"child\": \"value2\"}"}"
}
```

You could unwrap the JSON with the following transforms:

```toml
[transforms.root_json]
  inputs = ["<source_id>"]
  type   = "json_parser"
  field  = "message"

[transforms.parent_json]
  inputs = ["root_json"]
  type   = "json_parser"
  field  = "parent"

[transforms.child_json]
  inputs = ["parent_json"]
  type   = "json_parser"
  field  = "child"
```

This would produce the following event as output:

```javascript
{
  "child": "value2"
}
```

By default, Vector drops fields after parsing them via the `drop_field`
option.

</TabItem>
</Tabs>

## How It Works

### Chaining / Unwrapping

Please see the [I/O section](#i-o) for an example of chaining and unwrapping JSON.

### Correctness

The `json_parser` source has been involved in the following correctness tests:

* [`wrapped_json_correctness`][urls.wrapped_json_correctness_test]

Learn more in the [Correctness][docs.correctness] sections.

### Environment Variables

Environment variables are supported through all of Vector's configuration.
Simply add `${MY_ENV_VAR}` in your Vector configuration file and the variable
will be replaced before being evaluated.

You can learn more in the [Environment Variables][docs.configuration#environment-variables]
section.

### Invalid JSON

If the value for the specified[`field`](#field) is not valid JSON you can control keep or discard the event with the[`drop_invalid`](#drop_invalid) option. Setting it to `true` will discard the event and drop it entirely. Setting it to `false` will keep the event and pass it through. Note that passing through the event could cause problems and violate assumptions about the structure of your event.

### Key Conflicts

Any key present in the decoded JSON will override existin keys in the event.

### Nested Fields

If the decoded JSON includes nested fields it will be _deep_ merged into the event. For example, given the following event:

```javascript
{
  "message": "{"parent": {"child2": "value2"}}",
  "parent": {
    "child1": "value1"
  }
}
```

Parsing the `"message"` field would result the following structure:

```javascript
{
  "parent": {
    "child1": "value1",
    "child2": "value2"
  }
}
```


[docs.configuration#environment-variables]: /docs/setup/configuration#environment-variables
[docs.correctness]: /docs/about/correctness
[docs.data-model#log]: /docs/about/data-model#log
[urls.wrapped_json_correctness_test]: https://github.com/timberio/vector-test-harness/tree/master/cases/wrapped_json_correctness
