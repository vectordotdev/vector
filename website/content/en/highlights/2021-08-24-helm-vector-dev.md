---
date: "2021-08-24"
title: "Introducing https://helm.vector.dev"
description: "A new home for Vector's Helm charts"
authors: ["spencergilbert"]
pr_numbers: []
release: "0.16.0"
hide_on_release_notes: false
badges:
  type: "deprecation"
  platforms: ["helm"]
---

Vector’s 0.16.0 release will be the last version that publishes charts to both
https://packages.timber.io/helm/latest and https://packages.timber.io/helm/nightly repositories.

The new repository contains all released charts from the previous `latest` repository.
Moving forward we will be releasing charts at their own pace as we work towards the stable
releases for the _vector-agent_ and _vector-aggregator_ charts.

Development and issue tracking will be migrated to https://github.com/timberio/helm-charts
in the coming days.

## Upgrade Guide

The new repository can be added with:

```shell
helm repo add vector https://helm.vector.dev
```
