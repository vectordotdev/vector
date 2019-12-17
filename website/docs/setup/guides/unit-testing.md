---
title: Unit Testing Guide
sidebar_label: Unit Testing
description: Learn how to write and execute unit tests for your configs
status: beta
---

It's possible to define unit tests within a Vector configuration file that cover
a network of transforms within the topology. The purpose of these tests is to
improve the maintainability of configs containing larger and more complex
combinations of transforms.

The full spec can be found [here][docs.reference.tests]. This guide will cover
writing and executing a unit test for the following config:

import CodeHeader from '@site/src/components/CodeHeader';

<CodeHeader fileName="example.toml" />

```toml
[sources.over_tcp]
  type = "tcp"
  address = "0.0.0.0:9000"

[transforms.foo]
  type = "grok_parser"
  inputs = ["over_tcp"]
  pattern = "%{TIMESTAMP_ISO8601:timestamp} %{LOGLEVEL:level} %{GREEDYDATA:message}"

[transforms.bar]
  type = "add_fields"
  inputs = ["foo"]
  [transforms.bar.fields]
    new_field = "this is a static value"

[transforms.baz]
  type = "remove_fields"
  inputs = ["foo"]
  fields = ["level"]

[sinks.over_http]
  type = "http"
  inputs = ["baz"]
  uri = "http://localhost:4195/post"
  encoding = "text"
```

In this config we:

- Parse a log line into the fields `timestamp`, `level` and `message` with the
  transform `foo`.
- Add a static string field `new_field` using the transform `bar`.
- Remove the field `level` with the transform `baz`.

In reality it's unlikely that a config this simple would be worth the investment
of writing unit tests. Regardless, for the purpose of this guide we've concluded
that yes, we do wish to unit test this config.

Specifically, we need to ensure that the resulting events of our topology
(whatever comes out of the `baz` transform) always meets the following
requirements:

- Does NOT contain the field `level`.
- Contains the field `new_field`, with a static value `this is a static value`.
- Has a `timestamp` and `message` field containing the values extracted from the
  raw message of the input log.

Otherwise our system fails and an annoying relative (Uncle Cecil) moves in to
live with us indefinitely. We will do _anything_ to prevent that.

## Input

First we shall write a single unit test at the bottom of our config called
`check_simple_log`. Each test must define a single input event, which initiates
the test by injecting that event into a transform of the topology:

```toml
[[tests]]
  name = "check_simple_log"

  [tests.input]
    insert_at = "foo"
    type = "raw"
    value = "2019-11-28T12:00:00+00:00 info Sorry, I'm busy this week Cecil"
```

Here we've specified that our test should begin by injecting an event at the
transform `foo`. The `raw` input type creates a log with only a `message` field
and `timestamp` (set to the time of the test), where `message` is populated with
the contents of the `value` field.

## Conditions

This test won't work in its current state because there's nothing to check. In
order to perform checks with this unit test we define an expected output:

```toml
[[tests]]
  name = "check_simple_log"

  [tests.input]
    insert_at = "foo"
    type = "raw"
    value = "2019-11-28T12:00:00+00:00 info Sorry, I'm busy this week Cecil"

  [[tests.outputs]]
    extract_from = "baz"

    [[tests.outputs.conditions]]
      type = "check_fields"
      "level.exists" = false
      "new_field.equals" = "this is a static value"
      "timestamp.equals" = "2019-11-28T12:00:00+00:00"
      "message.equals" = "Sorry, I'm busy this week Cecil"
```

You can define any number of expected outputs, where we must specify at which
transform the output events should be extracted for checking. This allows you to
check the events from different transforms in a single test. For our purposes we
only need to check the output of `baz`.

An output can also have any number of conditions to check. In order for the test
to pass each condition for an output must resolve to `true`. It's possible for a
topology to result in >1 events extracted from a single transform, in which case
a condition must pass for one or more of the extracted events in order for the
test to pass.

The only condition we've defined here is a `check_fields` type. This is
currently the _only_ condition type on offer, and it allows us to specify any
number of field queries (of the format `"<field>.<predicate>" = "<argument>"`).

## Executing

With this test appended to the bottom of our config we are now able to execute
it. Executing tests within a config file can be done with the `test` subcommand:

```bash
vector test ./example.toml
```

Doing this results in the following output:

```sh
$ vector test ./example.toml 
Running ./example.toml tests
Test ./example.toml: check_simple_log ... failed

failures:

--- ./example.toml ---

Test 'check_simple_log':
check transform 'baz' failed conditions: [ 0 ], payloads (encoded in JSON format):
  {"timestamp":"2019-11-28T12:00:00+00:00","message":"Sorry, I'm busy this week Cecil"}
```

Woops! Something isn't right. Unfortunately we're only told which
output-condition failed, not which predicate of our `check_fields` condition
specifically caused the failure. If we refactor our test slightly we can make it
clearer by breaking our condition down to one per predicate:


```toml
[[tests]]
  name = "check_simple_log"

  [tests.input]
    insert_at = "foo"
    type = "raw"
    value = "2019-11-28T12:00:00+00:00 info Sorry, I'm busy this week Cecil"

  [[tests.outputs]]
    extract_from = "baz"

    [[tests.outputs.conditions]]
      type = "check_fields"
      "level.exists" = false

    [[tests.outputs.conditions]]
      type = "check_fields"
      "new_field.equals" = "this is a static value"

    [[tests.outputs.conditions]]
      type = "check_fields"
      "timestamp.equals" = "2019-11-28T12:00:00+00:00"

    [[tests.outputs.conditions]]
      type = "check_fields"
      "message.equals" = "Sorry, I'm busy this week Cecil"
```

Running the test again gives us this:

```sh
$ vector test ./example.toml 
Running ./example.toml tests
Test ./example.toml: check_simple_log ... failed

failures:

--- ./example.toml ---

Test 'check_simple_log':
check transform 'baz' failed conditions: [ 1 ], payloads (encoded in JSON format):
  {"timestamp":"2019-11-28T12:00:00+00:00","message":"Sorry, I'm busy this week Cecil"}
```

This time the output states that it's condition `1` that failed, which is the
condition checking for the field `new_field`. Try reviewing our config topology
to see if you can spot the mistake.

SPOILERS: The problem is that transform `baz` is configured with the input
`foo`, which means `bar` is skipped in the topology!

Side note: We would have also caught this particular issue with
`vector validate --topology ./example.toml`.

The fix is easy, we simply change the input of `baz` from `foo` to `bar`:

```diff
--- a/example.toml
+++ b/example.toml
@@ -15,7 +15,7 @@
 
 [transforms.baz]
   type = "remove_fields"
-  inputs = ["foo"]
+  inputs = ["bar"]
   fields = ["level"]
```

And running our test again gives us an exit status 0:

```sh
$ vector test ./example.toml 
Running ./example.toml tests
Test ./example.toml: check_simple_log ... passed
```

The test passed! Now if we configure our CI system to execute our test we can
ensure that Uncle Cecil remains in Shoreditch after any future config change.
What an insufferable hipster he is.


[docs.reference.tests]: /docs/reference/tests/
