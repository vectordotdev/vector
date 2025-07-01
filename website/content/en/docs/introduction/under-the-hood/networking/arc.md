---
title: Adaptive request concurrency (ARC)
short: ARC
weight: 1
tags: ["arc", "request", "concurrency", "adaptive request concurrency", "performance", "http"]
---

![The Adaptive Request Concurrency decision chart](/img/adaptive-concurrency.png)

**Adaptive Request Concurrency** (ARC) is a Vector networking feature that does away with static rate limits and automatically optimizes HTTP concurrency limits based on downstream service responses. The underlying mechanism is a feedback loop inspired by TCP congestion control algorithms.

The end result is improved performance and reliability across your entire observability infrastructure.

Check out the feature announcement for more information:

{{< jump "/blog/adaptive-request-concurrency" >}}
