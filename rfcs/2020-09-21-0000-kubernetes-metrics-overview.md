# RFC 0000 - 2020-09-21 - Overview of metrics in Kubernetes

This RFC's goal is to create a systematic overview of metrics in the
Kubernetes ecosystem.

## Table of contents

- [RFC 3684 - 2020-09-21 - Overview of metrics in Kubernetes](#rfc-3684---2020-09-21---overview-of-metrics-in-kubernetes)
  - [Table of contents](#table-of-contents)
  - [Motivation](#motivation)
  - [Overview](#overview)
  - [Metrics exposed by the Kubernetes system components](#metrics-exposed-by-the-kubernetes-system-components)
    - [Properties](#properties)
    - [Per-component insights](#per-component-insights)
  - [TODO](#todo)
  - [Prior art](#prior-art)

## Motivation

Metrics is Kubernetes are a large sector, with many concerns. We need a way to
better understand it, to, sort of, have a list of entry points to explore in
more detail.

This overview will help us reason about the metrics  in Kubernetes and make
educated design and planning decisions.

## Overview

- Application metrics exposed via [OpenMetrics](https://openmetrics.io/) or
  [Prometheus exposition format](https://prometheus.io/docs/instrumenting/exposition_formats/)
  for scraping, aka pull-based gathering.

- Application metrics shipped to a server of some kind, typically via `statsd`
  protocol, aka push-based gathering.

  In the modern metrics aggregation stacks, these are usually converted to
  pull-based gathering via
  [pushgateway](https://github.com/prometheus/pushgateway) or
  [statsd_exporter](https://github.com/prometheus/statsd_exporter) - but
  it's even more common to not have any push-based metrics at all.

- Metrics exposed by the Kubernetes system components via Prometheus exposition
  format.

  Among others, this category includes:

  - `kubelet`
  - `kube-apiserver`
  - `kube-scheduler`
  - `kube-controller-manager`
  - `etcd`
  - `docker`

  More on this [later](#metrics-exposed-by-the-kubernetes-system-components),
  but the key part here is those components are not all deployed together and
  require different access paths if we want to reach their `/metrics` endpoints.

- Metrics available via the [Metrics API] - the `kube-apiserver` API that offers
  system metrics for the purposes of autoscaling to various Kubernetes
  components (typically autoscalers -  i.e. Horizontal Pod Autoscaler,
  Vertical Pod Autoscaler, etc).

  This API essentially contains very limited data on resource usage of the
  `Pod`s and `Node`s.

  Reviewed in more detail at the Metrics API RFC.

- Metrics gathered from the kernel, at the host level (CPU/RAM/IO usage).

  These metrics themselves are pretty much well-known, they are mostly provided
  by the kernel.

  However, there are numerous ways to gather them. Most common these days is
  Prometheus' `node_exporter`, deployed as a dedicated `DaemonSet`.
  Vector too can gether host metrics - however it might also require a dedicated
  `DaemonSet` with special capabilities / cgroups (in addition to our log
  collecting `DaemonSet`) to have proper access to the metrics.

- Metrics derived from the state of the Kubernetes API (i.e. the data at
  the `kube-apiserver`).

  An implementation of this concept would be the best to explanation:
  <https://github.com/kubernetes/kube-state-metrics>

  The metrics derived from this state are in the lines of "how many deployments
  are there" and "how many *healthy* deployments are there".

- Prometheus server providing access to metrics it gathers.

  The presence of a Prometheus server, it being a de-facto standard for metrics
  aggregation at Kubernetes, it is possible to just grab all the metrics that
  it exposes. This enables a tool like Vector to abstract away the notion of
  which metrics in particular are being gathered, but still provide a way to
  organize the metrics shipping pipeline.

  > Aggregating metrics like Prometheus does it is not trivial, and despite it
  > being possible to implement Prometheus-like storage and aggregation in
  > Vector, working with the emerged standard rather than competing with it
  > would be better for the industry as a whole.
  > Especially given that Prometheus works well and provides very good
  > extensibility options, so I don't see technical reasons for competition
  > here.

- Platform metrics.

  These are metrics provided by the platform that Kubernetes cluster itself runs
  atop.

  This can be things like:

  - AWS/GCP/Azure/Other cloud metrics - like VPC, RDS metrics and etc.
  - For baremetal installations - metrics from hardware loadbalancers, external
    power/cooling solutions, hypervisors, etc.

- External metrics.

  Metrics gathered into a metrics aggregation pipeline that run inside of
  Kubernetes from the apps deployed outside of Kubernetes.

  Kind of similar to the platform metrics.

  These are separate category from the in-cluster applications because they have
  different concerns regarding access management.

[Metrics API]: https://github.com/kubernetes/metrics

## Metrics exposed by the Kubernetes system components

While remaining on a fairly high level, this section explore what system
components are there, what data they expose and how do we obtain the said data
for each of them.

Kubernetes is a complicated environment with a number of
[system components](https://kubernetes.io/docs/concepts/overview/components).

Without going into too technical details, we can highlight some properties that
are important for gathering metrics from the components.

> Kubernetes application framework exposes metrics via Prometheus exposition
> format at `/metrics` HTTP endpoint. This is the default way to implement
> metrics in the Kubernetes codebase, so, unless specified otherwise, assume by
> default that everything discussed in this section uses Prometheus format.

### Properties

- Cluster-role scheduling requirements

  - dedicated control-plane nodes only
  - regular worker nodes only
  - any node

- Access restriction to the `/metrics` endpoint

  - no restrictions
  - authorization at the HTTP/HTTPS layer
  - socket listening on `localhost`
  - access is guarded via dedicated cgroup namespace (typical when running in
    a `Pod` and when considering node-local access)
  - access is guarded via firewall
  - no `Endpoint` to the HTTP server

### Per-component insights

- `kube-apiserver`

  The very core of the Kubernetes.

  The easiest way to get a peek on the exposed metrics is via `kubectl`:

  ```shell
  $ kubectl get --raw /metrics
  # HELP aggregator_openapi_v2_regeneration_count [ALPHA] Counter of OpenAPI v2 spec regeneration count broken down by causing APIService name and reason.
  # TYPE aggregator_openapi_v2_regeneration_count counter
  ... metrics ...
  ```

  This way of accessing the metrics is suitable if you want to learn about the
  metrics available, however, this approach - accessing the `/metrics` endpoint
  via the `Service` - is not suitable for production-level monitoring.
  The reason is, this way we're only sending the request to and getting the
  response from a single instance of the `kube-apiserver`. In the HA deployments
  multiple `kube-apiserver`s will be running and working together, and we need
  to collect metrics from each of them.

  Being part of the `kube-apiserver` API, the `/metrics` endpoint requires
  Kubernetes API authorization to access.

- `kubelet`

  This component runs on every cluster node, and is responsible of driving the
  container runtime to keep `Pod` containers running on the node.

  It exposes it's own metrics (it's healthiness, Go GC data and etc), as well
  as metrics gathered by built-in `cadvisor`, resource metrics and probes.

  `cadvisor` is tool that gathers common container-level metrics - like
  per-container CPU/RAM/IO usage - and exposes them in a Prometheus metrics
  format. It's endpoint is exposed at `kubelet` HTTP server at a different path:
  `/metrics/cadvisor` (in addition to the standard `/metrics` path for
  `kubelet`'s own metrics).

  Resource metrics - a concise info on container and node CPU and RAM usage -
  are available under `/metrics/resource`.

  Probes metrics - derived from, liveness, readiness or startup probes per
  container - are under `/metrics/probes`.

  These metrics can be accessed by issuing a sequence of the following commands:

  1. Start a proxy to the `kube-apiserver`.

    ```shell
    $ kubectl proxy &
    Starting to serve on 127.0.0.1:8001
    ```

  1. Consult the list of the available cluster nodes.

    ```shell
    $ kubectl get nodes
    NAME       STATUS   ROLES    AGE   VERSION
    minikube   Ready    master   25h   v1.19.2
    ```

  1. Load the metrics.

    `kubelet` HTTP server root is available under this URL:
    `http://localhost:8001/api/v1/nodes/<node>:10250/proxy/`, where `<node>` is
    the name of your node, and the `10250` is the port that `kubelet` listens on
    at that node.

    > Note: port `10250` is currently the default for a standard
    > `kubeadm`-managed deployment.

    For instance, suppose our node name is `minikube` and the `kubelet` port is
    `10250`.

    To load `kubelet`'s own metrics use:

    ```shell
    $ curl http://localhost:8001/api/v1/nodes/minikube:10250/proxy/metrics
    # HELP apiserver_audit_event_total [ALPHA] Counter of audit events generated and sent to the audit backend.
    # TYPE apiserver_audit_event_total counter
    ... metrics ...
    ```

    To load `cadvisor` metrics use:

    ```shell
    $ curl http://localhost:8001/api/v1/nodes/minikube:10250/proxy/metrics/cadvisor
    # HELP cadvisor_version_info A metric with a constant '1' value labeled by kernel version, OS version, docker version, cadvisor version & cadvisor revision.
    # TYPE cadvisor_version_info gauge
    ... metrics ...
    ```

    And etc.

    This illustrates that `kubelet` metrics for any node can be obtained via the
    `kube-apiserver`.

    They can also be accessed locally (i.e. from a `DaemonSet`), but the
    authorization is required.

    More info on `kubelet` authentication/authorization is available
    [here](https://kubernetes.io/docs/reference/command-line-tools-reference/kubelet-authentication-authorization/).

## TODO

- components:
  - `kube-scheduler`
  - `kube-controller-manager`
  - `kube-proxy`
  - `etcd`
  - `docker` (and other container runtimes)
- storage monitoring
- network monitoring

## Prior art

- <https://coreos.com/operators/prometheus/docs/latest/user-guides/cluster-monitoring.html>
- <https://sysdig.com/blog/how-to-monitor-kubelet/>
- <https://github.com/prometheus/prometheus/blob/master/documentation/examples/prometheus-kubernetes.yml>
- <https://help.sumologic.com/Metrics/Kubernetes_Metrics>
