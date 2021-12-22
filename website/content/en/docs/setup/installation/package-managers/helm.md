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

This example configuration file lets you use Vector as an Agent to send Pod logs to standard output, Vector also collects host and internal metrics and exposes them via a Prometheus exporter. For more information about configuration options, see the [configuration] docs page.

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

An Aggregator, by default, is configured to accept events from a variety of common sources and writes them to standard output, it also reports internal metrics via a Prometheus exporter. For more information about configuration options, see the [Configuration] docs page.

The default sources and their associated ports are listed below:

```yaml
datadog_agent:
  address: 0.0.0.0:8282
  type: datadog_agent
fluent:
  address: 0.0.0.0:24224
  type: fluent
logstash:
  address: 0.0.0.0:5044
  type: logstash
splunk_hec:
  address: 0.0.0.0:8080
  type: splunk_hec
statsd:
  address: 0.0.0.0:8125
  mode: tcp
  type: statsd
syslog:
  address: 0.0.0.0:9000
  mode: tcp
  type: syslog
vector:
  address: 0.0.0.0:6000
  type: vector
  version: "2"
```

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
