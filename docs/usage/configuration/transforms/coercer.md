---
title: "coercer transform" 
sidebar_label: "coercer"
---

The `coercer` transform accepts [`log`][docs.data-model.log] events and allows you to coerce log fields into fixed types.

## Example

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';


```coffeescript
[transforms.my_transform_id]
  type = "coercer" # enum
  inputs = ["my-source-id"]
```



You can learn more

## Options

import Option from '@site/src/components/Option';
import Options from '@site/src/components/Options';

<Options filters={true}>


<Option
  defaultValue={null}
  enumValues={null}
  examples={[]}
  name={"types"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"table"}
  unit={null}>

### types

Key/Value pairs representing mapped log field types.

<Options filters={false}>


<Option
  defaultValue={null}
  enumValues={{"bool":"Coerces `\"true\"`/`/\"false\"`, `\"1\"`/`\"0\"`, and `\"t\"`/`\"f\"` values into boolean.","float":"Coerce to a 64 bit float.","int":"Coerce to a 64 bit integer.","string":"Coerce to a string.","timestamp":"Coerces to a Vector timestamp. [`strftime` specificiers][urls.strftime_specifiers] must be used to parse the string."}}
  examples={[{"name":"status","value":"int"},{"name":"duration","value":"float"},{"name":"success","value":"bool"},{"name":"timestamp","value":"timestamp|%s","comment":"unix"},{"name":"timestamp","value":"timestamp|%+","comment":"iso8601 (date and time)"},{"name":"timestamp","value":"timestamp|%F","comment":"iso8601 (date)"},{"name":"timestamp","value":"timestamp|%a %b %e %T %Y","comment":"custom strftime format"}]}
  name={"*"}
  nullable={false}
  path={"types"}
  relevantWhen={null}
  required={true}
  simple={true}
  type={"string"}
  unit={null}>

#### *

A definition of log field type conversions. They key is the log field name and the value is the type. [`strftime` specifiers][urls.strftime_specifiers] are supported for the `timestamp` type.


</Option>


</Options>

</Option>


</Options>

## Input/Output

Given the following input event:

{% code-tabs %}
{% code-tabs-item title="log" %}
```json
{
  // ... existing fields
  "bytes_in": "5667",
  "bytes_out": "20574",
  "host": "5.86.210.12",
  "message": "GET /embrace/supply-chains/dynamic/vertical",
  "status": "201",
  "timestamp": "19/06/2019:17:20:49 -0400",
  "user_id": "zieme4647"
}
```
{% endcode-tabs-item %}
{% endcode-tabs %}

And the following configuration:

{% code-tabs %}
{% code-tabs-item title="vector.toml" %}
```coffeescript
[transforms.<transform-id>]
  type = "coercer"

[transforms.<transform-id>.types]
  bytes_in = "int"
  bytes_out = "int"
  timestamp = "timestamp|%d/%m/%Y:%H:%M:%S %z"
  status = "int"
```
{% endcode-tabs-item %}
{% endcode-tabs %}

A [`log` event][docs.data-model.log] will be output with the following structure:

```javascript
{
  // ... existing fields
  "bytes_in": 5667,
  "bytes_out": 20574,
  "host": "5.86.210.12",
  "message": "GET /embrace/supply-chains/dynamic/vertical",
  "status": 201,
  "timestamp": <19/06/2019:17:20:49 -0400>,
  "user_id": "zieme4647"
}
```

## How It Works

### Environment Variables

Environment variables are supported through all of Vector's configuration.
Simply add `${MY_ENV_VAR}` in your Vector configuration file and the variable
will be replaced before being evaluated.

You can learn more in the [Environment Variables][docs.configuration#environment-variables]
section.

## Troubleshooting

The best place to start with troubleshooting is to check the
[Vector logs][docs.monitoring#logs]. This is typically located at
`/var/log/vector.log`, then proceed to follow the
[Troubleshooting Guide][docs.troubleshooting].

If the [Troubleshooting Guide][docs.troubleshooting] does not resolve your
issue, please:

1. Check for any [open `coercer_transform` issues][urls.coercer_transform_issues].
2. If encountered a bug, please [file a bug report][urls.new_coercer_transform_bug].
3. If encountered a missing feature, please [file a feature request][urls.new_coercer_transform_enhancement].
4. If you need help, [join our chat/forum community][urls.vector_chat]. You can post a question and search previous questions.


### Alternatives

Finally, consider the following alternatives:

* [`lua` transform][docs.transforms.lua]

## Resources

* [**Issues**][urls.coercer_transform_issues] - [enhancements][urls.coercer_transform_enhancements] - [bugs][urls.coercer_transform_bugs]
* [**Source code**][urls.coercer_transform_source]


[docs.configuration#environment-variables]: ../../../usage/configuration#environment-variables
[docs.data-model.log]: ../../../about/data-model/log.md
[docs.monitoring#logs]: ../../../usage/administration/monitoring.md#logs
[docs.transforms.lua]: ../../../usage/configuration/transforms/lua.md
[docs.troubleshooting]: ../../../usage/guides/troubleshooting.md
[urls.coercer_transform_bugs]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22transform%3A+coercer%22+label%3A%22Type%3A+bug%22
[urls.coercer_transform_enhancements]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22transform%3A+coercer%22+label%3A%22Type%3A+enhancement%22
[urls.coercer_transform_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22transform%3A+coercer%22
[urls.coercer_transform_source]: https://github.com/timberio/vector/tree/master/src/transforms/coercer.rs
[urls.new_coercer_transform_bug]: https://github.com/timberio/vector/issues/new?labels=transform%3A+coercer&labels=Type%3A+bug
[urls.new_coercer_transform_enhancement]: https://github.com/timberio/vector/issues/new?labels=transform%3A+coercer&labels=Type%3A+enhancement
[urls.strftime_specifiers]: https://docs.rs/chrono/0.3.1/chrono/format/strftime/index.html
[urls.vector_chat]: https://chat.vector.dev
