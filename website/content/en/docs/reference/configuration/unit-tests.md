---
title: Unit testing Vector configurations
short: Unit tests
weight: 6
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
vector test /etc/vector/vector.yaml
```

You can also specify multiple configuration files to test:

```bash
vector test /etc/vector/pipeline1.toml /etc/vector/pipeline2.toml
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

```coffee
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
assert!(!exists(.other), "extraneous other field present")
'''
```

You can also store the Boolean expressions in variables rather than passing the entire statement to
the `assert` function:

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
[sources.all_container_services]
type = "docker_logs"
docker_host = "http://localhost:2375"
include_images = ["web_frontend", "web_backend", "auth_service"]

# The transform being tested is a Vector Remap Language (VRL) transform that
# adds two fields to each incoming log event: a timestamp and a unique ID
[transforms.add_metadata]
type = "remap"
inputs = ["all_container_services"]
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
assert!(is_timestamp(.timestamp))
assert!(is_string(.id))
assert_eq!(.message, "successful transaction")
'''
```

This example represents a complete test of the `add_metadata` transform, include test `inputs`
and expected `outputs` drawn from a specific transform.

{{< info >}}
This unit involved only a single Vector transform. An example [multi-transform](#multiple) unit test
is provided below.
{{< /info >}}

### Real vs. test inputs

One important thing to note is that with this example configuration Vector is set up to pull in real
logs from Docker images using the [`docker_logs`][docker_logs] source. If Vector were running in
production, the `add_metadata` transform we're unit testing here would be modifying real log events.
But that's *not* what we're testing here. Instead, the `insert_at = "add_metadata"` directive
artificially inserts our test inputs into the `add_metadata` transform. You should think of Vector
unit tests as a way of **mocking observability data sources** and ensuring that your transforms
respond to those mock sources the way that you would expect.

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

### Inputs

In the `inputs` array for the test, you have these options:

Parameter | Type | Description
:---------|:-----|:-----------
`type` | string | The type of input you're providing. [`vrl`](#logs), [`log`](#logs), [`raw`](#logs), or [`metric`](#metrics) are currently the only valid values.
`insert_at` | string (name of transform) | The name of the transform into which the test input is inserted. This is particularly useful when you want to test only a subset of a transform pipeline.
`value` | string (raw event value) | A raw string value to act as an input event. Use only in cases where events are raw strings and not structured objects with event fields.
`log_fields` | object | If the transform handles [log events](#logs), these are the key/value pairs that comprise the input event.
`metric` | object | If the transform handles [metric events](#metrics), these are the fields that comprise that metric. Subfields include `name`, `tags`, `kind`, and others.
`source` | string (vrl program) | If the transform handles [log events](#logs), the result of the vrl program will be the input event.

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
```

### Outputs

In the `outputs` array of your unit testing configuration, you specify two things:

Parameter | Type | Description
:---------|:-----|:-----------
`extract_from` | string (name of transform) | The transform whose output you want to test.
`conditions` | array of objects | The [VRL conditions](#verifying) to run against the output.

Each condition in the `conditions` array has two fields:

Parameter | Type | Description
:---------|:-----|:-----------
`type` | string | The type of condition you're providing. [`vrl`][vrl] is currently the only valid value.
`source` | string (VRL Boolean expression) | Explained in detail [above](#verifying).

Here's an example `outputs` declaration:

```toml
[[tests.outputs]]
extract_from = "add_metadata"

[[tests.outputs.conditions]]
type = "vrl"
source = '''
assert!(is_string(.id))
assert!(exists(.tags))
'''
```

#### Asserting no output

In some cases, you may need to assert that _no_ event is output by a transform. You can specify
this at the root level of a specific test's configuration using the `no_outputs_from` parameter,
which takes a list of transform names. Here's an example:

```toml
[[tests]]
name = "Ensure no output"
no_outputs_from = ["log_filter", "metric_filter"]
```

In this test configuration, Vector would expect that the `log_filter` and `metric_filter` transforms
not to output _any_ events.

Some examples of use cases for `no_outputs_from`:

* When testing a [`filter`][filter] transform, you may want to assert that the [input](#inputs)
  event is filtered out
* When testing a [`remap`][remap] transform, you may need to assert that VRL's `abort` function is
  called when the supplied [VRL] program handles the input event

Below is a full example of using `no_outputs_from` in a Vector unit test:

```toml
[transforms.log_filter]
type = "filter"
inputs = ["log_source"]
condition = '.env == "production"'

[[tests]]
name = "Filter out non-production events"
no_outputs_from = ["log_filter"]

[[tests.inputs]]
type = "log"
insert_at = "log_filter"

[tests.inputs.log_fields]
message = "success"
code = 202
endpoint = "/transactions"
method = "POST"
env = "staging"
```

This unit test passes because the `env` field of the input event has a value of `staging`, which
fails the `.env == "production"` filtering condition; because the condition fails, no event is
output by the `log_filter` transform in this case.

### Event types

There are currently two event types that you can unit test in Vector:

* [`log`](#logs) events
* [`metric`](#metrics) events

#### Logs

As explained in the section on [inputs](#inputs) above, when testing log events, you can specify
either a structured event [object](#object) or a raw [string](#raw-string-value).

##### Object

To specify a structured log event as your test input, use `log_fields`:

```toml
[tests.inputs.log_fields]
message = "successful transaction"
code = 200
id = "38c5b0d0-5e7e-42aa-ae86-2b642ad2d1b8"
```

If there are hyphens in the field name, you will need to quote this part (at least in YAML):

```yaml
  - name: hyphens
    inputs:
      - insert_at: hyphens
        type: log
        log_fields:
          labels."this-has-hyphens": "this is a test"
```

##### Raw string value

To specify a raw string value for a log event, use `value`:

```toml
[[tests.inputs]]
insert_at = "add_metadata"
value = "<102>1 2020-12-22T15:22:31.111Z vector-user.biz su 2666 ID389 - Something went wrong"
```

##### VRL program

To specify a program to construct the log event, use `source`:

```toml
[[tests.inputs]]
  insert_at = "canary"
  type = "vrl"
  source = """
    . = {"a": {"b": "c"}, "d": now()}
  """
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

Aggregated metrics are a little different:

```yaml
tests:
  inputs:
    insert_at: my_aggregate_metrics_transform
    type: metric
    metric:
      name: http_rtt
      kind: incremental
      aggregated_histogram:
        buckets: []
        sum: 0
        count: 0
```

Here's a full end-to-end example of unit testing a metric through a transform:

```toml
[transforms.add_env_to_metric]
type = "remap"
inputs = []
source = '''
env, err = get_env_var("ENV")
if err != null {
  log(err, level: "error")
}
tags.environment = env
'''

[[tests]]
name = "add_unique_id_test"

[[tests.inputs]]
insert_at = "add_env_to_metric"
type = "metric"

[tests.inputs.metric]
name = "website_hits"
kind = "absolute"
counter = { value = 1 }

[[tests.outputs]]
extract_from = "add_env_to_metric"

[[tests.outputs.conditions]]
type = "vrl"
source = '''
assert_eq!(.name, "website_hits")
assert_eq!(.kind, "absolute")
assert_eq!(.tags.environment, "production")
'''
```

## Multiple transforms {#multiple}

The examples provided thus far in this doc have involved unit testing a single transform. It's also
possible, however, to test the output of multiple transforms chained together. Imagine a scenario
in which you have a transform called `add_env_metadata` that tags the event with environment
metadata, a transform called `sanitize` that removes some undesired fields, and finally a transform
called `add_host_metadata` that tags the event with a hostname. Below is an example unit test
configuration for this set of transform, with explanatory annotations:

{{< warning >}}
You may notice that the three transforms in this example could be combined into a single `remap`
transform. Their separation into multiple transforms here is purely for demonstration purposes.
{{< /warning >}}

```toml
# This source, like all sources, is ignored in the unit test itself
[sources.web_backend]
type = "docker_logs"
docker_host = "http://localhost:2375"
include_images = ["web_backend"]

# The first transform in the chain
[transforms.add_env_metadata]
type = "remap"
inputs = ["web_backend"]
source = '''
.tags.environment = "production"
'''

# The second transform in the chain
[transforms.sanitize]
type = "remap"
inputs = ["add_env_metadata"]
source = '''
del(.username)
del(.email)
'''

# The final transform in the chain
[transforms.add_host_metadata]
type = "remap"
inputs = ["sanitize"]
source = '''
.tags.host = "web-backend1.vector-user.biz"
'''

[[tests]]
name = "Multiple chained remap transforms"

[[tests.inputs]]
type = "log"
# Insert test input events into the first transform
insert_at = "add_env_metadata"

# The input event to insert into the first transform in the chain
[tests.inputs.log_fields]
message = "image successfully uploaded"
code = 202
username = "tonydanza1337"
email = "tony@whostheboss.com"
transaction_id = "bcef6a6a-2b72-4a9a-99a0-97ae89d82815"

[[tests.outputs]]
# Extract test outputs from the last transform
extract_from = "add_host_metadata"

[[tests.outputs.conditions]]
type = "vrl"
# Our VRL assertions for the test output
source = '''
assert_eq!(.tags.environment, "production", "incorrect environment tag")
assert_eq!(.tags.host, "web-backend1.vector-user.biz", "incorrect host tag")
assert!(!exists(.username))
assert!(!exists(.email))

