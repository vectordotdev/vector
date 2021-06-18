---
title: Concurrency Model
description: Vector's concurrency model enables automatic CPU concurrency.
---

<SVG src="/optimized_svg/concurrency-model_842_494.svg" />

<Alert severity="warning">

Vector's concurrency model is currently a work in progress. We are expecting
to complete the work in Q2 of 2021.

</Alert>

Vector implements a concurrency model that scales naturally with incoming data
volume as shown above. Each Vector [source][docs.sources] is responsible for
defining the unit of concurrency and implementing it accordingly. This allows
for a natural concurrency model that adapts to however Vector is being used,
avoiding the need for tedious concurrency tuning and configuration.

For example, the [`file` source][docs.sources.file] implements concurrency
across the number of files it's tailing, and the
[`socket` source][docs.sources.socket] implements concurrency across the number
active open connection it's maintaining.

## Stateless function transforms

As covered in the [topology model document](topology-model), Vector's
concurrency relies on stateless function transforms that can be inlined at the
source level. Therefore, task transforms should be defined at the end of
your topology to allow for maximum transform inlining.

[docs.sources.file]: /docs/reference/configuration/sources/file/
[docs.sources.socket]: /docs/reference/configuration/sources/socket/
[docs.sources]: /docs/reference/configuration/sources/
