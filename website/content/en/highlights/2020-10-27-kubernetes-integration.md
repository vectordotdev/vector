---
date: "2020-10-27"
title: "First-class Kubernetes integration"
description: "Vector officially supports Kubernetes with a first-class integration."
authors: ["binarylogic"]
featured: true
pr_numbers: [1314]
release: "0.11.0"
hide_on_release_notes: false
badges:
  type: "featured"
  domains: ["platforms"]
  platforms: ["kubernetes"]
---

After eight months of development, [100 pull requests][kubernetes_pull_requests],
and intensive QA in clusters producing over 30 terabytes per day, we’re pleased
to announce Vector's first-class Kubernetes integration. It is our intent for
Vector to become the single, best platform for collecting and processing all
Kubernetes observability data.

[**Read the Kubernetes announcement post →**][announcement_post]

## Feature highlights

1.  [**A new `kubernetes_logs` source**][kubernetes_logs_source] - A new source
    designed to handle the intricacies of Kubernetes log collection. It'll
    collect all Pod logs, merge split logs together, and enrich them with k8s
    metadata.
2.  [**YAML config support**][config_formats_highlight] -
    To ensure Vector fits cleanly into your existing K8s workflows, Vector now
    accepts YAML and JSON config formats.
3.  [**Adaptive Request Currency (ARC)**][adaptive_concurrency_post] -
    A new Vector feature designed to automatically optimize HTTP communication
    in the face of ever changing environments like Kubernetes. It does away with
    static rate limits and raises the performance and reliability of your entire
    observability infrastructure by monitoring downstream service performance.

## Get Started

To get started, follow the install instructions:

[**Kubernetes Installation Instructions →**][installation_docs]

[adaptive_concurrency_post]: /blog/adaptive-request-concurrency/
[config_formats_highlight]: /highlights/2020-11-25-json-yaml-config-formats/
[announcement_post]: /blog/kubernetes-integration/
[installation_docs]: /docs/setup/installation/platforms/kubernetes/
[kubernetes_logs_source]: /docs/reference/configuration/sources/kubernetes_logs/
[kubernetes_pull_requests]: https://github.com/vectordotdev/vector/pulls?q=is%3Apr+sort%3Aupdated-desc+kubernetes+is%3Aclosed
