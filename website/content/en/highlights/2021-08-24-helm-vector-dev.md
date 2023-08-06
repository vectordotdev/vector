---
date: "2021-08-24"
title: "Introducing helm.vector.dev"
description: "A new home for Vector's Helm charts"
authors: ["spencergilbert"]
pr_numbers: []
release: "0.16.0"
hide_on_release_notes: false
badges:
  type: "deprecation"
  platforms: ["helm"]
---

Vector's new home for helm charts is at https://helm.vector.dev!

We made this change as part of migrating the helm charts from our old AWS S3 bucket hosting to
GitHub Pages via [vectordotdev/helm-charts](https://github.com/vectordotdev/helm-charts). This new domain
will also allow us to swap hosting in the future without any user impact.

Vector’s 0.16.x release will be the last version that publishes charts to both
https://packages.timber.io/helm/latest and https://packages.timber.io/helm/nightly repositories.

The new repository contains all released charts from the previous `latest` repository.
Moving forward we will be releasing charts at their own pace as we work towards the stable
releases for the _vector-agent_ and _vector-aggregator_ charts.

Development and issue tracking will be migrated to https://github.com/vectordotdev/helm-charts
in the coming days.

## Upgrade Guide

The new repository can be added with:

```shell
helm repo add vector https://helm.vector.dev
helm repo update
```

Once added the _vector-agent_ chart can be installed with:

```shell
helm install vector vector/vector-agent \
  --namespace vector \
  --create-namespace \
```

Or upgraded with:

```shell
helm upgrade vector vector/vector-agent \
  --namespace vector \
  --reuse-values
```
