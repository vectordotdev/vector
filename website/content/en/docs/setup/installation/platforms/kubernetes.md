---
title: Install Vector on Kubernetes
short: Kubernetes
weight: 2
---

{{< requirement title="Minimum Kubernetes version" >}}
Vector must be installed on Kubernetes version **1.14** or higher.
{{< /requirement >}}

[Kubernetes], also known as **k8s**, is an open source container orchestration system for automating application deployment, scaling, and management. This page covers installing and managing Vector on the Kubernetes platform.

## Install

You can install Vector on Kubernetes using either [Helm](#helm) or [kubectl](#kubectl).

### Helm

{{< jump "/docs/setup/installation/package-managers/helm" >}}

### kubectl

[kubectl] is the Kubernetes command-line tool. It enables you to manage Kubernetes clusters

## Roles

### Agent

### Aggregator

## Deployment

Vector is an end-to-end observability data pipeline designed to deploy under various roles. You mix and match these roles to create topologies. The intent is to make Vector as flexible as possible, allowing you to fluidly integrate Vector into your infrastructure over time. The deployment section demonstrates common Vector pipelines:

{{< jump "/docs/setup/deployment/topologies" >}}

## How it works

### Checkpointing

Vector checkpoints the current read position after each successful read. This ensures that Vector resumes where it left off when it's restarted, which prevents data from being read twice. The checkpoint positions are stored in the data directory which is specified via the global [`data_dir`][data_dir] option, but can be overridden via the `data_dir` option in the file source directly.

### Container exclusion

The [`kubernetes_logs` source][kubernetes_logs] can skip the logs from the individual `container`s of a particular Pod. Add an annotation `vector.dev/exclude-containers` to the Pod and enumerate the names of all the containers to exclude in the value of the annotation like so:

```shell
vector.dev/exclude-containers: "container1,container2"
```

This annotation makes Vector skip logs originating from the `container1` and `container2` of the Pod marked with the annotation, while logs from other containers in the Pod are collected.

[data_dir]: /docs/reference/configuration/global-options#data_dir
[kubectl]: https://kubernetes.io/docs/reference/kubectl/overview
[kubernetes]: https://kubernetes.io
[kubernetes_logs]: /docs/reference/configuration/sources/kubernetes_logs
