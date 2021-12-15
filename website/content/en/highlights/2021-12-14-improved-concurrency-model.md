---
date: "2021-12-14"
title: "Improved concurrency model"
description: "New parallel processing with improved performance"
authors: ["barieom"]
pr_numbers: [10265]
release: "0.19.0"
hide_on_release_notes: false
badges:
  type: announcement
---

We've released an improved concurrency model that provides measurable performance
improvements and enables more efficient vertical scaling.

Previously, Vector set the limit to active tasks to the number of available CPUs
in a given environment. Transforms were run sequentially, which made transforms
a bottleneck in Vector pipelines.

With this new release, Vector will perform faster when a transform is a bottleneck,
assuming that more CPUs are available to share their work. This works by Vector
determine whether an individual transform will be processed in parallel when there
is a sufficient load to the environment.

To expand, this improvement works by spinning up multiple short-lived tasks
that concurrently run the same transform logic on separate batches of events.
In other words, they perform the processing for chunks of events at a time and
allowing a certain number of those tasks to be active at a time.


If you any feedback for us, let us know on our [Discord chat] or on [Twitter].

[Discord chat]: https://discord.com/invite/dX3bdkF
[Twitter]: https://twitter.com/vectordotdev
