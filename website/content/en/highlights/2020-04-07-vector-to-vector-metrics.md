---
date: "2020-04-14"
title: "The Vector Source Now Accepts Metrics"
description: "It's not possible to forward metrics between Vector instances"
authors: ["binarylogic"]
pr_numbers: [2245]
release: "0.9.0"
hide_on_release_notes: true
badges:
  type: "new feature"
  domains: ["sources"]
  sources: ["vector"]
---

Until recently the [`vector` source][docs.sources.vector] only accepted
[`log` events][docs.data-model.log]. Supporting metrics was blocked by pending
metric data model development, as well as topology improvements.
[PR#2245][urls.pr_2245] removes that limitation enabling you to truly build
observability pipelines that can process both logs and metrics, such as
the [centralized topology][docs.topologies#centralized].

[docs.data-model.log]: /docs/about/under-the-hood/architecture/data-model/log
[docs.sources.vector]: /docs/reference/configuration/sources/vector/
[docs.topologies#centralized]: /docs/setup/deployment/topologies/#centralized
[urls.pr_2245]: https://github.com/vectordotdev/vector/pull/2245
