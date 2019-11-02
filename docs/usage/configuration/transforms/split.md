---
event_types: ["log","log"]
issues_url: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22transform%3A+split%22
output_types: ["log"]
sidebar_label: "split|[\"log\",\"log\"]"
source_url: https://github.com/timberio/vector/tree/master/src/transforms/split.rs
status: "prod-ready"
title: "split transform" 
---

The `split` transform accepts [`log`][docs.data-model.log] events and allows you to split a field's value on a given separator and zip the tokens into ordered field names.

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
[transforms.my_transform_id]
  # REQUIRED - General
  type = "split" # example, must be: "split"
  inputs = ["my-source-id"] # example
  field_names = ["timestamp", "level", "message"] # example
  
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

</TabItem>
<TabItem value="advanced">

<CodeHeader fileName="vector.toml" learnMoreUrl="/usage/configuration" />

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

</TabItem>

</Tabs>

## Options

import Option from '@site/src/components/Option';
import Options from '@site/src/components/Options';

<Options filters={true}>


<Option
  common={false}
  defaultValue={true}
  enumValues={null}
  examples={[true,false]}
  name={"drop_field"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  type={"bool"}
  unit={null}>

### drop_field

If `true` the `field` will be dropped after parsing.


</Option>


<Option
  common={false}
  defaultValue={"message"}
  enumValues={null}
  examples={["message"]}
  name={"field"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  type={"string"}
  unit={null}>

### field

The field to apply the split on.


</Option>


<Option
  common={true}
  defaultValue={null}
  enumValues={null}
  examples={[["timestamp","level","message"]]}
  name={"field_names"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={true}
  type={"[string]"}
  unit={null}>

### field_names

The field names assigned to the resulting tokens, in order.


</Option>


<Option
  common={false}
  defaultValue={"whitespace"}
  enumValues={null}
  examples={[","]}
  name={"separator"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  type={"[string]"}
  unit={null}>

### separator

The separator to split the field on. If no separator is given, it will split on whitespace.


</Option>


<Option
  common={true}
  defaultValue={null}
  enumValues={null}
  examples={[]}
  name={"types"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  type={"table"}
  unit={null}>

### types

Key/Value pairs representing mapped log field types.

<Options filters={false}>


<Option
  common={true}
  defaultValue={null}
  enumValues={{"bool":"Coerces `\"true\"`/`/\"false\"`, `\"1\"`/`\"0\"`, and `\"t\"`/`\"f\"` values into boolean.","float":"Coerce to a 64 bit float.","int":"Coerce to a 64 bit integer.","string":"Coerce to a string.","timestamp":"Coerces to a Vector timestamp. [`strftime` specificiers][urls.strftime_specifiers] must be used to parse the string."}}
  examples={[{"name":"status","value":"int"},{"name":"duration","value":"float"},{"name":"success","value":"bool"},{"name":"timestamp","value":"timestamp|%s","comment":"unix"},{"name":"timestamp","value":"timestamp|%+","comment":"iso8601 (date and time)"},{"name":"timestamp","value":"timestamp|%F","comment":"iso8601 (date)"},{"name":"timestamp","value":"timestamp|%a %b %e %T %Y","comment":"custom strftime format"}]}
  name={"*"}
  nullable={false}
  path={"types"}
  relevantWhen={null}
  required={true}
  type={"string"}
  unit={null}>

#### *

A definition of log field type conversions. They key is the log field name and the value is the type. [`strftime` specifiers][urls.strftime_specifiers] are supported for the `timestamp` type.


</Option>


</Options>

</Option>


</Options>

## Input/Output

Given the following log line:

{% code-tabs %}
{% code-tabs-item title="log" %}
```json
{
  "message": "5.86.210.12,zieme4647,19/06/2019:17:20:49 -0400,GET /embrace/supply-chains/dynamic/vertical,201,20574"
}
```
{% endcode-tabs-item %}
{% endcode-tabs %}

And the following configuration:

{% code-tabs %}
{% code-tabs-item title="vector.toml" %}
```coffeescript
[transforms.<transform-id>]
type = "split"
field = "message"
fields = ["remote_addr", "user_id", "timestamp", "message", "status", "bytes"]
  [transforms.<transform-id>.types]
    status = "int"
    bytes = "int"
```
{% endcode-tabs-item %}
{% endcode-tabs %}

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


[docs.configuration#environment-variables]: ../../../usage/configuration#environment-variables
[docs.data-model.log]: ../../../about/data-model/log.md
[urls.strftime_specifiers]: https://docs.rs/chrono/0.3.1/chrono/format/strftime/index.html
