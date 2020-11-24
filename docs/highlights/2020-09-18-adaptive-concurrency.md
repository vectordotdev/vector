---
last_modified_on: "2020-09-18"
$schema: ".schema.json"
title: "Adaptive request concurrency"
description: "Increasing reliability and performance across your entire observability infrastructure."
author_github: "https://github.com/binarylogic"
pr_numbers: [3094]
release: "0.11.0"
hide_on_release_notes: false
tags: ["type: announcement", "domain: networking", "domain: reliability", "domain: performance"]
---

Vector 0.11 includes a new Adaptive Request Concurrency (ARC) feature that
raises the performance and reliability of your entire observability
infrastructure without any changes on your part. In short, it does away with
static rate-limits and automatically optimizes HTTP concurrency limits based on
downstream service responses. The underlying [mechanism](#how-it-works) is a
simple feedback loop inspired by TCP congestion control algorithms.

We highly recommend reading the [announcement blog post][announcement].

## Get Started

This feature, like all Vector features, will begin its life in public beta and
be available on an opt-in basis. To get it, enable it for each sink:

```toml
[sinks.my-sink]
type = "..."
```

[announcement]: /blog/...
