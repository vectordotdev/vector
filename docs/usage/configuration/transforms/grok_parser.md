---
title: "grok_parser transform" 
sidebar_label: "grok_parser"
---

The `grok_parser` transform accepts [`log`][docs.data-model.log] events and allows you to parse a log field value with [Grok][urls.grok].

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
[transforms.my_transform_id]
  type = "grok_parser" # enum
  inputs = ["my-source-id"]
  pattern = "%{TIMESTAMP_ISO8601:timestamp} %{LOGLEVEL:level} %{GREEDYDATA:message}"
```

</TabItem>
<TabItem value="advanced">

```coffeescript
[transforms.my_transform_id]
  # REQUIRED - General
  type = "grok_parser" # enum
  inputs = ["my-source-id"]
  pattern = "%{TIMESTAMP_ISO8601:timestamp} %{LOGLEVEL:level} %{GREEDYDATA:message}"
  
  # OPTIONAL - General
  drop_field = true # default
  field = "message" # default
  
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

You can learn more

## Options

import Option from '@site/src/components/Option';
import Options from '@site/src/components/Options';

<Options filters={true}>


<Option
  defaultValue={true}
  enumValues={null}
  examples={[true,false]}
  name={"drop_field"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"bool"}
  unit={null}>

### drop_field

If `true` will drop the specified `field` after parsing.


</Option>


<Option
  defaultValue={"message"}
  enumValues={null}
  examples={["message"]}
  name={"field"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"string"}
  unit={null}>

### field

The log field to execute the `pattern` against. Must be a `string` value.


</Option>


<Option
  defaultValue={null}
  enumValues={null}
  examples={["%{TIMESTAMP_ISO8601:timestamp} %{LOGLEVEL:level} %{GREEDYDATA:message}"]}
  name={"pattern"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={true}
  simple={true}
  type={"string"}
  unit={null}>

### pattern

The [Grok pattern][urls.grok_patterns]


</Option>


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

## How It Works

### Available Patterns

Vector uses the Rust [`grok` library][urls.rust_grok_library]. All patterns
[listed here][urls.grok_patterns] are supported. It is recommended to use
maintained patterns when possible since they can be improved over time by
the community.

### Debugging

We recommend the [Grok debugger][urls.grok_debugger] for Grok testing.

### Environment Variables

Environment variables are supported through all of Vector's configuration.
Simply add `${MY_ENV_VAR}` in your Vector configuration file and the variable
will be replaced before being evaluated.

You can learn more in the [Environment Variables][docs.configuration#environment-variables]
section.

### Performance

Grok is approximately 50% slower than the [`regex_parser` transform][docs.transforms.regex_parser].
We plan to add a [performance test][docs.performance] for this in the future.
While this is still plenty fast for most use cases we recommend using the
[`regex_parser` transform][docs.transforms.regex_parser] if you are experiencing
performance issues.

## Troubleshooting

The best place to start with troubleshooting is to check the
[Vector logs][docs.monitoring#logs]. This is typically located at
`/var/log/vector.log`, then proceed to follow the
[Troubleshooting Guide][docs.troubleshooting].

If the [Troubleshooting Guide][docs.troubleshooting] does not resolve your
issue, please:

1. Check for any [open `grok_parser_transform` issues][urls.grok_parser_transform_issues].
2. If encountered a bug, please [file a bug report][urls.new_grok_parser_transform_bug].
3. If encountered a missing feature, please [file a feature request][urls.new_grok_parser_transform_enhancement].
4. If you need help, [join our chat/forum community][urls.vector_chat]. You can post a question and search previous questions.


### Alternatives

Finally, consider the following alternatives:

* [`lua` transform][docs.transforms.lua]
* [`regex_parser` transform][docs.transforms.regex_parser]
* [`split` transform][docs.transforms.split]
* [`tokenizer` transform][docs.transforms.tokenizer]

## Resources

* [**Issues**][urls.grok_parser_transform_issues] - [enhancements][urls.grok_parser_transform_enhancements] - [bugs][urls.grok_parser_transform_bugs]
* [**Source code**][urls.grok_parser_transform_source]
* [**Grok Debugger**][urls.grok_debugger]
* [**Grok Patterns**][urls.grok_patterns]


[docs.configuration#environment-variables]: ../../../usage/configuration#environment-variables
[docs.data-model.log]: ../../../about/data-model/log.md
[docs.monitoring#logs]: ../../../usage/administration/monitoring.md#logs
[docs.performance]: ../../../performance.md
[docs.transforms.lua]: ../../../usage/configuration/transforms/lua.md
[docs.transforms.regex_parser]: ../../../usage/configuration/transforms/regex_parser.md
[docs.transforms.split]: ../../../usage/configuration/transforms/split.md
[docs.transforms.tokenizer]: ../../../usage/configuration/transforms/tokenizer.md
[docs.troubleshooting]: ../../../usage/guides/troubleshooting.md
[urls.grok]: http://grokdebug.herokuapp.com/
[urls.grok_debugger]: http://grokdebug.herokuapp.com/
[urls.grok_parser_transform_bugs]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22transform%3A+grok_parser%22+label%3A%22Type%3A+bug%22
[urls.grok_parser_transform_enhancements]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22transform%3A+grok_parser%22+label%3A%22Type%3A+enhancement%22
[urls.grok_parser_transform_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22transform%3A+grok_parser%22
[urls.grok_parser_transform_source]: https://github.com/timberio/vector/tree/master/src/transforms/grok_parser.rs
[urls.grok_patterns]: https://github.com/daschl/grok/tree/master/patterns
[urls.new_grok_parser_transform_bug]: https://github.com/timberio/vector/issues/new?labels=transform%3A+grok_parser&labels=Type%3A+bug
[urls.new_grok_parser_transform_enhancement]: https://github.com/timberio/vector/issues/new?labels=transform%3A+grok_parser&labels=Type%3A+enhancement
[urls.rust_grok_library]: https://github.com/daschl/grok
[urls.strftime_specifiers]: https://docs.rs/chrono/0.3.1/chrono/format/strftime/index.html
[urls.vector_chat]: https://chat.vector.dev
