---
title: Pipeline Model
description: Vector's pipeline model explains how data is collected and routed within Vector.
---

<SVG src="/optimized_svg/pipeline-model_1350_760.svg" />

Vector's pipeline model is based on a [directed acyclic graph][urls.dag] of
[components][urls.vector_components] that contains independent subgraphs.
[Events][docs.architecture.data-model] must flow in a single direction from sources
to sinks, and cannot create cycles. Each component in the graph can produce zero
or more events.

## Defining pipelines

A Vector pipeline is defined through a TOML, YAML, or JSON
[configuration][urls.vector_configuration] file. For maintainability,
many Vector users use data templating languages like [Jsonnet][urls.jsonnet]
or [Cue][urls.cue].

Configuration is checked during compile-time (Vector boot) to present simple
mistakes and enforce the DAG properties.

## In-flight manipulation

Vector's configured pipeline can be adjusted in real-time without restarting Vector.

### Reload

Vector supports hot [reloading][docs.process-management#reloading] to apply
any configuration changes. This is achieved by sending a `SIGHUP` process
signal to Vector's process.

### API

Vector also includes an [API][docs.reference.api] that allows for real-time
observation and manipulation of a running Vector instance.

[docs.architecture.data-model]: /docs/about/under-the-hood/architecture/data-model/
[docs.process-management#reloading]: /docs/administration/process-management/#reloading
[docs.reference.api]: /docs/reference/api/
[urls.cue]: https://cuelang.org/
[urls.dag]: https://en.wikipedia.org/wiki/Directed_acyclic_graph
[urls.jsonnet]: https://jsonnet.org/
[urls.vector_components]: /components/
[urls.vector_configuration]: /docs/configuration/
