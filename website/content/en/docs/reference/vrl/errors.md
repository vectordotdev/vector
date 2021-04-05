---
title: VRL error reference
short: Errors
weight: 3
---

VRL is a [fail-safe](#fail-safety), which means that a VRL program doesn't compile unless every possible error is handled. This largely contributes to VRL's safety principle. Observability data is notoriously unpredictable and fail-safety ensures that your VRL programs elegantly handle malformed data, a problem that often plagues observability pipelines.

## Compile-time errors

{{< vrl/errors/compile-time >}}

## Runtime errors

{{% vrl/errors/runtime %}}
