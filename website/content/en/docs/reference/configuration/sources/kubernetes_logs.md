---
title: Kubernetes logs
description: Collect logs from [Kubernetes](https://kubernetes.io) Nodes
kind: source
---

The `kubernetes_logs` source enriches log data with Kubernetes metadata via the Kubernetes API.

## Setup

The `kubernetes_logs` source is part of a larger setup strategy for the Kubernetes platform.

{{< jump "/docs/setup/installation/platforms/kubernetes" >}}

## Configuration

{{< component/config >}}

## Output

{{< component/output >}}

## Telemetry

{{< component/telemetry >}}

## Examples

{{< component/examples >}}

## How it works

### Checkpointing

{{< snippet "checkpointing" >}}

### Container exclusion

The `kubernetes_logs` source can skip the logs from the individual [`container`s](#container) of a particular [Pod]. Add an `annotation vector.dev/exclude-containers` to the Pod and enumerate the `name`s of all the containers to exclude in the value of the annotation, like so:

```yaml
vector.dev/exclude-containers: "container1,container2"
```

This annotation makes Vector skip logs originating from `container1` and `container2` of the Pod marked with the annotation, while logs from other [`container`s](#container) in the Pod are still collected.

### Context

{{< snippet "context" >}}

### Enrichment

Vector enriches data with Kubernetes context. You can find a comprehensive list of fields in the [Output](#output) section above.

### Filtering

Vector provides rich filtering options for Kubernetes log collection:

* Built-in [Pod](#pod-exclusion) and [container](#container-exclusion) exclusion rules.
* The [`exclude_paths_glob_patterns`](#exclude_path_glob_patterns) option enables you to exclude Kubernetes log files by the filename and filepath.
* The [`extra_field_selector`](#extra_field_selector) option specifies the field selector to filter Pods with, to be used in addition to the built-in Node filter.
* The [`extra_label_selector`](#extra_label_selector) option specifies the label selector to filter Pods with, to be used in addition to the built-in [`vector.dev/exclude`](#pod-exclusion) filter.

### Kubernetes API access control

Vector requires access to the Kubernetes API. Specifically, the `kubernetes_logs` source uses the `/api/v1/pods` endpoint to "watch" Pods from all namespaces.

Modern Kubernetes clusters run with RBAC (role-based access control) scheme. RBAC-enabled clusters require some configuration to grant Vector the authorization to access the Kubernetes API endpoints. As RBAC is currently the standard way of controlling access to the Kubernetes API, we ship the necessary configuration out of the box: see `ClusterRole`, `ClusterRoleBinding` and `ServiceAccount` in our kubectl YAML config, and the `rbac` configuration in the Helm chart.

If your cluster doesn't use any access control scheme and doesn't restrict access to the Kubernetes API, you don't need to do any extra configuration. Vector should work fine without any such scheme.

Clusters using a legacy ABAC scheme aren't officially supported, although Vector *may* work if you configure access properly. We encourage you to switch to RBAC. If you use a custom access control scheme, make sure tgat Vector Pod/`ServiceAccount` is granted access to the `/api/v1/pods` resource.

### Kubernetes API communication

Vector communicates with the Kubernetes API to enrich the data it collects with Kubernetes context. Therefore, Vector must have access to communicate with the [Kubernetes API server][kubernetes_api]. If Vector is running in a Kubernetes cluster, it connects to that cluster using the [Kubernetes-provided access information][kubernetes_api_access].

In addition to access, Vector implements proper desync handling to ensure that communication is safe and reliable. This ensures that Vector doesn't overwhelm the Kubernetes API or compromise its stability.

### Partial message merging

By default, Vector merges partial messages that are split due to Docker's size limit. For everything else, we recommend using the [`reduce` transform][reduce], which enables you to handle custom merging of things like stacktraces.

### Pod exclusion

By default, the `kubernetes_logs` source skils logs from the [Pods][pod] that have a `vector.dev/exclude: "true"` label. You can configure additional exclusion rules via label or field selectors. See the [available options](#configuration).

### Pod removal

To ensure that all data is collected, Vector continues to collect logs from the [Pod] for some time after its removal. This ensures that Vector obtains some of the most important data, such as crash details.

### Resource limits

We recommend the following resource limits for Vector:

#### Agent resource limits

If you deploy Vector as an agent (collecting data for each of your Nodes), we recommend these limits:

```yaml
resources:
  requests:
    memory: "64Mi"
    cpu: "500m"
  limits:
    memory: "1024Mi"
    cpu: "6000m"
```

{{< warning >}}
As with all Kubernetes resource limit recommendations, use these as a reference apoint and adjust as necessary. If your configured Vector pipeline is complex you may need more resources; if your pipeline is less complex you may need less.
{{< /warning >}}

### State

{{< snippet "stateless" >}}

### State management

#### Agent state management

For the agent role, Vector stores its state at the host-mapped directory with a static path, so if it's redeployed it continues from where it was interrupted.

### Testing and reliability

Vector is tested intensively in conjunction with Kubernetes. In addition to Kubernetes being Vector's most popular installation method, Vector implements a comprehensive end-to-end test suite for all minor Kubernetes versions beginning with Kubernetes version 1.14.


[kubernetes_api]: https://kubernetes.io/docs/reference/command-line-tools-reference/kube-apiserver
[kubernetes_api_access]: https://kubernetes.io/docs/tasks/access-application-cluster/access-cluster/#accessing-the-api-from-a-pod
[pod]: https://kubernetes.io/docs/concepts/workloads/pods
[reduce]: /docs/reference/configuration/transforms/reduce
