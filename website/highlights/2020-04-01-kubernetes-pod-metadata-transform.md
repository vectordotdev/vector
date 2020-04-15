---
last_modified_on: "2020-04-13"
$schema: "/.meta/.schemas/highlights.json"
title: "New Kubernetes Pod Metdata Transform"
description: "Easily rnrich your logs with Kubernetes metadata"
author_github: https://github.com/binarylogic
pr_numbers: [1888]
release: "nightly"
importance: "high"
tags: ["type: new feature", "domain: sources", "source: vector"]
---

The Vector team is working hard to deliver a best-in-class Kubernetes
integration (scheduled for `0.10.0`). We have not officially announced this
integration because it is still in alpha. We are testing with a number of large
clusters and finalizing the integration details in [RFC#2222][urls.pr_2222]
(feel free to chime in!). To that end, we've released a new
[`kubernetes_pod_metadata` transform][docs.transforms.kubernetes_pod_metadata]
that enriches your Kubernetes logs with juicy Kubernetes metadata. Some details:

* [14 fields][docs.transforms.kubernetes_pod_metadata#fields].
* The [ability to whitelist][docs.transforms.kubernetes_pod_metadata#fields] fields.
* Real-time updates to cluster changes via the Kubernetes watch API.
* [Asynchronous fetching and caching][docs.transforms.kubernetes_pod_metadata#building--maintaining-the-metadata] of this data for optimal performance.
* And just being a good Kubernetes citizen: handling k8s API failures with retries, exponential backoffs, and jitter. Avoiding issues like [this](https://github.com/fluent/fluent-bit/issues/1399).

Stay tuned for our official Kubernetes announcement, which is scheduled for
`0.10.0`, our next release.


[docs.transforms.kubernetes_pod_metadata#building--maintaining-the-metadata]: /docs/reference/transforms/kubernetes_pod_metadata/#building--maintaining-the-metadata
[docs.transforms.kubernetes_pod_metadata#fields]: /docs/reference/transforms/kubernetes_pod_metadata/#fields
[docs.transforms.kubernetes_pod_metadata]: /docs/reference/transforms/kubernetes_pod_metadata/
[urls.pr_2222]: https://github.com/timberio/vector/pull/2222
