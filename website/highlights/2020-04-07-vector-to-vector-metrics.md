---
last_modified_on: "2020-04-14"
$schema: "/.meta/.schemas/highlights.json"
title: "The Vector Source Now Accepts Metrics"
description: "It's not possible to forward metrics between Vector instances"
author_github: "https://github.com/binarylogic"
pr_numbers: [2245]
release: "0.9.0"
hide_on_release_notes: true
tags: ["type: new feature", "domain: sources", "source: vector"]
---

import SVG from 'react-inlinesvg';

Until recently the [`vector` source][docs.sources.vector] only accepted
[`log` events][docs.data-model.log]. Supporting metrics was blocked by pending
metric data model development, as well as topology improvements.
[PR#2245][urls.pr_2245] removes that limitation enabling you to truly build
observability pipelines that can process both logs and metrics, such as
the [centralized topology][docs.topologies#centralized].


[docs.data-model.log]: /docs/about/data-model/log/
[docs.sources.vector]: /docs/reference/sources/vector/
[docs.topologies#centralized]: /docs/setup/deployment/topologies/#centralized
[urls.pr_2245]: https://github.com/timberio/vector/pull/2245
