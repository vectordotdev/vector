---
date: "2021-10-05"
title: "0.17 Upgrade Guide"
description: "An upgrade guide that addresses breaking changes in 0.17.0"
authors: ["tobz"]
pr_numbers: []
release: "0.17.0"
hide_on_release_notes: false
badges:
  type: breaking change
---

Vector's 0.17.0 release includes two **breaking changes**:

1. [Blackhole sink configuration changes](#blackhole)
1. [Datadog Logs sink loses `batch.max_bytes` setting](#datadog_logs_max_bytes)

We cover it below to help you upgrade quickly:

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
