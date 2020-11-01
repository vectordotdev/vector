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

Vector 0.11 includes a new adaptive conccurrency feature designed to
automatically optimize network bandwidth, backoff when it senses trouble,
and make your observability infrastructure significantly more performant and
reliable.

## Adaptive Concurrency. The Elevator Pitch.

We cover this feature in-depth in our [announcement blog post][announcement],
but here's the skinny.

One of the most difficult things to do in distributed systems is coordinating
communication in a way that optimizes performance without risking reliability.
This is often achieved through rate-limits. In theory, rate-limits sound simple,
but in practice they're often the largest source of performance degredation.

For example, setting rate-limits too high risks overwhelming services causing
them fail. And setting them too low artifically limits performance.

<insert diagram demonstrating this>

Finding the right balance is impossibly difficult in cloud environments where
capacity and data volume are constantly changing. And because observability
pipelines absolutely cannot go down, we found that performance was _severely_
limited in real-world setups.

> In some cases we found that performance was artificially limited by over 60%.

Enter Vector's adapative concurrency feature. This feature does away with rate
limits and automatically finds the optimal network concurrency based on the
_current_ environment conditions. It's inspired by TCP congestion control
algorithms (AIMD to be exact):

* Deployed more Vector instances? Vector will work it out and divvy the capacity
  among Vector instances.
* Scaled up your Elasticsearch cluster? Vector will gradually increase it's
  network concurrency until it saturates bandwidth.

All of this is to say tha Vector cares about _real-world_ performance.
Performance is a feature of Vector and we are doubling down on our reputation as
the most performance and reliable observability pipeline.

If you have the time, we highly recommend reading the
[announcement blog post][announcement].

## Get Started

As with all new Vector features, this feature is currently in public beta
and is opt-in only. To get it, just enable it for each sink:

```toml
[sinks.my-sink]
type = "..."
```

[announcement]: ...
