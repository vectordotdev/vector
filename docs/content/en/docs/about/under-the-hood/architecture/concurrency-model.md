---
title: Concurrency model
weight: 2
tags: ["concurrency", "pipeline", "state"]
---

{{< svg "img/concurrency-model.svg" >}}

{{< warning title="Work in progress" >}}
Vector's concurrency model is currently a work in progress. We're expecting to complete the work in **Q2 of 2021**.
{{< /warning >}}

Vector implements a concurrency model that scales naturally with incoming data volume as shown above. Each Vector [source][sources] is responsible for defining the unit of concurrency and implementing it accordingly. This allows for a natural concurrency model that adapts to however Vector is being used, avoiding the need for tedious concurrency tuning and configuration.

For example, the [`file` source][file] implements concurrency across the number of files it's tailing, and the [`socket` source][socket] implements concurrency across the number active open connection it's maintaining.

## Stateless function transforms

As covered in the [pipeline model][pipeline] documentation, Vector's concurrency relies on stateless function transforms that can be inlined at the source level. Task transforms should thus be defined at the end of your topology to allow for maximum transform inlining.

[file]: /docs/reference/configuration/sources/file
[socket]: /docs/reference/configuration/sources/socket
[sources]: /docs/reference/configuration/sources/
[pipeline]: /docs/about/under-the-hood/architecture/pipeline-model
