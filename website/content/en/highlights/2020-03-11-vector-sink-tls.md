---
date: "2020-04-13"
title: "The Vector Source and Sink Support TLS"
description: "Securely forward data between Vector instances"
authors: ["binarylogic"]
pr_numbers: [2025]
release: "0.9.0"
hide_on_release_notes: true
badges:
  type: "new feature"
  domains: ["sources"]
  sources: ["vector"]
---

A highly requested feature of Vector is to support the TLS protocol for the
[`vector` source][docs.sources.vector] and [`vector` sink][docs.sinks.vector].
This is now available. Check out the `tls.*` options.

[docs.sinks.vector]: /docs/reference/configuration/sinks/vector/
[docs.sources.vector]: /docs/reference/configuration/sources/vector/
