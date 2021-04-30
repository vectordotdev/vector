---
title: Remap
description: >
  Modify your observability data as it passes through your topology using [VRL](/docs/reference/vrl)
kind: transform
featured: true
---

The `remap` transform is designed for parsing, shaping, and transforming data in Vector. It implements [Vector Remap Language][vrl] (VRL), an expression-oriented language that processes observability data (logs and metrics) safely and at blazing speeds.

{{< success >}}
Refer to the [VRL reference][vrl] when writing scripts.

[vrl]: /docs/reference/vrl
{{< /success >}}

## Configuration

{{< component/config >}}

## Telemetry

{{< component/telemetry >}}

## VRL examples

{{< vrl/real-world-examples >}}

## How it works

### Lazy event mutation

When you make changes to an event using VRL's path assignment syntax, the change isn't immediately applied to the actual event. If the program fails to run to completion, any changes made until that point are dropped and the event is kept in its original state.

If you want to make sure that your event is changed as expected, you need to write your program to never fail at runtile (the compiler can help with this). Alternatively, if you want to ignore/drop events that cause the program to fail, you can set the [`drop_on_error`](#drop_on_error) configuration value to `true`.

You can learn more about runtime errors in the [VRL reference][vrl].

### State

The `remap` transform is stateless, which means that its behavior is consistent across each input.

### Vector Remap Language

Vector Remap Language (VRL) is a restrictive, fast, and safe language that we designed specifically for mapping observability data. It frees you from the need to chain together numerous transforms—`add_fields`, `remove_fields`, and so on—to accomplish rudimentary reshaping of data.

The intent is to offer the same robustness of a full language runtime (such as [Lua]) without paying the performance or safety penalty.

You can learn more about Vector Remap Language in the [VRL reference][vrl].

[lua]: /docs/reference/configuration/transforms/lua
[vrl]: /docs/reference/vrl