valid_transaction_id = exists(.transaction_id) &&
  is_string(.transaction_id) &&
  length!(.transaction_id) == 36

assert!(valid_transaction_id, "transaction ID invalid")
'''
```

From a testing standpoint, all three transforms here can be thought of as a single unit. One example
event is inserted at the beginning of the chain (`add_env_metadata`), one output test event is
extracted from the end of the chain (`add_host_metadata`), and one set of VRL
[assertions](#verifying) verifies that that output event conforms to our expectations.

You could also test a subset of this transform chain. This configuration, for example, would test
only the first two transforms (`add_env_metadata` and `sanitize`):

```toml
[[tests]]
name = "First two transforms"

[[tests.inputs]]
type = "log"
# Insert test input into the first transform
insert_at = "add_env_metadata"

# For comparison, we can use the same input event as above
[tests.inputs.log_fields]
message = "image successfully uploaded"
code = 202
username = "tonydanza1337"
email = "tony@whostheboss.com"
transaction_id = "bcef6a6a-2b72-4a9a-99a0-97ae89d82815"

[[tests.outputs]]
# Extract test output from the second transform rather than the last
extract_from = "sanitize"

[[tests.outputs.conditions]]
type = "vrl"
source = '''
assert_eq!(.tags.environment, "production", "incorrect environment tag")
assert!(!exists(.tags.host), "host tag included")
assert!(!exists(.username))
assert!(!exists(.email))

valid_transaction_id = exists(.transaction_id) &&
  is_string(.transaction_id) &&
  length!(.transaction_id) == 36

assert!(valid_transaction_id, "transaction ID invalid")
'''
```

In the VRL conditions for this two-transform test, notice that the assertion regarding the `host`
tag is changed to this, which verifies that that tag isn't present, which is what we should expect
given that the `add_host_metadata` transform isn't included here:

```text
assert!(!exists(.tags.host), "host tag included")
```

[abort]: /docs/reference/vrl/functions/#abort
[assert]: /docs/reference/vrl/functions/#assert
[assert_eq]: /docs/reference/vrl/functions/#assert_eq
[assertions]: /docs/reference/vrl#assertions
[boolean]: /docs/reference/vrl/#boolean-expressions
[comparisons]: /docs/reference/vrl/expressions/#comparison
[contains]: /docs/reference/vrl/functions/#contains
[datadog_search]: https://docs.datadoghq.com/logs/explorer/search_syntax
[docker_logs]: /docs/reference/configuration/sources/docker_logs
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
