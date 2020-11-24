---
last_modified_on: "2020-10-27"
$schema: ".schema.json"
title: "First-class Kubernetes integration"
description: "Vector officially support Kubernetes with a first-class integration."
author_github: "https://github.com/binarylogic"
featured: true
pr_numbers: [1314]
release: "0.11.0"
hide_on_release_notes: false
tags: ["type: announcement"]
---

After eight months of development, [83 pull requests][kubernetes_pull_requests],
and intensive QA in clusters producing over 30 terabytes per day, we’re pleased
to announce Vector's first-class Kubernetes integration. It is our intent for
Vector to become the single, best platform for all Kubernetes observability
data.

For more details, checkout the
[Kubernetes announcement blog post][announcement_post].

## Get Started

To cut straight to the chase, check out our Kubernetes installation instructions:

<Jump to="/docs/setup/installation/platforms/kubernetes/#install">Kubernetes Installation Instructions</Jump>

## Notable features

1. A new [`kubernetes_logs` source] that:
   1. Automatically collects all Node logs.
   2. Automatically merges split logs due to the 12kb Docker limit.
   3. Enriches logs with Kubernetes metdata.
   4. Provides robust filtering options for Pod inclusion/exclusion.
   5. Is designed for scale. Vector is routinely benchmarked at 150k messages
      per second tailing hundreds of files.
2.
3. Composable design that allows k8s operators to include Vector in restricted
   and unrestricted setups.

Prometheus integrations

PS - we also launched a new [adapative concurrency feature][adative_concurrency_post]
that compliments our Kubernetes integration.

## Future plans

[announcement_post]: TODO
[installation_docs]: TODO
[kubernetes_pull_requests]: TODO
