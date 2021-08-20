---
title: Install Vector using Helm
short: Helm
weight: 3
---

[Helm] is a package manager for Kubernetes that facilitates the deployment and management of applications and services on Kubernetes clusters. This page covers installing and managing the Vector Agent and the Vector Aggregator through the Helm package repository. The Agent and the Aggregator are added by adding the Vector Helm chart but are installed onto each node and configured separately.

{{< warning title="Aggregator role in private beta" >}}
Helm support for the [aggregator] role is currently in private beta. We're currently seeking beta testers. If interested, please [join our chat][chat] and let us know.

[agent]: /docs/setup/deployment/roles/#agent
[aggregator]: /docs/setup/deployment/roles/#aggregator
[chat]: https://chat.vector.dev
{{< /warning >}}

## Adding the Helm repo

If you haven't already, start by adding the Vector repo:

```shell
helm repo add timberio https://packages.timber.io/helm/latest
helm repo update
```

## Agent

The Vector [Agent] lets you collect data from your [sources] and then deliver it to a variety of destinations with [sinks].

### Installing

Once you add the Vector Helm repo, install the Vector Agent to each node:

```shell
helm install vector timberio/vector-agent \
  --namespace vector \
  --create-namespace \
  --values values.yaml
```

### Updating

Or to update the Vector Agent:

```shell
helm repo update && \
helm upgrade vector timberio/vector-agent \
  --namespace vector \
  --reuse-values
```

### Configuring

To check available Helm chart configuration options:

```shell
helm show values timberio/vector-agent
```

This example configuration file lets you use Vector as an Agent to send logs to standard output. For more information about configuration options, see the [configuration] docs page.

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
## Aggregator

The Vector [Aggregator] lets you [transform] your data. For example, dedupe, aggregate, or redact the data before sending it to its final destination.

### Installing

Once you add the Vector Helm repo, install the Vector Aggregator to each node:

```shell
helm install vector timberio/vector-aggregator \
  --namespace vector \
  --create-namespace \
  --values values.YAML
```

### Updating

Or to update the Vector Aggregator:

```shell
helm repo update && \
helm upgrade vector timberio/vector-aggregator \
  --namespace vector \
  --reuse-values
```

### Configuring

To check available Helm chart configuration options:

```shell
helm show values timberio/vector-aggregator
```

This example configuration file lets you use Vector as an Aggregator to parse events to make them human-readable. For more information about configuration options, see the [Configuration] docs page.

```toml
# Add example
```

## Uninstalling Vector

To uninstall the Vector helm chart:

```shell
helm uninstall vector --namespace vector 
```

## Management

{{< jump "/docs/administration/management" "helm" >}}

[helm]: https://helm.sh
[Configuration]: /docs/reference/configuration/
[Agent]: /docs/setup/deployment/roles/#agent
[sources]: /docs/reference/configuration/sources/
[sinks]: /docs/reference/configuration/sinks/
[Aggregator]: /docs/setup/deployment/roles/#aggregator