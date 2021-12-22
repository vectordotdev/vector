---
date: "2021-12-14"
title: "Improved concurrency model"
description: "New parallel processing with improved performance"
authors: ["barieom"]
pr_numbers: [10265]
release: "0.19.0"
hide_on_release_notes: false
badges:
  type: announcement
---

We've released an improved concurrency model that provides measurable performance
improvements and enables more efficient vertical scaling.

Previously, Vector was limited to executing transforms on a single thread, which led to them often being the bottleneck in Vector pipelines.

With this new release, Vector will perform faster when a transform is a bottleneck,
assuming that more CPUs are available to share their work. This works by Vector
determine whether an individual transform will be processed in parallel when there
is a sufficient load to the environment. 

This improvement works by spinning up multiple short-lived tasks that concurrently
run the same transform logic on separate batches of events. No configuration
changes are necessary to start taking advantage of this feature. The current list of
transforms that support parallelization are:
```
src/transforms/tokenizer.rs
src/transforms/regex_parser.rs
src/transforms/metric_to_log.rs
src/transforms/filter.rs
src/transforms/grok_parser.rs
src/transforms/key_value_parser.rs
src/transforms/json_parser.rs
src/transforms/logfmt_parser.rs
src/transforms/log_to_metric.rs
src/transforms/remap.rs
src/transforms/aws_cloudwatch_logs_subscription_parser.rs
```


If you any feedback for us, let us know on our [Discord chat] or on [Twitter].

[Discord chat]: https://discord.com/invite/dX3bdkF
[Twitter]: https://twitter.com/vectordotdev
