---
title: Highlights - February 2025
short: Highlights - February 2025
description: New features and guides are now available!
authors: [ "pront" ]
date: "2025-02-12"
badges:
  type: announcement
  domains: [ "dev" ]
tags: [ "features", "dev", "debugging", "guides", "guide" ]
---

_In this blog post we want to highlight some recently added features and guides._

## Features

* A new OpenTelemetry sink is now available. You can find a
  [quickstart guide here]({{< ref "/docs/reference/configuration/sinks/opentelemetry/#quickstart" >}}).
  * We have more plans here such as gRPC support and smart grouping at the sink.
* We introduced a new exclusive route transform. You can read more in our
  [release highlight]({{< ref "/highlights/2024-11-07-exclusive_route" >}}).
* Apple Silicon builds are now available in our [downloads page]({{< ref "/download" >}}).
  Note that these are available since `v0.44.0` and won't show up for older versions.
* A new type of `enrichment_table`, called `memory`, was introduced! This table can also function as a sink and addresses several new use cases. See this
  [release highlight]({{< ref "/highlights/2025-02-24-memory_enrichment_table.md" >}}) for details.
* The VRL function library keeps growing thanks to community contributions! You can read more:
  * [0.21.0 Features]({{< ref "/releases/0.44.0/#new-features" >}})
  * [0.20.0 Features]({{< ref "/releases/0.43.0/#new-features" >}})

### Guides

We recently added a new guide category: [Development]({{< ref "guides/developer/_index.md" >}}).

### Debugging Guide

You can find the new [debugging guide here]({{< ref "guides/developer/debugging/" >}}). The goal of this guide is to serve as a
starting point for Vector users who are experiencing issues with their Vector configurations and deployments. In addition, it highlights
debugging tools that Vector offers.

### IDE Support

You can find the new [config autocompletion guide here]({{< ref "guides/developer/config-autocompletion/" >}}).
We supported generating a Vector config schema for a while. This guide demonstrates how we can leverage this schema and import it into
popular IDEs in order to help with writing configs.

We would love to this to VRL IDE support in the future. In the meantime, we would like to highlight
this   [tree-sitter](https://github.com/tree-sitter/tree-sitter) plugin that was developed by https://github.com/belltoy. You can read more
[here](https://github.com/vectordotdev/vrl/issues/964).
