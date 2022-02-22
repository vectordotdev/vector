---
date: "2022-02-16"
title: "Acknowledgement configuration move"
description: "Configuration of acknowledgements has moved from sources to sinks"
authors: ["bruceg"]
hide_on_release_notes: false
pr_numbers: [11346]
release: "0.21.0"
badges:
  type: "deprecation"
  domains: ["config"]
---

Currently, end-to-end acknowledgements are opt-in at the source-level
via the `acknowledgements.enabled` setting. This made sense initially
since sources are the ones that are acknowledging back to clients, but
makes it difficult to achieve durability. Durability, which is the
primary goal of acknowledgements, is sink-dependent instead of
source-dependent. That is, it is important to assert that all data
going to a system of record is fully acknowledged, for example, for
all the sources that it came from, this guaranteeing delivery to that
destination.

To achieve this, an `acknowledgements` option has been added to
sinks. When the configuration is loaded, all sources that are
connected to a sink that has this option enabled will automatically be
configured to wait for sinks to acknowledge before issuing their own
acknowledgements (where possible).

The source configuration `acknowledgements` option will remain in this
version, but is deprecated and will be removed in version 0.22.0.

See [the documentation for end-to-end
acknowledgements][acknowledgements] for more details on the
acknowledgement process.

[acknowledgements]: https://vector.dev/docs/about/under-the-hood/architecture/end-to-end-acknowledgements/
