---
date: "2020-09-18"
title: "Adaptive Request Concurrency (ARC)"
description: "Increasing reliability and performance across your entire observability infrastructure."
authors: ["lucperkins"]
pr_numbers: [3094]
release: "0.11.0"
hide_on_release_notes: false
badges:
  type: "new feature"
  domains: ["networking"]
  domains: ["performance", "reliability"]
---

Vector 0.11 includes a new Adaptive Request Concurrency (ARC) feature that
raises the performance and reliability of your entire observability
infrastructure without any changes on your part. In short, it does away with
static rate-limits and automatically optimizes HTTP concurrency limits based on
downstream service responses. The underlying mechanism is a simple feedback loop
inspired by TCP congestion control algorithms.

[**Read the ARC announcement post →**][announcement]

## Get Started

This feature, like all Vector features, will begin its life in public beta and
be available on an opt-in basis. To get it, enable it for each sink:

```toml
[sinks.my-sink]
type = "..." # any http-based sink
request.concurrency = "adaptive"
# and remove the request.rate_limit_* settings
```

[announcement]: /blog/adaptive-request-concurrency/
