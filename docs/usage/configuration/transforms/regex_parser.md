---
title: "regex_parser transform" 
sidebar_label: "regex_parser"
---

The `regex_parser` transform accepts [`log`][docs.data-model.log] events and allows you to parse a log field's value with a [Regular Expression][urls.regex].

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
  type = "regex_parser" # enum
  inputs = ["my-source-id"]
  regex = "^(?P<timestamp>.*) (?P<level>\\w*) (?P<message>.*)$"
```

</TabItem>
<TabItem value="advanced">

```coffeescript
[transforms.my_transform_id]
  # REQUIRED - General
  type = "regex_parser" # enum
  inputs = ["my-source-id"]
  regex = "^(?P<timestamp>.*) (?P<level>\\w*) (?P<message>.*)$"
  
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

If the specified `field` should be dropped (removed) after parsing.


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

The log field to parse. See [Failed Parsing](#failed-parsing) for more info.


</Option>


<Option
  defaultValue={null}
  enumValues={null}
  examples={["^(?P<timestamp>.*) (?P<level>\\w*) (?P<message>.*)$"]}
  name={"regex"}
  nullable={false}
  path={null}
  relevantWhen={null}
  required={true}
  simple={true}
  type={"string"}
  unit={null}>

### regex

The Regular Expression to apply. Do not inlcude the leading or trailing `/`. See [Failed Parsing](#failed-parsing) and [Regex Debugger](#regex-debugger) for more info.


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

Key/Value pairs representing mapped log field types. See [Regex Syntax](#regex-syntax) for more info.

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

Given the following log line:

{% code-tabs %}
{% code-tabs-item title="log" %}
```json
{
  "message": "5.86.210.12 - zieme4647 5667 [19/06/2019:17:20:49 -0400] \"GET /embrace/supply-chains/dynamic/vertical\" 201 20574"
}
```
{% endcode-tabs-item %}
{% endcode-tabs %}

And the following configuration:

{% code-tabs %}
{% code-tabs-item title="vector.toml" %}
```coffeescript
[transforms.<transform-id>]
  type = "regex_parser"
  field = "message"
  regex = '^(?P<host>[\w\.]+) - (?P<user>[\w]+) (?P<bytes_in>[\d]+) \[(?P<timestamp>.*)\] "(?P<method>[\w]+) (?P<path>.*)" (?P<status>[\d]+) (?P<bytes_out>[\d]+)$'

[transforms.<transform-id>.types]
  bytes_int = "int"
  timestamp = "timestamp|%d/%m/%Y:%H:%M:%S %z"
  status = "int"
  bytes_out = "int"
```
{% endcode-tabs-item %}
{% endcode-tabs %}

A [`log` event][docs.data-model.log] will be output with the following structure:

```javascript
{
  // ... existing fields
  "bytes_in": 5667,
  "host": "5.86.210.12",
  "user_id": "zieme4647",
  "timestamp": <19/06/2019:17:20:49 -0400>,
  "message": "GET /embrace/supply-chains/dynamic/vertical",
  "status": 201,
  "bytes": 20574
}
```

Things to note about the output:

1. The `message` field was overwritten.
2. The `bytes_in`, `timestamp`, `status`, and `bytes_out` fields were coerced.


## How It Works

### Environment Variables

Environment variables are supported through all of Vector's configuration.
Simply add `${MY_ENV_VAR}` in your Vector configuration file and the variable
will be replaced before being evaluated.

You can learn more in the [Environment Variables][docs.configuration#environment-variables]
section.

### Failed Parsing

If the `field` value fails to parse against the provided `regex` then an error
will be [logged][docs.monitoring#logs] and the event will be kept or discarded
depending on the `drop_failed` value.

A failure includes any event that does not successfully parse against the
provided `regex`. This includes bad values as well as events missing the
specified `field`.

### Performance

The `regex_parser` source has been involved in the following performance tests:

* [`regex_parsing_performance`][urls.regex_parsing_performance_test]

Learn more in the [Performance][docs.performance] sections.

### Regex Debugger

To test the validity of the `regex` option, we recommend the [Golang Regex
Tester][urls.regex_tester] as it's Regex syntax closely 
follows Rust's.

### Regex Syntax

Vector follows the [documented Rust Regex syntax][urls.rust_regex_syntax] since
Vector is written in Rust. This syntax follows a Perl-style regular expression
syntax, but lacks a few features like look around and backreferences.

#### Named Captures

You can name Regex captures with the `<name>` syntax. For example:

```
^(?P<timestamp>.*) (?P<level>\w*) (?P<message>.*)$
```

Will capture `timestamp`, `level`, and `message`. All values are extracted as
`string` values and must be coerced with the `types` table.

More info can be found in the [Regex grouping and flags
documentation][urls.regex_grouping_and_flags].

#### Flags

Regex flags can be toggled with the `(?flags)` syntax. The available flags are:

| Flag | Descriuption |
| :--- | :----------- |
| `i`  | case-insensitive: letters match both upper and lower case |
| `m`  | multi-line mode: ^ and $ match begin/end of line |
| `s`  | allow . to match `\n` |
| `U`  | swap the meaning of `x*` and `x*?` |
| `u`  | Unicode support (enabled by default) |
| `x`  | ignore whitespace and allow line comments (starting with `#`)

For example, to enable the case-insensitive flag you can write:

```
(?i)Hello world
```

More info can be found in the [Regex grouping and flags
documentation][urls.regex_grouping_and_flags].

## Troubleshooting

The best place to start with troubleshooting is to check the
[Vector logs][docs.monitoring#logs]. This is typically located at
`/var/log/vector.log`, then proceed to follow the
[Troubleshooting Guide][docs.troubleshooting].

If the [Troubleshooting Guide][docs.troubleshooting] does not resolve your
issue, please:

1. Check for any [open `regex_parser_transform` issues][urls.regex_parser_transform_issues].
2. If encountered a bug, please [file a bug report][urls.new_regex_parser_transform_bug].
3. If encountered a missing feature, please [file a feature request][urls.new_regex_parser_transform_enhancement].
4. If you need help, [join our chat/forum community][urls.vector_chat]. You can post a question and search previous questions.


### Alternatives

Finally, consider the following alternatives:

* [`grok_parser` transform][docs.transforms.grok_parser]
* [`lua` transform][docs.transforms.lua]
* [`split` transform][docs.transforms.split]
* [`tokenizer` transform][docs.transforms.tokenizer]

## Resources

* [**Issues**][urls.regex_parser_transform_issues] - [enhancements][urls.regex_parser_transform_enhancements] - [bugs][urls.regex_parser_transform_bugs]
* [**Source code**][urls.regex_parser_transform_source]
* [**Regex Tester**][urls.regex_tester]
* [**Rust Regex Syntax**][urls.rust_regex_syntax]


[docs.configuration#environment-variables]: ../../../usage/configuration#environment-variables
[docs.data-model.log]: ../../../about/data-model/log.md
[docs.monitoring#logs]: ../../../usage/administration/monitoring.md#logs
[docs.performance]: ../../../performance.md
[docs.transforms.grok_parser]: ../../../usage/configuration/transforms/grok_parser.md
[docs.transforms.lua]: ../../../usage/configuration/transforms/lua.md
[docs.transforms.split]: ../../../usage/configuration/transforms/split.md
[docs.transforms.tokenizer]: ../../../usage/configuration/transforms/tokenizer.md
[docs.troubleshooting]: ../../../usage/guides/troubleshooting.md
[urls.new_regex_parser_transform_bug]: https://github.com/timberio/vector/issues/new?labels=transform%3A+regex_parser&labels=Type%3A+bug
[urls.new_regex_parser_transform_enhancement]: https://github.com/timberio/vector/issues/new?labels=transform%3A+regex_parser&labels=Type%3A+enhancement
[urls.regex]: https://en.wikipedia.org/wiki/Regular_expression
[urls.regex_grouping_and_flags]: https://docs.rs/regex/1.1.7/regex/#grouping-and-flags
[urls.regex_parser_transform_bugs]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22transform%3A+regex_parser%22+label%3A%22Type%3A+bug%22
[urls.regex_parser_transform_enhancements]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22transform%3A+regex_parser%22+label%3A%22Type%3A+enhancement%22
[urls.regex_parser_transform_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22transform%3A+regex_parser%22
[urls.regex_parser_transform_source]: https://github.com/timberio/vector/tree/master/src/transforms/regex_parser.rs
[urls.regex_parsing_performance_test]: https://github.com/timberio/vector-test-harness/tree/master/cases/regex_parsing_performance
[urls.regex_tester]: https://regex-golang.appspot.com/assets/html/index.html
[urls.rust_regex_syntax]: https://docs.rs/regex/1.1.7/regex/#syntax
[urls.strftime_specifiers]: https://docs.rs/chrono/0.3.1/chrono/format/strftime/index.html
[urls.vector_chat]: https://chat.vector.dev
