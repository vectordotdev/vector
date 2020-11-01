---
last_modified_on: "2020-10-27"
$schema: ".schema.json"
title: "Switching to the MPL 2.0 License"
description: "The Vector project has switched to the Mozilla Public License 2.0"
author_github: "https://github.com/binarylogic"
pr_numbers: [1314]
release: "0.11.0"
hide_on_release_notes: false
tags: ["type: announcement"]
---

After eight months of development, [83 pull requests][kubernetes_pull_requests],
and intensive QA in clusters producing over 50 terabytes per day, we’re pleased
to announce Vector's first-class Kubernetes integration. We went deep on
the quality. It is our intent for Vector to become the single, best pipeline for
all Kubernetes observability data.

For a deepdive into our Kubernetes integration, checkout the
[announcement blog post][announcement_post].

PS - we also launched a new [adapative concurrency feature][adative_concurrency_post]
that compliments our Kubernetes integration.

## Get Started

We recommend installing Vector through our Helm charts:

1. Add our Helm repo:

   ```bash
   helm repo add timberio-nightly https://packages.timber.io/helm/nightly
   ```

2. Configure Vector:

   ```bash
   helm template vector timberio-nightly/vector --devel --values values.yaml --namespace vector
   ```

3. Deploy:

   ```bash
   helm install vector timberio-nightly/vector --devel --values values.yaml --namespace vector --create-namespace
   ```

For other methods, like `kubectl`, visit our
[Kubernetes installation docs][installation_docs].

[announcement_post]: TODO
[installation_docs]: TODO
[kubernetes_pull_requests]: TODO
