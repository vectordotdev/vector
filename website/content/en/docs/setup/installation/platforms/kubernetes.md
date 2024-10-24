---
title: Install Vector on Kubernetes
short: Kubernetes
weight: 2
---

{{< requirement title="Minimum Kubernetes version" >}}
Vector must be installed on Kubernetes version **1.15** or higher. Vector is
tested with Kubernetes versions **1.19** or higher.
{{< /requirement >}}

[Kubernetes], also known as **k8s**, is an open source container orchestration system for automating application deployment, scaling, and management. This page covers installing and managing Vector on the Kubernetes platform.

## Install

You can install Vector on Kubernetes using [Helm](#helm), [kubectl](#kubectl) or [Vector Operator](#vector-operator)

### Helm

{{< jump "/docs/setup/installation/package-managers/helm" >}}

### kubectl

[kubectl] is the Kubernetes command-line tool. You can use it as an alternative to [Helm](#helm) to install Vector on Kubernetes The instructions below are for installing Vector in the [Agent] and [Aggregator] roles.

[agent]: /docs/setup/deployment/roles/#agent
[aggregator]: /docs/setup/deployment/roles/#aggregator

#### Agent

The Vector [Agent] lets you collect data from your [sources] and then deliver it to a variety of destinations with [sinks].

##### Define Vector's namespace

We recommend running Vector in its own Kubernetes namespace. In the instructions here we'll use `vector` as a namespace but you're free to choose your own.

```shell
kubectl create namespace --dry-run=client -o yaml vector > namespace.yaml
```

##### Prepare your kustomization file

This example configuration file deploys Vector as an Agent, the full default configuration can be found [here](https://github.com/vectordotdev/helm-charts/blob/develop/charts/vector/templates/configmap.yaml). For more information about configuration options, see the [configuration] docs page.

```shell
cat <<-'KUSTOMIZATION' > kustomization.yaml
---
apiVersion: kustomize.config.k8s.io/v1beta1
kind: Kustomization

# Override the namespace of all of the resources we manage.
namespace: vector

bases:
  # Include Vector recommended base (from git).
  - github.com/vectordotdev/vector/tree/master/distribution/kubernetes/vector-agent

images:
  # Override the Vector image to pin the version used.
  - name: timberio/vector
    newName: timberio/vector
    newTag: {{< version >}}-distroless-libc

resources:
  # The namespace previously created to keep the resources in.
  - namespace.yaml
KUSTOMIZATION
```

##### Verify your kustomization file

```shell
kubectl kustomize
```

##### Install Vector

```shell
kubectl apply -k .
```

##### Tail Vector logs

```shell
kubectl logs -n vector daemonset/vector
```

#### Aggregator

The Vector [Aggregator] lets you [transform] and ship data collected by other agents. For example, it can insure that the data you are collecting is scrubbed of sensitive information, properly formatted for downstream consumers, sampled to reduce volume, and more.

##### Define Vector's namespace

We recommend running Vector in its own Kubernetes namespace. In the instructions here we'll use `vector` as a namespace but you're free to choose your own.

```shell
kubectl create namespace --dry-run=client -o yaml vector > namespace.yaml
```

##### Prepare your kustomization file

This example configuration deploys Vector as an Aggregator, the full configuration can be found [here](https://github.com/vectordotdev/helm-charts/blob/develop/charts/vector/templates/configmap.yaml). For more information about configuration options, see the [Configuration] docs page.

```shell
cat <<-'KUSTOMIZATION' > kustomization.yaml
---
apiVersion: kustomize.config.k8s.io/v1beta1
kind: Kustomization

# Override the namespace of all of the resources we manage.
namespace: vector

bases:
  # Include Vector recommended base (from git).
  - github.com/vectordotdev/vector/tree/master/distribution/kubernetes/vector-aggregator

images:
  # Override the Vector image to pin the version used.
  - name: timberio/vector
    newName: timberio/vector
    newTag: {{< version >}}-distroless-libc

resources:
  # The namespace previously created to keep the resources in.
  - namespace.yaml
KUSTOMIZATION
```

##### Verify your kustomization file

```shell
kubectl kustomize
```

##### Install Vector

```shell
kubectl apply -k .
```

##### Tail Vector logs

```shell
"kubectl logs -n vector statefulset/vector"
```

### Vector Operator

The [Vector Operator](https://github.com/kaasops/vector-operator) is community supported resource. The operator deploys and configures a Vector Agent as a DaemonSet on every Node to collect container and application logs from the Node's file system.

For additional information, see the [documentation](https://github.com/kaasops/vector-operator/tree/main/docs).

## Deployment

Vector is an end-to-end observability data pipeline designed to deploy under various roles. You mix and match these roles to create topologies. The intent is to make Vector as flexible as possible, allowing you to fluidly integrate Vector into your infrastructure over time. The deployment section demonstrates common Vector pipelines:

{{< jump "/docs/setup/deployment/topologies" >}}

## How it works

### Checkpointing

Vector checkpoints the current read position after each successful read. This ensures that Vector resumes where it left off when it's restarted, which prevents data from being read twice. The checkpoint positions are stored in the data directory which is specified via the global [`data_dir`][data_dir] option, but can be overridden via the `data_dir` option in the file source directly.

### Container exclusion

The [`kubernetes_logs` source][kubernetes_logs] can skip the logs from the individual `container`s of a particular Pod. Add an annotation `vector.dev/exclude-containers` to the Pod and enumerate the names of all the containers to exclude in the value of the annotation like so:

```yaml
vector.dev/exclude-containers: "container1,container2"
```

This annotation makes Vector skip logs originating from the `container1` and `container2` of the Pod marked with the annotation, while logs from other containers in the Pod are collected.

### Context

By default, the [`kubernetes_logs`][kubernetes_logs] source augments events with helpful content keys, as explained in the [Output][kubernetes_logs_output] section.

### Enrichment

Vector enriches data with Kubernetes context. You can find a comprehensive list of fields in the [`kubernetes_logs` source output docs][kubernetes_logs_output].

### Filtering

Vector provides rich filtering options for Kubernetes log collection:

* Built-in Pod and container exclusion rules
* The `exclude_paths_glob_patterns` option enables you to exclude Kubernetes log files by filename and path.
* The `extra_field_selector` option specifies the field selector to filter Pods with, to be used in addition to the built-in `Node` filter.
* The `extra_label_selector` option specifies the label selector filter Pods with, to be used in addition to the built-in [`vector.dev/exclude` filter][exclude_filter].
* The `extra_namespace_label_selector` option specifies the label selector filter Namespaces with, to be used in addition to the built-in [`vector.dev/exclude` filter][exclude_filter].

### Kubernetes API access control

Vector requires access to the Kubernetes API. Specifically, the [`kubernetes_logs` source][kubernetes_logs] source uses the `/api/v1/pods` endpoint to "watch" Pods from all namespaces.

Modern Kubernetes clusters run with a role-based access control (RBAC) scheme. RBAC-enabled clusters require some configuration to grant Vector the authorization to access Kubernetes API endpoints. As RBAC is currently the standard way of controlling access to the Kubernetes API, we ship the necessary configuration out of the box. See `ClusterRole`, `ClusterRoleBinding`, and a `ServiceAccount` in our kubectl YAML config and the `rbac` configuration in the Helm chart.

If your cluster doesn't use any access control scheme and doesn't restrict access to the Kubernetes API, you don't need to provide any extra configuration, as Vector should just work.

Clusters using a legacy ABAC scheme aren't officially supported, although Vector might work if you configure access properly. We encourage you to switch to RBAC. If you use a custom access control scheme, make sure that Vector is granted access to the `/api/v1/pods` resource.

### Kubernetes API communication

Vector communicates with the Vector API to enrich the data it collects with Kubernetes context. In order to do that, Vector needs access to the [Kubernetes API server][k8s_api]. If Vector is running in a Kubernetes cluster, Vector connects to that cluster using the [Kubernetes-provided access information][access_info].

In addition to access, Vector implements proper desync handling to ensure that communication is safe and reliable. This ensures that Vector doesn't overwhelm the Kubernetes API or compromise its stability.

### Metrics

Vector's Helm chart deployments provide quality of life around setup and maintenance of metrics pipelines in Kubernetes. Each of the Helm charts provides an `internal_metrics` source and `prometheus` sink out of the box. Agent deployments also expose `host_metrics` via the same `prometheus` sink.

Charts come with options to enable Prometheus integration via annotations or Prometheus Operator integration via PodMonitor. The Prometheus `node_exporter` agent isn't required when the `host_metrics` source is enabled.

### Partial message merging

By default, Vector merges partial messages that are split due to the Docker size limit. For everything else, we recommend that you use the [`reduce` transform][reduce], which enables you to handle custom merging of things like stacktraces.

### Pod exclusion

By default, the [`kubernetes_logs` source][kubernetes_logs] skips logs from Pods that have a `vector.dev/exclude: "true"` label. You can configure additional exclusion rules via label or field selectors. See the [available options][kubernetes_logs_config].

### Pod removal

To ensure that all data is collected, Vector continues to collect logs from Pods for some time after their removal. This ensures that Vector obtains some of the most important data, such as crash details.

### Resource limits

We recommend the resource limits listed below when running Vector on Kubernetes.

#### Agent resource limits

If you deploy Vector as an [Agent] (collecting data for each of your Kubernetes [Nodes][node]), we recommend the following limits:

```yaml
resources:
  requests:
    memory: "64Mi"
    cpu: "500m"
  limits:
    memory: "1024Mi"
    cpu: "6000m"
```

{{< info >}}
As with all Kubernetes resource limit recommendations, use these as a reference point and adjust as necessary. If your configuration Vector pipeline is complex, you may need more resources; if you have a simple pipeline, you may need less.
{{< /info >}}

### State

The [`kubernetes_logs`][kubernetes_logs] component is stateless, which means that its behavior is consistent across each input.

### State management

#### Agent state management

For the [Agent] role, Vector stores its state in the host-mapped directory with a static path. If it's redeployed, it's able to continue from where it was interrupted.

### Testing and reliability

Vector is tested extensively against Kubernetes. In addition to Kubernetes being Vector's most popular installation method, Vector implements a comprehensive end-to-end test suite for all minor Kubernetes versions beginning with 1.14.

[access_info]: https://kubernetes.io/docs/tasks/access-application-cluster/access-cluster/#accessing-the-api-from-a-pod
[agent]: /docs/setup/deployment/roles#agent

[daemonset]: https://kubernetes.io/docs/concepts/workloads/controllers/daemonset
[data_dir]: /docs/reference/configuration/global-options#data_dir
[exclude_filter]: /docs/setup/installation/platforms/kubernetes/#pod-exclusion
[k8s_api]: https://kubernetes.io/docs/reference/command-line-tools-reference/kube-apiserver
[kubectl]: https://kubernetes.io/docs/reference/kubectl/overview
[kubernetes]: https://kubernetes.io
[kubernetes_logs]: /docs/reference/configuration/sources/kubernetes_logs
[kubernetes_logs_config]: /docs/reference/configuration/sources/kubernetes_logs/#configuration
[kubernetes_logs_output]: /docs/reference/configuration/sources/kubernetes_logs#output-data
[kustomize]: https://kustomize.io
[node]: https://kubernetes.io/docs/concepts/architecture/nodes
[reduce]: /docs/reference/configuration/transforms/reduce
[sinks]: /docs/reference/configuration/sinks
[sources]: /docs/reference/configuration/sources
[transforms]: /docs/reference/configuration/transforms
[Aggregator]: /docs/setup/deployment/roles/#aggregator
[transform]: /docs/reference/configuration/transforms/
