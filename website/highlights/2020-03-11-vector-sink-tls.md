---
last_modified_on: "2020-04-13"
$schema: "/.meta/.schemas/highlights.json"
title: "The Vector Source & Sink Support TLS"
description: "Securely forward data between Vector instances"
author_github: "https://github.com/binarylogic"
pr_numbers: [2025]
release: "0.9.0"
hide_on_release_notes: true
tags: ["type: new feature", "domain: sources", "source: vector"]
---

A highly requested feature of Vector is to support the TLS protocol for the
[`vector` source][docs.sources.vector] and [`vector` sink][docs.sinks.vector].
This is now available. Check out the `tls.*` options.


[docs.sinks.vector]: /docs/reference/sinks/vector/
[docs.sources.vector]: /docs/reference/sources/vector/
