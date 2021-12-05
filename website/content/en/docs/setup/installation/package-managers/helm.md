---
title: Install Vector using Helm
short: Helm
weight: 3
---

[Helm] is a package manager for Kubernetes that facilitates the deployment and management of applications and services on Kubernetes clusters. This page covers installing and managing the Vector agent chart and the Vector aggregator chart from the Helm package repository. The agent chart and the aggregator chart are made available by adding the Vector Helm repository but are installed and configured separately.

{{< warning title="Aggregator role in public beta" >}}
Helm support for the [aggregator] role is currently in public beta. We're seeking beta testers! If deploying the aggregator chart, please [join our chat][chat] and let us know how it went.

[aggregator]: /docs/setup/deployment/roles/#aggregator
[chat]: https://chat.vector.dev
{{< /warning >}}

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
helm show values vector/vector-agent
```

This example configuration file lets you use Vector as an Agent to send logs to standard output. For more information about configuration options, see the [configuration] docs page.

```yaml
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

### Installing

Once you add the Vector Helm repo, and added a Vector configuration file, install the Vector Agent:

```shell
helm install vector vector/vector-agent \
  --namespace vector \
  --create-namespace \
  --values values.yaml
```

### Updating

Or to update the Vector Agent:

```shell
helm repo update && \
helm upgrade vector vector/vector-agent \
  --namespace vector \
  --reuse-values
```

## Aggregator

The Vector [Aggregator] lets you [transform] and ship data collected by other agents. For example, it can insure that the data you are collecting is scrubbed of sensitive information, properly formatted for downstream consumers, sampled to reduce volume, and more.

### Configuring

To check available Helm chart configuration options:

```shell
helm show values vector/vector-aggregator
```

This example configuration file lets you use Vector as an Aggregator to parse events to make them human-readable. For more information about configuration options, see the [Configuration] docs page.

```yaml
cat <<-'VALUES' > values.yaml
# The Vector Aggregator chart defines a
# vector source that is made available to you.
# You do not need to define a log source.
transforms:
  # Adjust as necessary. This remap transform parses a JSON
  # formatted log message, emitting a log if the contents are
  # not valid JSON
  # /docs/reference/transforms/
  remap:
    type: remap
    inputs: ["vector"]
    source: |
      structured, err = parse_json(.message)
      if err != null {
        log("Unable to parse JSON: " + err, level: "error")
      } else {
        . = merge(., object!(structured))
      }
sinks:
  # Adjust as necessary. By default we use the console sink
  # to print all data. This allows you to see Vector working.
  # /docs/reference/sinks/
  stdout:
    type: console
    inputs: ["remap"]
    target: "stdout"
    encoding: "json"
VALUES
```

### Installing

Once you add the Vector Helm repo, and add a Vector configuration file, install the Vector Aggregator:

```shell
helm install vector vector/vector-aggregator \
  --namespace vector \
  --create-namespace \
  --values values.yaml
```

### Updating

Or to update the Vector Aggregator:

```shell
helm repo update && \
helm upgrade vector vector/vector-aggregator \
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
