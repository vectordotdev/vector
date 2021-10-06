---
date: "2021-10-05"
title: "0.17 Upgrade Guide"
description: "An upgrade guide that addresses breaking changes in 0.17.0"
authors: ["jszwedko", "tobz"]
pr_numbers: []
release: "0.17.0"
hide_on_release_notes: false
badges:
  type: breaking change
---

Vector's 0.17.0 release includes five **breaking changes**:

1. [Blackhole sink configuration changes](#blackhole)
1. [Datadog Logs sink loses `batch.max_bytes` setting](#datadog_logs_max_bytes)
1. [Vector now logs to stderr](#logging)
1. [The `generator` source now has a default `interval` setting](#interval)
1. [The deprecated `wasm` transform was removed](#wasm)

And one deprecation:

1. [The `aws_s3` `multiline` option has been deprecated](#multiline)

We cover them below to help you upgrade quickly:

## Upgrade guide

### Blackhole sink configuration changes {#blackhole}

We've updated the blackhole sink to print its statistics summary on an interval, rather than after a
specific number of events.  This provides a consistent reporting experience regardless of the number
of events coming into the sink, including when _no_ events are coming in.

The configuration field `print_amount` has been removed, and replaced with `print_interval_secs`.
Additionally, `print_interval_secs` defaults to `1 second`, which has the additional benefit of
providing a very basic "events per second" indicator out-of-the-box.

### Datadog Logs sink loses `batch.max_bytes` setting {#datadog_logs_max_bytes}

We've updated the Datadog Logs sink to conform more tightly to the Datadog Logs
API's constraints, one of which is a maximum payload size. The recommendation of
that API is to send payloads as close to but not over 5MB in an uncompressed,
serialized form. The sink will now always try to send 5MB payloads, consistent
with your timeout settings.

Users that have previously set `batch.max_bytes` may now safely remove the
value. If it is left the setting will have no effect.

### Vector now logs to stderr {#logging}

Previously, Vector used to log all output to stdout, but this made it difficult to use the output of the `console` sink,
which also writes to stdout by default.  Following some discussion in
[#1714](https://github.com/vectordotdev/vector/issues/1740) we decided to modify Vector to, instead, log to stderr so
that stdout can be processed separately.

If you were previously depending on Vector's logs appearing in stdout, you should now look for them in stderr.

### The `generator` source now has a default `interval` setting {#interval}

Previously, the [`generator`][generator] source had no default `interval`, which meant that if you
started Vector without setting an `interval`, the `generator` would output batches of test events as
fast as it can. In version 0.17.0, the default for `interval` is now `1.0`, which means that Vector
outputs one batch per second. To specify no delay between batches you now need to explicit set
`interval` to `0.0`.

[generator]: /docs/reference/configuration/sources/generator

### The deprecated `wasm` transform was removed {#wasm}

The `wasm` transform was [deprecated in v0.16.0](deprecation) and has been removed in this release.

In its place, we recommend using the `remap` and `lua` transforms.

Note, we may revisit adding WASM support to Vector for custom plugins in the future. If you have a use-case, please add
it to the [Github issue](9466).

[deprecation]: /content/en/highlights/2021-08-23-removing-wasm
[9466]: https://github.com/vectordotdev/vector/issues/9466

### The `aws_s3` `multiline` option has been deprecated {#multiline}

As part of some on-going work to support automatic decoding of data in sources, we have deprecated the `multiline`
option on the `aws_s3` source in-lieu of the `reduce` transform which can be used to multiple events into one.

For example:

```toml
[sources.my_aws_s3_source.multiline]
start_pattern = '\\$'
mode = "continue_past"
condition_pattern = '\\$'
timeout_ms = 1000
```

Would translate to:

```toml
[transforms.s3_multiline_merging]
type = "reduce"
inputs = [ "my_aws_s3_source" ]
ends_when = '!match(string!(.message), "\\$")'
expire_after_ms = 1_000
group_by = [ "bucket", "object" ]
merge_strategies.message = "concat_newline"
```
