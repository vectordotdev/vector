---
date: "2021-10-08"
title: "0.17 Upgrade Guide"
description: "An upgrade guide that addresses breaking changes in 0.17.0"
authors: ["jszwedko", "tobz"]
pr_numbers: []
release: "0.17.0"
hide_on_release_notes: false
badges:
  type: breaking change
---

Vector's 0.17.0 release includes several **breaking changes**:

1. [Blackhole sink configuration changes](#blackhole)
1. [Datadog Logs sink loses `batch.max_bytes` setting](#datadog_logs_max_bytes)
1. [Vector now logs to stderr](#logging)
1. [The `generator` source now has a default `interval` setting](#interval)
1. [The deprecated `wasm` transform was removed](#wasm)
1. [The `exec` source now has a `decoding` setting](#exec_source)
1. [The algorithm underlying ARC has been optimized](#arc)

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

[generator]: /docs/reference/configuration/sources/demo_logs

### The deprecated `wasm` transform was removed {#wasm}

The `wasm` transform was [deprecated in v0.16.0][deprecation] and has been removed in this release.

In its place, we recommend using the `remap` and `lua` transforms.

Note, we may revisit adding WASM support to Vector for custom plugins in the future. If you have a use-case, please add
it to the [GitHub issue][9466].

[deprecation]: /highlights/2021-08-23-removing-wasm
[9466]: https://github.com/vectordotdev/vector/issues/9466

### The `exec` source now has a `decoding` setting {#exec_source}

Previously, the [`exec` source][exec] had an `event_per_line` setting
that controlled how events were parsed out of the input data from the
executed program. This has been removed and replaced by separate
[`framing`][exec_framing] and [`decoding`][exec_decoding] options that
provide more control over the formats that this source accepts.

[exec]: /docs/reference/configuration/sources/exec
[exec_decoding]: /docs/reference/configuration/sources/exec/#decoding
[exec_framing]: /docs/reference/configuration/sources/exec/#framing

### The algorithm underlying ARC has been optimized {#arc}

The algorithm underlying the [adaptive request concurrency][arc]
mechanism has been optimized in this release to take into account the
variance between request response times. This has come with changes to
the configuration as well. The option to control the RTT threshold
value, `rtt_threshold_ratio`, has been replaced by the RTT variance
calculation. It has been replaced by
[`rtt_deviation_scale`][rtt_deviation_scale] which can be used to adjust
the scale factor applied to this value.

[arc]: /docs/about/under-the-hood/networking/arc/
[rtt_deviation_scale]: /docs/reference/configuration/sinks/http/#request.adaptive_concurrency.rtt_deviation_scale
