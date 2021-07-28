---
title: Install Vector using Helm
short: Helm
weight: 3
---

[Helm] is a package manager for Kubernetes that facilitates the deployment and management of applications and services on Kubernetes clusters. This page covers installing and managing Vector through the Helm package repository.

## Agent

### Installation

Add the Vector repo:

```shell
helm repo add timberio https://packages.timber.io/helm/latest
helm repo update
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
helm install vector timberio/vector-agent \
  --namespace vector \
  --create-namespace \
  --values values.yaml
```

### Other actions

{{< tabs default="Upgrade Vector" >}}
{{< tab title="Upgrade Vector" >}}
```shell
helm repo update && \
helm upgrade vector timberio/vector-agent \
  --namespace vector \
  --reuse-values
```
{{< /tab >}}
{{< tab title="Uninstall Vector" >}}
```shell
helm uninstall vector --namespace vector 
```
{{< /tab >}}
{{< /tabs >}}

## Aggregator

### Installation

Add the Vector repo:

```shell
helm repo add timberio https://packages.timber.io/helm/latest
helm repo update
```

Check available Helm chart configuration options:

```shell
helm show values timberio/vector-aggregator
```

Configure Vector:

```toml
cat <<-'VALUES' > values.yaml
# The Vector Kubernetes integration automatically defines a
# vector source that is made available to you.
# You do not need to define a log source.
sinks:
  # Adjust as necessary. By default we use the console sink
  # to print all data. This allows you to see Vector working.
  # /docs/reference/sinks/
  stdout:
    type: console
    inputs: ["vector"]
    target: "stdout"
    encoding: "json"
VALUES
```

Install Vector:

```shell
helm install vector timberio/vector-aggregator \
  --namespace vector \
  --create-namespace \
  --values values.yaml
```

### Other actions

{{< tabs default="Upgrade Vector" >}}
{{< tab title="Upgrade Vector" >}}
```shell
helm repo update && \
helm upgrade vector timberio/vector-aggregator \
  --namespace vector \
  --reuse-values
```
{{< /tab >}}
{{< tab title="Uninstall Vector" >}}
```shell
helm uninstall vector --namespace vector 
```
{{< /tab >}}
{{< /tabs >}}

## Management

{{< jump "/docs/administration/management" "helm" >}}

[helm]: https://helm.sh
