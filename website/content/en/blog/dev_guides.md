---
title: Developer Guides
short: Developer Guides
description: New guides are now available!
authors: [ "pront" ]
date: "2025-02-12"
badges:
  type: announcement
  domains: [ "dev" ]
tags: [ "dev", "debugging", "guides", "guide" ]
---

TODO: Add more sections to this, since we have a lot of new features like exclusive route and OTEL sink.

# Announcement

We recently added a new guide category, [Developer]({{< ref "guides/developer/_index.md" >}})!

## Debugging Guide

You can find the new debugging guide [here]({{< ref "guides/developer/debugging/debugging/" >}}). The goal of this guide is to serve as a
starting for Vector users who are experiencing issues with their Vector configurations and deployments. In addition, it highlights
debugging tools that Vector offers.

## IDE Support

You can find the new debugging guide [here]({{< ref "guides/developer/config-autocompletion/" >}}).
We supported generating a Vector config schema for a while. This guide demonstrates how we can leverage this schema and import it into
popular IDEs in order to help with writing configs.

We would love to this to VRL IDE support in the future. In the meantime, we would like to highlight
this   [tree-sitter](https://github.com/tree-sitter/tree-sitter) plugin that was developed by https://github.com/belltoy. You can read more
[here](https://github.com/vectordotdev/vrl/issues/964).