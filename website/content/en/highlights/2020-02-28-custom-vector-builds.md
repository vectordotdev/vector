---
date: "2020-04-16"
title: "À la carte Custom Vector Builds"
description: "Build Vector with select components"
authors: ["binarylogic"]
pr_numbers: [1924]
release: "0.9.0"
hide_on_release_notes: false
badges:
  type: "new feature"
  domains: ["sources"]
  sources: ["vector"]
---

We've implemented a feature flag system that lets you build custom versions
of Vector with selected [components][pages.components]. This is handy if:

1. You're embedding Vector and you want to minimize the binary size as much as
   possible.
2. You're security requires are extremely sensitive and you want to reduce
   the footprint of features that Vector exposes.

## Getting Started

To get started, check out the [feature flags][docs.from-source#feature-flags]
section in our [build from source docs][docs.from-source]. For example:

```bash
FEATURES="sources-file,transforms-json_parser,sinks-kafka" make build
```

[docs.from-source#feature-flags]: /docs/setup/installation/manual/from-source/#feature-flags
[docs.from-source]: /docs/setup/installation/manual/from-source/
[pages.components]: /components/
