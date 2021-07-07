---
title: Unit testing Vector configurations
short: Unit tests
weight: 5
aliases: ["/docs/reference/tests"]
---

Vector enables you to [unit test] [transforms] in your processing topology. The goal of unit tests in
general is to improve the maintainability of configurations containing larger and more complex
combinations of transforms.

Unit tests in Vector work just like unit tests for standard software libraries:

1. Provide a set of **inputs** to a transform (or a graph of transforms)
1. Provide expected **outputs** from the changes made by the transform (or a graph of transforms)
1. Receive directly actionable feedback on test failures

If *any* of your unit tests fail, the Vector instance doesn't start up. This makes unit tests a
useful guardrail for running in Vector in production settings where you need to ensure that your
topology doesn't exhibit unexpected behavior.

## Running unit tests

You can execute tests within a configuration file using the `test` subcommand:

```bash
vector test /etc/vector/vector.toml
```

You can also specify multiple files:

```bash
vector test /etc/vector/*.toml
```

{{< success >}}
Specifying multiple files would be useful here if you wanted to keep your unit tests in a separate
file from your topology configuration.
{{< /success >}}

## Example unit test configuration {#example}

Let's start with an annotated example:

```toml
# This is the transform being tested. It's a VRL transform that adds two fields
# to each incoming log event: a timestamp and a unique ID
[transforms.add_metadata]
type = "remap"
inputs = []
source = '''
.timestamp = now()
.id = uuid_v4()
'''

# Here we begin declaring our test suite
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

## Configuring unit tests

{{< warning >}}
Unit tests are a bit tricky to configure. This section provides an exhaustive listing of available
configuration parameters, but if you have trouble seeing how things fit together in this section,
skip down to the [Example configuration](#examples) section for an end-to-end example that may help
illuminate the broader picture. Vector's own [internal unit tests][vector_tests] are also a good
resource.
{{< /warning >}}

Unit tests in Vector live alongside your topology configuration. You can specify your tests in the
same config file or split them out into a separate file if you wish. The table below lists important
parameters:

Parameter | Description
:---------|:-----------
`inputs` | A table that defines [input events](#inputs) for the unit test.
`outputs` | A table that defines [expected outputs](#outputs) for the unit test.
`no_outputs_from` | A list of transforms that must *not* output events in order for the test to pass

### Inputs

Parameter | Description
:---------|:-----------
`insert_at` | The name of the transform into which the test input is inserted.
`log_fields` | If the transform handles [log events](#logs), these are the key/value pairs that comprise the input event.
`metric` | If the transform handles [metric events](#metrics), these are the fields

#### Example inputs declaration

```toml

```

### Event types

There are currently type event types in Vector:

* [`log`][logs]
* [`metric`][metrics]

#### Logs

To specify the fields in a log event to be unit tested:

```toml
[transforms.my_transform.log_fields]
```

#### Metrics

To specify the fields in a metric event to be unit tested:

```toml
[transforms.my_transform.metric_fields]

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

#### Raw events



### Outputs

## Testing multiple transforms

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

## Other examples

Vector's internal testing for its provided transforms includes a wide range of unit tests. We
recommend the [Vector repo][vector_tests] as a potential source of inspiration when writing your own
Vector unit tests.

[filter]: TODO
[logs]: TODO
[metrics]: TODO
[remap]: /docs/reference/configuration/transforms/remap
[transforms]: /docs/reference/glossary/#transform
[unit test]: https://en.wikipedia.org/wiki/Unit_testing
[vector_tests]: https://github.com/timberio/vector/tree/master/tests/behavior/transforms
[vrl]: https://vrl.dev
