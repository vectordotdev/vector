---
date: "2021-12-15"
title: "A New Disk Buffer"
description: "New disk buffer for improved performance and correctness"
authors: ["barieom"]
pr_numbers: [10261, 10135]
release: "0.19.0"
hide_on_release_notes: false
badges:
  type: enhancement
---

We're thrilled to announce the release of a new disk buffer, drastically improving the
durability of Vector in production environments.

Previously, Vector users had to switch to disk buffering to incorporate additional
level of reliability in mission critical environments where data loss is unnegotiable.
More specifically, this was necessary to prevent data loss when given downstream sinks
experience temporary issues or the machines running Vector themselves have problems that
cause them to crash.

With this new release, we are excited to offer a performant disk buffer that makes your
pipelines even more reliable. Under the hood, we've eliminated our dependency to LevelDB,
replacing it completely with Rust. 



[Splunk indexer]: https://docs.splunk.com/Documentation/Splunk/8.2.3/Data/AboutHECIDXAck
[indexer how it works]: https://master.vector.dev/docs/reference/configuration/sinks/splunk_hec_metrics/#indexer-acknowledgements
[Discord chat]: https://discord.com/invite/dX3bdkF
[Twitter]: https://twitter.com/vectordotdev
