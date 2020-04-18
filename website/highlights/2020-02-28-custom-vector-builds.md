---
last_modified_on: "2020-04-13"
$schema: "/.meta/.schemas/highlights.json"
title: "A La Carte Custom Vector Builds"
description: "Build Vector with select components"
author_github: "https://github.com/binarylogic"
pr_numbers: [1924]
release: "nightly"
hide_on_release_notes: false
tags: ["type: new feature", "domain: sources", "source: vector"]
---

We've implemented a feature flag system that lets you build custom versions
of Vector with selected [components][pages.components]. This is handy if:

1. You're embedding Vector and you want to minimize the binary size as much as
   possible.
2. You're security requires are extremely sensitive and you want to reduce
   the footprint of features that Vector exposes.

To get started, check out the [feature flags][docs.from-source#feature-flags]
section in our [build from source docs][docs.from-source].


[docs.from-source#feature-flags]: /docs/setup/installation/manual/from-source/#feature-flags
[docs.from-source]: /docs/setup/installation/manual/from-source/
[pages.components]: /components/
