---
title: Install Vector using Helm
short: Helm
weight: 3
---

[Helm] is a package manager for Kubernetes that facilitates the deployment and management of applications and services on Kubernetes clusters. This page will cover installing and managing Vector through the Helm package repository.

## Installation

{{< warning title="Aggregator role in private beta" >}}
Helm support for the [aggregator] role is currently in private beta. We're currently seeking beta testers. If interested, please [join our chat][chat] and let us know.

As an alternative, you can still manually deploy Vector in the aggregator role. Instructions throughout this doc will be for the [agent] role only.

[agent]: /docs/about/setup/deployment/roles/#agent
[aggregator]: /docs/about/setup/deployment/roles/#aggregator
[chat]: https://chat.vector.dev
{{< /warning >}}

Add the Vector repo:

```shell
helm repo add timberio https://packages.timber.io/helm/latest
```

Check available Helm chart configuration options:

```shell
helm show values timberio/vector-agent
```

Configure Vector:

```toml
cat <<-'VALUES' > values.yaml
# The Vector Kubernetes integration automatically defines a
# kubernetes_logs source that is made available to you.
# You do not need to define a log source.
sinks:
  # Adjust as necessary. By default we use the console sink
  # to print all data. This allows you to see Vector working.
  # /docs/reference/sinks/
  stdout:
    type: console
    inputs: ["kubernetes_logs"]
    target: "stdout"
    encoding: "json"
VALUES
```

Install Vector:

```shell
helm install timberio/vector-agent \
  --namespace vector \
  --create-namespace vector \
  --values values.yaml
```

## Deployment

Vector is an end-to-end observability data pipeline designed to deploy under various roles. You mix and match these roles to create topologies. The intent is to make Vector as flexible as possible, allowing you to fluidly integrate Vector into your infrastructure over time. The deployment section demonstrates common Vector pipelines:

{{< jump "/docs/setup/deployment/topologies" >}}

## Administration

### Restart

```shell
kubectl rollout restart --namespace vector daemonset/vector-agent
```

### Observe

```shell
kubectl logs --namespace vector daemonset/vector-agent
```

### Upgrade

```shell
helm repo update && helm upgrade --namespace vector vector timberio/vector-agent --reuse-values
```

### Uninstall

```shell
helm uninstall --namespace vector vector
```

[helm]: https://helm.sh
