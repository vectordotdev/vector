---

event_types: ["log"]
issues_url: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22transform%3A+split%22

sidebar_label: "split|[\"log\"]"
source_url: https://github.com/timberio/vector/tree/master/src/transforms/split.rs
status: "prod-ready"
title: "split transform"

---

The `split` transform accepts [`log`][docs.data-model#log] events and allows you to split a field's value on a given separator and zip the tokens into ordered field names.

## Configuration

import CodeHeader from '@site/src/components/CodeHeader';

<CodeHeader fileName="vector.toml" learnMoreUrl="/docs/setup/configuration"/ >

```toml
[transforms.my_transform_id]
  # REQUIRED - General
  type = "split" # example, must be: "split"
  inputs = ["my-source-id"] # example
  field_names = ["timestamp", "level", "message"] # example
  
  # OPTIONAL - General
  drop_field = true # default
  field = "message" # default
  separator = "," # default
  
  # OPTIONAL - Types
  [transforms.my_transform_id.types]
    status = "int" # example
    duration = "float" # example
    success = "bool" # example
    timestamp = "timestamp|%s" # example
    timestamp = "timestamp|%+" # example
    timestamp = "timestamp|%F" # example
    timestamp = "timestamp|%a %b %e %T %Y" # example
```

## Options

import Fields from '@site/src/components/Fields';

import Field from '@site/src/components/Field';

<Fields filters={true}>


<Field
  common={true}
  defaultValue={true}
  enumValues={null}
  examples={[true,false]}
  name={"drop_field"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  templateable={false}
  type={"bool"}
  unit={null}
  >

### drop_field

If `true` the[`field`](#field) will be dropped after parsing.


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

The field to apply the split on.


</Field>


<Field
  common={true}
  defaultValue={null}
  enumValues={null}
  examples={[["timestamp","level","message"]]}
  name={"field_names"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={true}
  templateable={false}
  type={"[string]"}
  unit={null}
  >

### field_names

The field names assigned to the resulting tokens, in order.


</Field>


<Field
  common={true}
  defaultValue={"whitespace"}
  enumValues={null}
  examples={[","]}
  name={"separator"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  templateable={false}
  type={"[string]"}
  unit={null}
  >

### separator

The separator to split the field on. If no separator is given, it will split on whitespace.


</Field>


<Field
  common={true}
  defaultValue={null}
  enumValues={null}
  examples={[]}
  name={"types"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  templateable={false}
  type={"table"}
  unit={null}
  >

### types

Key/Value pairs representing mapped log field types.

<Fields filters={false}>


<Field
  common={true}
  defaultValue={null}
  enumValues={{"bool":"Coerces `\"true\"`/`/\"false\"`, `\"1\"`/`\"0\"`, and `\"t\"`/`\"f\"` values into boolean.","float":"Coerce to a 64 bit float.","int":"Coerce to a 64 bit integer.","string":"Coerce to a string.","timestamp":"Coerces to a Vector timestamp. [`strptime` specificiers][urls.strptime_specifiers] must be used to parse the string."}}
  examples={[{"name":"status","value":"int"},{"name":"duration","value":"float"},{"name":"success","value":"bool"},{"name":"timestamp","value":"timestamp|%s","comment":"unix"},{"name":"timestamp","value":"timestamp|%+","comment":"iso8601 (date and time)"},{"name":"timestamp","value":"timestamp|%F","comment":"iso8601 (date)"},{"name":"timestamp","value":"timestamp|%a %b %e %T %Y","comment":"custom strptime format"}]}
  name={"*"}
  nullable={false}
  path={"types"}
  relevantWhen={null}
  required={true}
  templateable={false}
  type={"string"}
  unit={null}
  >

#### *

A definition of log field type conversions. They key is the log field name and the value is the type. [`strptime` specifiers][urls.strptime_specifiers] are supported for the `timestamp` type.


</Field>


</Fields>

</Field>


</Fields>

## Output

Given the following log line:

```json
{
  "message": "5.86.210.12,zieme4647,19/06/2019:17:20:49 -0400,GET /embrace/supply-chains/dynamic/vertical,201,20574"
}
```

And the following configuration:

```toml
[transforms.<transform-id>]
type = "split"
field = "message"
fields = ["remote_addr", "user_id", "timestamp", "message", "status", "bytes"]
  [transforms.<transform-id>.types]
    status = "int"
    bytes = "int"
```

A [`log` event][docs.data-model.log] will be output with the following structure:

```javascript
{
  // ... existing fields
  "remote_addr": "5.86.210.12",
  "user_id": "zieme4647",
  "timestamp": "19/06/2019:17:20:49 -0400",
  "message": "GET /embrace/supply-chains/dynamic/vertical",
  "status": 201,
  "bytes": 20574
}
```

A few things to note about the output:

1. The `message` field was overwritten.
2. The `status` and `bytes` fields are integers because of type coercion.

## How It Works

### Environment Variables

Environment variables are supported through all of Vector's configuration.
Simply add `${MY_ENV_VAR}` in your Vector configuration file and the variable
will be replaced before being evaluated.

You can learn more in the [Environment Variables][docs.configuration#environment-variables]
section.


[docs.configuration#environment-variables]: /docs/setup/configuration#environment-variables
[docs.data-model#log]: /docs/about/data-model#log
[docs.data-model.log]: /docs/about/data-model/log
[urls.strptime_specifiers]: https://docs.rs/chrono/0.3.1/chrono/format/strftime/index.html
