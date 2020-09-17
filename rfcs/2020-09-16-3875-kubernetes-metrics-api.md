# RFC 3875 - 2020-09-16 - Kubernetes Metrics API

This RFC explores ways to integrate with Kubernetes Metrics API.

## Scope

This RFC will cover:

- what's Metrics API, how it's defined, how it's deployed, how to access it;
- how should we integrate with Metrics API, and should we do it in the first
  place;

This RFC will not cover:

- other ways to collect metrics in the Kubernetes environment:
  - directly gathering metrics off of the system components (`kubelet`,
    `kube-apiserver`, `kube-scheduler`, `kube-controller`, `docker`, etc) by
    scraping their prometheus-format `/metrics` endpoints;
  - fetching metrics from Prometheus server deployed for "Full metrics
    pipeline" - Prometheus can natively monitor Kubernetes, nodes, and itself,
    and we could just grab all the metrics from it;
  - gathering metrics over the living Kubernetes API Objects - not resource
    usage, but arbitrary metrics - see
    <https://github.com/kubernetes/kube-state-metrics>;
- Vendor-specific Metrics API implementations - we focus on Resource Metrics
  API for now;
- Implementing the Metrics API itself in Vector.

## Introduction

Here's a quick summary of the situation:

- Kubernetes has a "native" [Metrics API](https://github.com/kubernetes/metrics) -
  an API that's exposed via the same `kube-apiserver` that handles the rest
  of the core Kubernetes APIs.
  Metrics API _defines_ the API, but doesn't provide the _implementation_
  of the said API.
- _One of the implementations_ of this API is the
  [Metrics Server](https://github.com/kubernetes-sigs/metrics-server) -
  an independent system-level component of the Kubernetes ecosystem.
  It has to be deployed separately into Kubernetes clusters, and without it,
  Metrics API doesn't contain any data (in fact, technically it
  [won't be available](https://kubernetes.io/docs/concepts/extend-kubernetes/api-extension/apiserver-aggregation/#aggregation-layer)
  at all).
  The job of this component is to gather the
  [**resource usage metrics**](https://kubernetes.io/docs/tasks/debug-application-cluster/resource-metrics-pipeline/#measuring-resource-usage)
  from `kubelet`s, which, in turn, collect them from the underlying
  [container runtime](https://github.com/kubernetes/community/blob/master/contributors/devel/sig-node/cri-container-stats.md).
- Kubernetes makes uses of the data available via it's Metrics API itself -
  in [Horizontal Pod Autoscaler](https://kubernetes.io/docs/tasks/run-application/horizontal-pod-autoscale)
  (automatically scales the number of `Pod`s in a replication controller,
  deployment, replica set or stateful set based on observed CPU utilization)
  and [Vertical Pod Autoscaler](https://github.com/kubernetes/autoscaler/tree/master/vertical-pod-autoscaler)
  (automatically adjusts the resource `limits` and `requests` for the containers
  in their `Pod`s based on usage and thus allow proper scheduling onto nodes so
  that appropriate resource amount is available for each `Pod`).
  This means that this is _the_ data used as input for the actual cluster
  autoscaling mechanisms, and it is very well worth to for end users to see it.

## Metrics API

So, what is _Metrics API_, _Resource Usage Metrics_ and how do they look like?

_Metrics API_ is a project that provides type definitions for metrics APIs that
Kubernetes makes use of.

It doesn't provide implementations though. In essence, all is it is a shared
schema (or parts of it) that consumers can consume, and implementers implement.

Physically, it's a [git repo](https://github.com/kubernetes/metrics) with
a bunch of Go code. It provides the Go code with the API definitions and
client code (also in Go) to consume those metrics.

_Metrics API_ defines the following APIs:

- [Resource Metrics API](https://github.com/kubernetes/metrics#resource-metrics-api)

  - Defined [here](https://github.com/kubernetes/metrics/blob/bd98ade7905d9e394dfa9fcf1eee4e2a3966e9a9/pkg/apis/metrics/v1beta1/types.go).
  - Design [here](https://github.com/kubernetes/community/blob/master/contributors/design-proposals/instrumentation/resource-metrics-api.md).

  This is the API that Kubernetes itself relies on in it's autoscaling components.

  The API defines both the shape of the data (requests and responses) and the
  the endpoints (methods and paths). Needless to say, like any `kube-apiserver`
  API it's an HTTP API.

  This API allows consumers to access resource metrics (CPU and memory) for pods
  and nodes.

  In essence, the payload we can expect is
  [this object](https://pkg.go.dev/k8s.io/api/core/v1#ResourceList),
  nested in various ways in the
  [API resource root objects](https://pkg.go.dev/k8s.io/metrics/pkg/apis/metrics).

  It can be further subdivided into the following groups by the resource kinds the operation is conducted:

  - `NodeMetrics`

    Available operations:

    - `get` - obtain a single `NodeMetrics` object by the `Node` name;
    - `list` - obtain a `NodeMetricsList`, with a `NodeMetrics` per each `Node`
      in the cluster;
    - `watch` - stream the updates to `NodeMetrics` via the standard k8s watch
      mechanism; like `list`, but streams.

    See the
    [generated client](https://github.com/kubernetes/metrics/blob/bd98ade7905d9e394dfa9fcf1eee4e2a3966e9a9/pkg/client/clientset/versioned/typed/metrics/v1beta1/nodemetrics.go).

  - `PodMetrics`

    Available operations:

    - `get` - obtain a single `PodMetrics` object by the namespace and the `Pod`
      name;
    - `list` - obtain a `PodMetricsList`, with a `PodMetrics` per each `Pod`
      in the specified namespace;
    - `watch` - stream the updates to `PodMetrics` in a specified namespace via
      the standard k8s watch mechanism; like `list`, but streams.

    See the [generated client](https://github.com/kubernetes/metrics/blob/bd98ade7905d9e394dfa9fcf1eee4e2a3966e9a9/pkg/client/clientset/versioned/typed/metrics/v1beta1/podmetrics.go).

  This API is implemented by the [Metrics Server](https://github.com/kubernetes-sigs/metrics-server).

  This API or it's data is referred to as Resource Usage Metrics.

  For more information, see:

  - [Metrics API design](https://github.com/kubernetes/community/blob/master/contributors/design-proposals/instrumentation/resource-metrics-api.md)
  - [Metrics Server design](https://github.com/kubernetes/community/blob/master/contributors/design-proposals/instrumentation/metrics-server.md)

- [Custom Metrics API](https://github.com/kubernetes/metrics#custom-metrics-api) and External Metrics API

  These APIs are more generic, and are not relied upon by the Kubernetes components.

  - Custom Metrics API

    - Defined [here](https://github.com/kubernetes/metrics/blob/bd98ade7905d9e394dfa9fcf1eee4e2a3966e9a9/pkg/apis/custom_metrics/v1beta2/types.go).
    - Design [here](https://github.com/kubernetes/community/blob/master/contributors/design-proposals/instrumentation/custom-metrics-api.md).

    The API specifies the shape of the objects suitable to handle a generic
    metric value on an arbitrary Kubernetes API object.
    It's all meta and such, but essentially the types defined are
    `MetricIdentifier`, `MetricListOptions`, `MetricValue` and
    `MetricValueList`.
    You get the idea - generic ways to describe generic metrics about something.

    We can, for root-scoped and namespaced objects, do:

    - `GetForObject` - obtain a `MetricValue` by a Kubernetes API Object
      group and name, and a metric name and selector;
    - `GetForObjects` - obtain a `MetricValueList` by a Kubernetes API Object
      group and selector, and a metric name and selector.

    See the
    [generated client](https://github.com/kubernetes/metrics/blob/bd98ade7905d9e394dfa9fcf1eee4e2a3966e9a9/pkg/client/custom_metrics/versioned_client.go)

  - External Metrics API

    - Defined [here](https://github.com/kubernetes/metrics/blob/bd98ade7905d9e394dfa9fcf1eee4e2a3966e9a9/pkg/apis/external_metrics/v1beta1/types.go).
    - Design [here](https://github.com/kubernetes/community/blob/master/contributors/design-proposals/instrumentation/external-metrics-api.md).

    This API defines just two types - `ExternalMetricValue` and
    `ExternalMetricValueList`.

    These are just metrics, identified by the name and and a list of labels
    (kv pairs of string) - no relation to the Kubernetes API resource like with
    at Custom Metrics API.

    All we can do with this API is get an `ExternalMetricValueList` by the
    namespace, metric name and metrics selector.

  The implementation of these are typically vendor-specific, and expose
  information about the cloud that Kubernetes runs atop and/or surrounding
  infrastructure.

Check out a list of
[implementations](https://github.com/kubernetes/metrics/blob/bd98ade7905d9e394dfa9fcf1eee4e2a3966e9a9/IMPLEMENTATIONS.md)
of the Metrics API to better understand the idea on how it all works.

> We're currently focusing on integrating with just the Resource Metrics API but
> it's important to understand the distinction.

## Should we use it

Overall, it looks like Metrics API is designed to allow external monitoring
tools to route data in such a way that it's available in a "native" fashion
to the Horizontal Pod Autoscaler and other Kubernetes components that rely on
this data. This means that while we _can_ integrate with the Metrics API
directly, it might not be the best way, and going with an alternative - like
integrating with those external metrics aggregation system via the Prometheus
scraping protocol- might be a better approach.

One benefit to integrating with the Metrics API is the consumed data are always
the same shape and form, and are therefore portable and independent on the
underlying monitoring solution. They are dependent of the Kubernetes API though,
which is arguable harder to maintain than a Prometheus scraping format.

There's a note in the `Metrics Server` `README.md`:

> Metrics Server is not meant for non-autoscaling purposes.
> For example, don't use it to forward metrics to monitoring solutions, or as a
> source of monitoring solution metrics.

I asked people from the `sig-instrumentation` to comment on this.

The reason why they don't want monitoring tools to use Metrics API is that the
reported metrics are not considered accurate. However, we are totally fine using
it if whe treat this data at _inputs to the autoscalers_ - the difference being
that we don't assume those to be accurate metrics - just merely some values that
Kubernetes autoscalers see and act upon. They can still be very useful to our
users to better understand how autoscalers are behaving and how their cluster
estimates it's capacity, however they can't be considered accurate metrics on
the resources. If we implement this, it important that we communicate this
semantic aspect to the end users.

Source: <https://kubernetes.slack.com/archives/C20HH14P7/p1600368737067500?thread_ts=1600347316.062400&cid=C20HH14P7>

### The verdict

Given that the data is not accurate, and the implementation is quite challenging
(see <https://github.com/Arnavion/k8s-openapi/issues/76> - there are some
technical difficulties with support for client generation - nothing impossible,
but some effort is needed) and the underlying data is likely available via other
means (for instance - via Prometheus endpoints of that same Metrics Server) -
it would probably be more effective to focus on other ways to ingest metrics
from the Kubernetes ecosystem now.

We can and come back to this later. Signs that we want to revisit this would be
voiced user demand for this feature or the necessity to monitor specifically
the autoscalers inputs.

It is still not clear if this data is easily available from elsewhere - but if
it is, it's likely accessible via Prometheus scraping protocol.

## How to use it

If we were to integrate with the Metrics API, the most straightforward way would
be to add the `kubernetes_resource_usage_metrics_nodes`,
`kubernetes_resource_usage_metrics_pods`, `kubernetes_custom_metrics` and
`kubernetes_external_metrics` sources.

Why not just a single source with all three metrics at once? The main reason is
that user might want to have multiple instances of these sources with different
settings.
Each source would accept options according to the capabilities of the respective
API, and run exactly one watch-stream internally.
Another reason is that the data format of the all the listed sources is
different, and it would be (potentially) easier to handle if they were separate
inputs for the topology.

To make the config more compact, we could offer
a `kubernetes_resource_usage_metrics` macro that would unfold into
`kubernetes_resource_usage_metrics_nodes` and
`kubernetes_resource_usage_metrics_pods`.

## We can implement Metrics API too

That's right, we can implement the Metrics API and server Kubernetes as a source
of data for the autoscaling workloads.

However, it's a complicated topic, and it's worth its own focused RFC.
