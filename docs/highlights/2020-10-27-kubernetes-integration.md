---
last_modified_on: "2020-10-27"
$schema: ".schema.json"
title: "First-class Kubernetes integration"
description: "Vector officially supports Kubernetes with a first-class integration."
author_github: "https://github.com/binarylogic"
featured: true
pr_numbers: [1314]
release: "0.11.0"
hide_on_release_notes: false
tags: ["type: featured", "domain: platforms", "platform: kubernetes"]
---

After eight months of development, [100 pull requests][kubernetes_pull_requests],
and intensive QA in clusters producing over 30 terabytes per day, we’re pleased
to announce Vector's first-class Kubernetes integration. It is our intent for
Vector to become the single, best platform for collecting and processing all
Kubernetes observability data.

[**Read the Kubernetes announcement post**][announcement_post]

## Feature highlights

1.  [**A new `kubernetes_logs` source**][kubernetes_logs_source] - A new source
    designed to handle the intricacies of Kuberenetes log collection. It'll
    collect all Pod logs, merge split logs together, and enrich them with k8s
    metadata.
2.  [**YAML config support**][config_formats_highlight] -
    Vector's [`file` source][file_source] powers the new `kubernetes_logs`
    source, and to keep up with very large Kubernetes deployments we invested
    in performance improvements. We were able to improve throughput by over 25%
    across the board. This [further raises the bar][file_soure_benchmarks] in
    file tailing performance to meet the high demands of large-scale Kubernetes
    environments.
3.  [**Adaptive Request Currency (ARC)**][adaptive_concurrency_post] -
    A new Vector feature designed to automatically optimize HTTP communication
    in the face of ever changing environments like Kubernetes. It does away with
    static rate limits and raises the performance and reliability of your entire
    observability infrastructure by monitoring downstream service performance.

## Get Started

To cut straight to the chase, check out the:

[**Kubernetes Installation Instructions**][installation_docs]

## Future plans

[adaptive_concurrency_post]: /blog/adaptive-request-concurrency/
[config_formats_highlight]: /highlights/2020-11-25-json-yaml-config-formats/
[announcement_post]: /blog/...
[installation_docs]: /docs/setup/installation/platforms/kubernetes/
[kubernetes_logs_source]: /docs/reference/sources/kubernetes_logs/
[kubernetes_pull_requests]: https://github.com/timberio/vector/pulls?q=is%3Apr+sort%3Aupdated-desc+kubernetes+is%3Aclosed
