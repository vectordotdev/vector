---
last_modified_on: "2020-09-18"
$schema: ".schema.json"
title: "Adaptive concurrency. Optimizing performance & reliability."
description: "How Vector optimizes performance and increases reliability of your entire observability infrastructure."
author_github: "https://github.com/binarylogic"
pr_numbers: [3094]
release: "0.11.0"
hide_on_release_notes: false
tags: ["type: announcement", "domain: networking"]
---

Vector 0.11 includes a new adaptive concurrency feature designed to
automatically optimize network bandwidth, back off when it senses trouble,
and make your observability infrastructure significantly more performant and
reliable.

## Adaptive Concurrency. The Elevator Pitch.

We cover this feature in depth in our [announcement blog post][announcement],
but here's the skinny.

One of the most challenging things to do in distributed systems is to coordinate
communication in a way that optimizes performance without risking reliability.
This is often achieved through rate-limits. In theory, static rate limits
sound simple, but in practice, they're usually the largest source of performance
degradation.

Setting rate-limits too high, for example, risks overwhelming services, causing
them to fail, while setting them too low artificially limits performance.

<insert diagram demonstrating this>

Finding the right balance is difficult in cloud environments where capacity and
data volume are continually changing. And because observability pipelines
absolutely cannot go down, we found that performance was _severely_ limited in
real-world setups.

> In some cases, we found that performance was artificially limited by over 60%.

Enter Vector's adaptive concurrency feature. This feature does away with static
rate limits and automatically finds the optimal network concurrency based on the
_current_ environment conditions. It's inspired by AIMD TCP congestion control
algorithm:

* Deployed more Vector instances? Vector will work it out and divvy the capacity
  among Vector instances.
* Scaled up your Elasticsearch cluster? Vector will gradually increase its
  network concurrency until it saturates bandwidth.

All of this is to say that Vector cares about _real-world_ performance.
Performance is a feature of Vector, and it is our goal to be the highest
performing and most reliability observability pipeline.

We highly recommend reading the [announcement blog post][announcement].

## Get Started

This feature, like all Vector features, will begin its life in public beta and
be available on an opt-in basis. To get it, enable it for each sink:

```toml
[sinks.my-sink]
type = "..."
```

[announcement]: ...
