---
title: Pipeline model
weight: 1
tags: ["pipeline", "dag", "graph", "configuration"]
---

{{< svg "img/pipeline-model.svg" >}}

Vector's pipeline model is based on a [directed acyclic graph][dag] of [components] that contains independent subgraphs. [Events] must flow in a single direction from sources to sinks and can't create cycles. Each component in the graph can produce zero or more events.

## Defining pipelines

A Vector pipeline is defined through a YAML, TOML, or JSON [configuration] file. For maintainability, many Vector users use configuration and data templating languages like [Jsonnet] or [CUE].

Configuration is checked at pipeline compile time (when Vector boots). This prevents simple mistakes and enforces DAG properties.

## In-flight manipulation

Vector's configured pipeline can be adjusted in real time without restarting Vector.

### Reload

Vector supports [hot reloading][reloading] to apply any configuration changes. This is achieved by sending a `SIGHUP` process signal to Vector's process.

### API

Vector also includes an [API] that allows for real-time observation and manipulation of a running Vector instance.

[api]: /docs/reference/api
[components]: /components
[configuration]: /docs/reference/configuration
[cue]: https://cuelang.org
[dag]: https://en.wikipedia.org/wiki/Directed_acyclic_graph
[events]: /docs/about/under-the-hood/architecture/data-model
[jsonnet]: https://jsonnet.org
[reloading]: /docs/administration/management/#reloading
