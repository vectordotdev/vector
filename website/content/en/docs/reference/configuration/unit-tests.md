---
title: Unit testing Vector configurations
short: Unit tests
weight: 5
aliases: ["/docs/reference/tests", "/docs/reference/configuration/tests"]
---

Vector enables you to [unit test] the [transforms] in your processing [pipeline]. Unit tests can
improve the maintainability of your Vector configurations, particularly in larger and more complex
pipelines. Unit tests in Vector work just like unit tests in most programming languages:

1. Provide a set of **inputs** to a transform (or to [multiple transforms](#multiple) chained
  together)
1. Provide expected **outputs** from the changes made by the transform (or multiple transforms)
1. Receive directly actionable feedback on test failures

In general, unit tests can serve as a useful guardrail when running in Vector in production settings
where you need to ensure that your topology doesn't exhibit unexpected behavior.

This doc will begin with an [example](#example) unit test configuration and proceed to a more
reference-style [guide](#configuring).

## Verifying output {#verifying}

You can use [Boolean expressions][boolean] written in [Vector Remap Language][vrl] (VRL) to verify
that your test outputs are what you would expect given your test inputs. Here's an example:

```toml
[[tests.outputs.conditions]]
type = "vrl"
source = '''
is_string(.message) && is_timestamp(.timestamp) && !exists(.other)
'''
```

In this case, the VRL program (under `source`) evaluates to a single Boolean that expresses the
following:

* The `message` field must be a string
* The `timestamp` field must be a valid timestamp
* The `other` field must not exist

{{< success title="VRL documentation" >}}
When writing unit tests, we recommend using the [VRL documentation][vrl] as a steady point of
reference. Especially useful when writing Boolean expressions are the [type functions][type],
functions like [`exists`][exists], [`includes`][includes], and [`contains`][contains], and
[comparisons].

[comparisons]: https://vrl.dev/expressions/#comparisons
[contains]: https://vrl.dev/functions/#contains
[exists]: https://vrl.dev/functions/#exists
[includes]: https://vrl.dev/functions/#includes
[type]: https://vrl.dev/functions/#type-functions
[vrl]: https://vrl.dev
{{< /success >}}

### Only the last expression is evaluated

When writing a VRL condition for your test output, it's important to bear in mind that the condition
passes if the **last expression** provided evaluates to `true`. If you include multiple Boolean
expressions, all but the last one are disregarded. This condition would thus evaluate to `true`:

```ruby
1 == 2
"booper" == "bopper"
true
```

Because of this, we recommend always structuring your conditions as a **single Boolean expression**,
using `&&` (and) and `||` to chain Boolean expressions together when multiple expressions are in
play. The condition immediately above would evaluate to `false`, as we'd expect, if rewritten like
this:

```ruby
1 == 2 && "booper" == "bopper" && true
```

### Multiple lines

For the sake of readability, you can also spread a single Boolean expression across multiple lines.
Both of the following are valid:

```ruby
# Indented
1 == 2 &&
  "booper" == "bopper" &&
  true

# Not indented
1 == 2 &&
"booper" == "bopper" &&
true
```

## Running unit tests

You can execute tests within a configuration file using the [`test`][vector_test] subcommand:

```bash
vector test /etc/vector/vector.toml
```

You can also specify multiple files:

```bash
vector test /etc/vector/*.toml
```

Specifying multiple files can be useful if you want to keep your unit tests in a separate file from
your pipeline configuration. Vector treats the multiple files as a single configuration.

## Example unit test configuration {#example}

Let's start with an annotated unit testing example:

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
name = "transaction_logging_test"

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
is_timestamp(.timestamp)
is_string(.id)
.message == "successful transaction"
'''
```

This example represents a complete test of the `add_metadata` transform, complete with test
`inputs` and `outputs`.

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
name = "skip_modify_transform"
no_outputs_from = ["modify_transform"]
```

In this case,

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
tags.environment = "production"
```

### Outputs

In the `outputs` array of your unit testing configuration you specify two things:

Parameter | Type | Description
:---------|:-----|:-----------
`extract_from` | string (name of transform) | The transform whose output you want to test.
`conditions` | array of objects | The [VRL conditions][verifying] to run against the output.

Each condition in the `conditions` array has two fields:

Parameter | Type | Description
:---------|:-----|:-----------
`type` | string | The type of condition you're providing. As the original `check_fields` syntax is now deprecated, this defaults to  `vrl`.
`source` | string (VRL Boolean expression) | Explained in detail [above](#verifying).


both the expected output
events from the transform(s) you specified in in the [`inputs`](#inputs) array as well the point in
the transform chain from which output events are to be extracted.

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

There are currently three type event types in Vector:

* [`log`](#logs) events
* [`metric`](#metrics) events
* [`raw`](#raw) events


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
counter.value = 1
```

Here's a full end-to-end example of unit testing a metric through a transform:

```toml
[transforms.add_unique_id_to_metric]
type = "remap"
inputs = []
source = '''
.id = uuid_v4()
'''

[[tests]]
name = "add_unique_id_test"

[[tests.inputs]]
insert_at = "add_unique_id_to_metric"
type = "metric"

[tests.inputs.metric]
name = "website_hits"
kind = "absolute"
counter.value = 1

[[tests.outputs]]
extract_from = "add_unique_id_to_metric"

[[tests.outputs.conditions]]
type = "vrl"
source = '''
.name == "website_hits" &&
  .kind == "absolute" &&
  is_string(.id)
'''
```

[boolean]: https://vrl.dev/#boolean-expressions
[filter]: /docs/reference/configuration/transforms/filter
[logs]: /docs/about/under-the-hood/architecture/data-model/log
[metrics]: /docs/about/under-the-hood/architecture/data-model/metric
[pipeline]: /docs/reference/glossary/#pipeline
[remap]: /docs/reference/configuration/transforms/remap
[transforms]: /docs/reference/glossary/#transform
[unit test]: https://en.wikipedia.org/wiki/Unit_testing
[vector_test]: /docs/reference/cli#test
[vector_tests]: https://github.com/timberio/vector/tree/master/tests/behavior/transforms
[vrl]: https://vrl.dev
