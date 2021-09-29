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

If *any* of your unit tests fail, the Vector instance doesn't start up. This makes unit tests a
useful guardrail for running in Vector in production settings where you need to ensure that your
topology doesn't exhibit unexpected behavior.

This doc will begin with an [example](#example) unit test configuration and proceed to a more
reference-style [guide](#configuring).

## Verifying output

You can use [Boolean expressions][boolean] written in [Vector Remap Language][vrl] (VRL) to verify
that your test outputs are what you would expect given your test inputs. Here's an example:

```toml
[[tests.outputs.conditions]]
type = "vrl"
source = '''
is_string(.message) && is_timestamp(.timestamp) && !exists(.other)
'''
```

In this case, the VRL program (under `source`) evaluates to a single Boolean which, if it evaluates
to `true`, expresses that the `message` field of the event is a string

{{< success title="VRL documentation" >}}
When writing unit tests, we recommend using the [VRL documentation][vrl] as a point of reference.
Especially useful when writing Boolean expressions are the [type functions][type], the [debug]

[type]: https://vrl.dev/functions/#type-functions
[vrl]: https://vrl.dev
{{< /success >}}

In the condition above

When writing a VRL condition for your test output, it's important to bear in mind that the condition
passes if the **last expression** provided evaluates to `true`.

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

Optionally, you can specify a a `no_outputs_from` list of transforms that must *not* output events
in order for the test to pass.

### Inputs

In in the `inputs` array for the test, you have these options:

Parameter | Description
:---------|:-----------
`insert_at` | The name of the transform into which the test input is inserted.
`log_fields` | If the transform handles [log events](#logs), these are the key/value pairs that comprise the input event.
`metric` | If the transform handles [metric events](#metrics), these are the fields

### Event types

There are currently three type event types in Vector:

* [`log`](#logs) events
* [`metric`](#metrics) events
* [`raw`](#raw) events


#### Logs

To specify the fields in a log event to be unit tested:

```toml
[transforms.my_transform.log_fields]
```

#### Metrics

To specify the fields in a metric event to be unit tested:

```toml
[transforms.my_transform.metric_fields]
type = "remap"
inputs = []
source = '''

'''
```

Full example:

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
.name == "website_hits"
.kind == "absolute"
.counter.value == 1
is_string(.id)
'''
```

#### Raw events {#raw}

**Raw** events in a unit test are specified as neither logs nor metrics. Providing raw events as
test inputs can be useful in situations where

### Outputs

In the `outputs` array of your unit testing configuration you specify both the expected output
events from the transform(s) you specified in in the [`inputs`](#inputs) as well the point in the
transform chain from which output events are to be extracted.

Here's an example `outputs` declaration:

```toml
[[tests.outputs]]
extract_from = ""
```

Here, `extract_from` means that



## Testing multiple transforms {#multiple}

In the example [above](#example) we tested a single `add_metadata` transform. In many cases, though,
you want to supply input events to a *graph* of multiple transforms and ensure that the output is
what you expect. Here's an example of a graph of transforms:

* A [`remap`][remap] transform uses [VRL] to modify the event
* A [`filter`][filter] transform excludes selected events from the event stream based on supplied
  conditions

```toml
[sources.generate_random]
type = "generator"
format = "syslog"

[transforms.something]
type = "remap"
inputs = ["generate_random"]
source = '''
. = parse_syslog!(.message)
'''

[[tests]]
name = "verify"

[[tests.outputs]]
extract_from = "something"

[[tests.outputs.conditions]]
type = "vrl"
source = '''
exists(.)
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
