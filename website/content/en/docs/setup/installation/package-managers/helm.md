---
title: Install Vector using Helm
short: Helm
weight: 3
---

[Helm] is a package manager for Kubernetes that facilitates the deployment and management of applications and services on Kubernetes clusters. This page covers installing and managing the Vector chart.

## Adding the Helm repo

If you haven't already, start by adding the Vector repo:

```shell
helm repo add vector https://helm.vector.dev
helm repo update
```

## Agent

The Vector [Agent] lets you collect data from your [sources] and then deliver it to a variety of destinations with [sinks].

### Configuring

To check available Helm chart configuration options:

```shell
helm show values vector/vector
```

This example configuration file deploys Vector as an Agent, the full default configuration can be found [here](https://github.com/vectordotdev/helm-charts/blob/develop/charts/vector/templates/configmap.yaml). For more information about configuration options, see the [configuration] docs page.

```yaml
cat <<-'VALUES' > values.yaml
role: Agent
VALUES
```

### Installing

Once you add the Vector Helm repo, and added a Vector configuration file, install the Vector Agent:

```shell
helm install vector vector/vector \
  --namespace vector \
  --create-namespace \
  --values values.yaml
```

### Updating

Or to update the Vector Agent:

```shell
helm repo update && \
helm upgrade vector vector/vector \
  --namespace vector \
  --reuse-values
```

## Aggregator

The Vector [Aggregator] lets you [transform] and ship data collected by other agents. For example, it can insure that the data you are collecting is scrubbed of sensitive information, properly formatted for downstream consumers, sampled to reduce volume, and more.

### Configuring

To check available Helm chart configuration options:

```shell
helm show values vector/vector
```

The chart deploys an Aggregator by default, the full configuration can be found [here](https://github.com/vectordotdev/helm-charts/blob/develop/charts/vector/templates/configmap.yaml). For more information about configuration options, see the [Configuration] docs page.

### Installing

Once you add the Vector Helm repo, install the Vector Aggregator:

```shell
helm install vector vector/vector \
  --namespace vector \
  --create-namespace
```

### Updating

Or to update the Vector Aggregator:

```shell
helm repo update && \
helm upgrade vector vector/vector \
  --namespace vector \
  --reuse-values
```

## Uninstalling Vector

To uninstall the Vector helm chart:

```shell
helm uninstall vector --namespace vector
```

## View Helm Chart Source

If you'd like to clone the charts, file an Issue or submit a Pull Request, please take a look at [vectordotdev/helm-charts](https://github.com/vectordotdev/helm-charts).

## Management

{{< jump "/docs/administration/management" "helm" >}}

[helm]: https://helm.sh
[Configuration]: /docs/reference/configuration/
[Agent]: /docs/setup/deployment/roles/#agent
[sources]: /docs/reference/configuration/sources/
[sinks]: /docs/reference/configuration/sinks/
[Aggregator]: /docs/setup/deployment/roles/#aggregator
[transform]: /docs/reference/configuration/transforms/
