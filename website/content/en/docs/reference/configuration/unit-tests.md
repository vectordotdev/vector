---
title: Unit testing Vector configurations
short: Unit tests
weight: 5
aliases: [
  "/docs/reference/tests",
  "/docs/reference/configuration/tests",
  "/guides/level-up/unit-testing"
]
---

Vector enables you to [unit test] [transforms] in your processing [pipeline]. Unit tests in Vector
work just like unit tests in most programming languages:

1. Provide a set of [inputs](#inputs) to a transform (or to [multiple transforms](#multiple) chained
  together).
1. Specify the expected [outputs](#outputs) from the changes made by the transform (or multiple
  transforms).
1. Receive directly actionable feedback from any test failures.

Unit tests can serve as a useful guardrail when running in Vector in production settings where you
need to ensure that your topology doesn't exhibit unexpected behavior and generally improve the
maintainability of your Vector pipelines, particularly in larger and more complex pipelines.

## Running unit tests

You can execute tests within a [configuration](#configuring) file using Vector's
[`test`][vector_test] subcommand:

```bash
vector test /etc/vector/vector.toml
```

You can also specify multiple configuration files to test:

```bash
vector test /etc/vector/pipeline1.toml /etc/vector/pipeline2.toml
```

Glob patterns are also supported:

```bash
vector test /etc/vector/*.toml
```

Specifying multiple files is useful if you want to, for example, keep your unit tests in a separate
file from your pipeline configuration. Vector always treats multiple files as a single, unified
configuration.

## Verifying output {#verifying}

You can use [VRL assertions][assertions] to verify that the output of the transform(s) being tested
conforms to your expectations. VRL provides two assertion functions:

* [`assert`][assert] takes a [Boolean expression][boolean] as its first argument. If the Boolean
  resolves to `false`, the test fails and Vector logs an error.
* [`assert_eq`][assert_eq] takes any two values as its first two arguments. If those two values
  aren't equal, the test fails and Vector logs an error.

With both functions, you can supply a custom log message to be emitted if the assertion fails:

```ruby
# Named argument
assert!(1 == 2, message: "the rules of arithmetic have been violated")
assert_eq!(1, 2, message: "the rules of arithmetic have been violated")

# Positional arguments are also valid
assert!(1 == 2, "the rules of arithmetic have been violated")
assert_eq!(1, 2, "the rules of arithmetic have been violated")
```

{{< info title="Make your assertions infallible" >}}
We recommend making `assert` and `assert_eq` invocations in unit tests [infallible] by applying the
bang (`!`) syntax, as in `assert!(1 == 1)` rather than `assert(1 == 1)`. The `!` indicates that the
VRL program should abort if the condition fails.

[infallible]: /docs/reference/vrl/#fallibility
{{< /info >}}

If you use the `assert` function, you need to pass a [Boolean expression][boolean] to the function
as the first argument. Especially useful when writing Boolean expressions are the [type
functions][type], functions like [`exists`][exists], [`includes`][includes],
[`is_nullish`][is_nullish] and [`contains`][contains], and VRL [comparisons]. Here's an example
usage of a Boolean expression passed to an `assert` function:

```toml
[[tests.outputs.conditions]]
type = "vrl"
source = '''
assert!(is_string(.message) && is_timestamp(.timestamp) && !exists(.other))
'''
```

In this case, the VRL program (under `source`) evaluates to a single Boolean that expresses the
following:

* The `message` field must be a string
* The `timestamp` field must be a valid timestamp
* The `other` field must not exist

It's also possible to break a test up into multiple `assert` or `assert_eq` statements:

```toml
source = '''
assert!(exists(.message), "no message field provided")
assert!(!is_nullish(.message), "message field is an empty string")
assert!(is_string(.message), "message field has as unexpected type")
assert_eq!(.message, "success", "message field had an unexpected value")
assert!(exists(.timestamp), "no timestamp provided")
assert!(is_timestamp(.timestamp), "timestamp is invalid")
assert!(!exists(.other), "extraneous other field doesn't belong")
'''
```

It's also possible to store the Boolean expressions in variables rather than passing the entire
statement to an `assert` function:

```toml
source = '''
message_field_valid = exists(.message) &&
  !is_nullish(.message) &&
  .message == "success"

assert!(message_field_valid)
'''
```

## Example unit test configuration {#example}

Below is an annotated example of a unit test suite for a transform called `add_metadata`, which
adds a unique ID and a timestamp to log events:

```toml
# The transform being tested is a Vector Remap Language (VRL) transform that
# adds two fields to each incoming log event: a timestamp and a unique ID
[transforms.add_metadata]
type = "remap"
inputs = []
source = '''
.timestamp = now()
.id = uuid_v4()
'''

# Here we begin configuring our test suite
[[tests]]
name = "Test for the add_metadata transform"

# The inputs for the test
[[tests.inputs]]
insert_at = "add_metadata" # The transform into which the testing event is inserted
type = "log"               # The event type (either log or metric)

# The test log event that is passed to the `add_metadata` transform
[tests.inputs.log_fields]
message = "successful transaction"
code = 200

# The expected outputs of the test
[[tests.outputs]]
extract_from = "add_metadata" # The transform from which the resulting event is extracted

# The declaration of what we expect
[[tests.outputs.conditions]]
type = "vrl"
source = '''
is_timestamp(.timestamp) &&
  is_string(.id) &&
  .message == "successful transaction"
'''
```

This example represents a complete test of the `add_metadata` transform, include test `inputs`
and expected `outputs` drawn from a specific transform.

{{< success title="Multiple config formats available" >}}
The unit testing example above is in TOML but Vector also supports YAML and JSON as configuration
formats.
{{< /success >}}

## Configuring unit tests {#configuring}

Unit tests in Vector live alongside your topology configuration. You can specify your tests in the
same config file alongside your transform definitions or split them out into a separate file.

Unit tests need are specified inside of a `tests` array. Each test requires a `name`:

```toml
[[tests]]
name = "test 1"
# Other test config

[[tests]]
name = "test_2"
# Other test config

# etc.
```

Inside each test definition, you need to specify two things:

* An array of `inputs` that provides [input events](#inputs) for the test.
* An array of `outputs` that provides [expected outputs](#outputs) for the test.

Optionally, you can specify a `no_outputs_from` list of transforms that must *not* output events
in order for the test to pass. Here's an example:

```toml
[[tests]]
name = "skip_remove_fields"
no_outputs_from = ["remove_extraneous_fields"]
```

In this case, the output from some transform called `remove_extraneous_fields` is

### Inputs

In in the `inputs` array for the test, you have these options:

Parameter | Type | Description
:---------|:-----|:-----------
`insert_at` | string (name of transform) | The name of the transform into which the test input is inserted. This is particularly useful when you want to test only a subset of a transform pipeline.
`value` | string (raw event value) | A raw string value to act as an input event. Use only in cases where events are raw strings and not structured objects with event fields.
`log_fields` | object | If the transform handles [log events](#logs), these are the key/value pairs that comprise the input event.
`metric` | object | If the transform handles [metric events](#metrics), these are the fields that comprise that metric. Subfields include `name`, `tags`, `kind`, and others.

Here's an example `inputs` declaration:

```toml
[transforms.add_metadata]
# transform config

[[tests]]
name = "Test add_metadata transform"

[[tests.inputs]]
insert_at = "add_metadata"

[tests.inputs.log_fields]
message = "<102>1 2020-12-22T15:22:31.111Z vector-user.biz su 2666 ID389 - Something went wrong"
tags = { environment = "production" }
```

### Outputs

In the `outputs` array of your unit testing configuration you specify two things:

Parameter | Type | Description
:---------|:-----|:-----------
`extract_from` | string (name of transform) | The transform whose output you want to test.
`conditions` | array of objects | The [VRL conditions](#verifying) to run against the output.

Each condition in the `conditions` array has two fields:

Parameter | Type | Description
:---------|:-----|:-----------
`type` | string | The type of condition you're providing. As the original `check_fields` syntax is now deprecated, this defaults to [`vrl`][vrl], although [`datadog_search`][datadog_search] syntax is also valid.
`source` | string (VRL Boolean expression) | Explained in detail [above](#verifying).

Here's an example `outputs` declaration:

```toml
[[tests.outputs]]
extract_from = "add_metadata"

[[tests.outputs.conditions]]
type = "vrl"
source = '''
is_string(.id) && exists(.tags)
'''
```

{{< danger title="`check_fields` conditions now deprecated" >}}
Vector initially provided a `check_fields` condition type that enabled you to specify Boolean
test conditions using a special configuration-based system. `check_fields` is now deprecated. We
strongly recommend converting any existing `check_fields` tests to `vrl` conditions.
{{< /danger >}}

### Event types

There are currently two event types that you can unit test in Vector:

* [`log`](#logs) events
* [`metric`](#metrics) events

#### Logs

As explained in the section on [inputs](#inputs) above, when testing log events you have can specify
either a structured event [object] or a raw [string].

##### Object

To specify a structured log event as your test input, use `log_fields`:

```toml
[tests.inputs.log_fields]
message = "successful transaction"
code = 200
id = "38c5b0d0-5e7e-42aa-ae86-2b642ad2d1b8"
```

##### Raw string value

To specify a raw string value for a log event, use `value`:

```toml
[[tests.inputs]]
insert_at = "add_metadata"
value = "<102>1 2020-12-22T15:22:31.111Z vector-user.biz su 2666 ID389 - Something went wrong"
```

#### Metrics

You can specify the fields in a metric event to be unit tested using a `metric` object:

```toml
[[tests.inputs]]
insert_at = "my_metric_transform"
type = "metric"

[tests.inputs.metric]
name = "count"
kind = "absolute"
counter = { value = 1 }
```

Here's a full end-to-end example of unit testing a metric through a transform:

```toml
[transforms.add_env_to_metric]
type = "remap"
inputs = []
source = '''
env, err = get_env_var!("ENV")
if err != null {
  log(err, level: "error")
}
tags.environment = env
'''

[[tests]]
name = "add_unique_id_test"

[[tests.inputs]]
insert_at = "add_unique_id_to_metric"
type = "metric"

[tests.inputs.metric]
name = "website_hits"
kind = "absolute"
counter = { value = 1 }

[[tests.outputs]]
extract_from = "add_unique_id_to_metric"

[[tests.outputs.conditions]]
type = "vrl"
source = '''
.name == "website_hits" &&
  .kind == "absolute" &&
  .tags.environment == "production"
'''
```

[assert]: /docs/reference/vrl/functions/#assert
[assert_eq]: /docs/reference/vrl/functions/#assert_eq
[assertions]: /docs/reference/vrl#assertions
[boolean]: /docs/reference/vrl/#boolean-expressions
[comparisons]: /docs/reference/vrl/expressions/#comparison
[contains]: /docs/reference/vrl/functions/#contains
[datadog_search]: https://docs.datadoghq.com/logs/explorer/search_syntax
[exists]: /docs/reference/vrl/functions/#exists
[filter]: /docs/reference/configuration/transforms/filter
[includes]: /docs/reference/vrl/functions/#includes
[is_nullish]: /docs/reference/vrl/functions/#is_nullish
[logs]: /docs/about/under-the-hood/architecture/data-model/log
[metrics]: /docs/about/under-the-hood/architecture/data-model/metric
[pipeline]: /docs/reference/glossary/#pipeline
[remap]: /docs/reference/configuration/transforms/remap
[transforms]: /docs/reference/glossary/#transform
[type]: /docs/reference/vrl/functions/#type-functions
[unit test]: https://en.wikipedia.org/wiki/Unit_testing
[vector_test]: /docs/reference/cli#test
[vector_tests]: https://github.com/vectordotdev/vector/tree/master/tests/behavior/transforms
[vrl]: /docs/reference/vrl
