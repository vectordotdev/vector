---
date: "2020-03-31"
title: "Unit Testing Your Vector Config Files"
description: "Treating your Vector configuration files as code"
authors: ["binarylogic"]
pr_numbers: [1220]
release: "0.6.0"
hide_on_release_notes: false
badges:
  type: "new feature"
  domains: ["config"]
aliases: ["/blog/unit-testing-vector-config-files"]
---

Today we're excited to announce beta support for unit testing Vector
configurations, allowing you to define tests directly within your Vector
configuration file. These tests are used to assert the output from topologies of
[transform][docs.transforms] components given certain input events, ensuring
that your configuration behavior does not regress; a very powerful feature for
mission-critical production pipelines that are collaborated on.

<!--more-->

## Example

Let's look at a basic example that uses the [`regex_parser`
transform][docs.transforms.regex_parser] to parse log lines:

```toml title="vector.toml"
[sources.my_logs]
  type    = "file"
  include = ["/var/log/my-app.log"]

[transforms.parser]
  inputs = ["my_logs"]
  type   = "regex_parser"
  regex  = "^(?P<timestamp>[\\w\\-:\\+]+) (?P<level>\\w+) (?P<message>.*)$"

[[tests]]
  name = "verify_regex"

  [tests.input]
    insert_at = "parser"
    type = "raw"
    value = "2019-11-28T12:00:00+00:00 info Hello world"

  [[tests.outputs]]
    extract_from = "parser"

    [[tests.outputs.conditions]]
      type = "check_fields"
      "timestamp.equals" = "2019-11-28T12:00:00+00:00"
      "level.equals" = "info"
      "message.equals" = "Hello world"
```

And you can run the tests via the new `test` subcommand:

```sh
$ vector test ./vector.toml
Running ./vector.toml tests
Test ./vector.toml: verify_regex ... passed
```

## Why?

Many Vector configurations will be simple transformations across straight
forward pipelines (such as [tailing a file][docs.sources.file] and piping the
data to [AWS CloudWatch Logs][docs.sinks.aws_cloudwatch_logs]) and don't really
need protection from regressions. However, Vector configs are capable of
expanding indefinitely with [transforms][docs.transforms] in order to solve as
much of your processing needs as possible.

As a configuration grows, and as the number of owners of a configuration grow, the potential
for regressions also grows just like a regular code base. The lack of testing
capabilities of configuration driven services is therefore a common pain for
larger organizations. We hope that natively supporting unit tests in Vector
configs will preemptively solve this problem.

## Getting Started

To help you get started we put together two documentation pages:

1. [A unit testing guide][guides.advanced.unit_testing]
2. [A reference to the unit testing configuration spec][docs.reference.tests]

These should be everything you need and will be actively maintained as this
feature matures.

## Feedback

We're eager to hear your feedback! Unit testing, as a `beta` feature, is still
in an early phase and we need case studies and comments in order to ensure it
works well for everyone. Please let us know what you think either in our
[community chat](https://chat.vector.dev/) or by
[raising an issue](https://github.com/vectordotdev/vector/issues/new).

[docs.reference.tests]: /docs/reference/configuration/tests
[docs.sinks.aws_cloudwatch_logs]: /docs/reference/configuration/sinks/aws_cloudwatch_logs
[docs.sources.file]: /docs/reference/configuration/sources/file
[docs.transforms.regex_parser]: /docs/reference/vrl/functions/#parse_regex
[docs.transforms]: /docs/reference/configuration/transforms
[guides.advanced.unit_testing]: /guides/level-up/unit-testing
