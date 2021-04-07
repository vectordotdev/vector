---
title: Adaptive Request Concurrency (ARC)
sidebar_label: ARC
description: The fundamental Vector concepts. A great place to start learning about Vector.
---

![The Adaptive Request Concurrency decision chart](/img/adaptive-concurrency.png)

ARC (Adaptive Request Concurrency) is a Vector networking feature that does away
with static rate limits and automatically optimizes HTTP concurrency limits
based on downstream service responses. The underlying [mechanism](#how-it-works)
is a feedback loop inspired by TCP congestion control algorithms.

The end result is improved performance and reliability across your entire
observability infrastructure.

<Jump to="/blog/adaptive-request-concurrency/">Read the feature announcement</Jump>
