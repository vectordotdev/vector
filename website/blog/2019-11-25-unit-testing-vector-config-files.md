---
id: unit-testing-vector-config-files
title: "Unit Testing: Treating Your Vector Config Files As Code"
author: Ashley
author_title: Vector Core Team
author_url: https://github.com/jeffail
author_image_url: https://github.com/jeffail.png
tags: ["type: announcement", "domain: config"]
---

Today we're excited to announce support for unit testing your configuration
files! This feature allows you to inline tests directly within your Vector
configuration file. These tests are used to assert output from individual
[transform][docs.transforms] components, ensuring that your configuration
behavior does not regress; a very powerful feature for mission-critical
production pipelines that are collaborated on.

## Example

Let's look at a basic example that uses the [`regex_parser` 
transform][docs.transforms.regex_parser] to parse log lines:

<CodeHeader fileName="vector.toml" />

```toml
[sources.my_logs]
  type    = "file"
  include = ["/var/log/my-app.log"]

[transforms.parser]
  inputs = ["my_logs"]
  type   = "regex_parser"
  regex  = "^(?P<timestamp>\\w*) (?P<level>\\w*) (?P<message>.*)$"

[[tests]]
  name = "verify_regex"

  [tests.input]
    insert_at = "my_logs"
    type = "raw"
    value = "2019-11-28T12:00:00+00:00 info Hello world"

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

On the surface, Vector configuration files seem like simple instructions to
connect disparate systems. For example, [tailing a file][docs.sources.file] and
sending that data to [AWS CloudWatch Logs][docs.sinks.aws_cloudwatch_logs]. But
as you start to become familiar with Vector, you'll start adding
[transforms][docs.transforms] and important logic to your configuration. All of
sudden, your Vector configuration files start to look and feel like code. Unit
tests allow you to assert various conditions, ensuring your "code" does not
regress.

## Getting Started

To help you get started we put together 2 documentation files:

1. [A unit testing guide][docs.guides.unit_testing]
2. [Unit testing reference][docs.reference.tests]

These should be everything you need and will be actively maintained as this
feature matures.

## Feedback!

We're eager to hear your feedback! Please note, this feature is in `beta` and
represents the initial MVP version. We'd like to collect more feedback before
we


[docs.guides.unit_testing]: /docs/setup/guides/unit-testing
[docs.reference.tests]: /docs/reference/tests
[docs.sinks.aws_cloudwatch_logs]: /docs/reference/sinks/aws_cloudwatch_logs
[docs.sources.file]: /docs/reference/sources/file
[docs.transforms]: /docs/reference/transforms
