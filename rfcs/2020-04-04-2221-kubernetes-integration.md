# RFC 2221 - 2020-04-04 - Kubernetes Integration

This RFC outlines how the Vector will integration with Kubernetes (k8s).

**Note: This RFC is retroactive and meant to seve as an audit to complete our
Kubernetes integration. At the time of writing this RFC, Vector has already made
considerable progress on it's Kubernetes integration. It has a `kubernetes`
source, `kubernetes_pod_metadata` transform, an example daemonset file, and the
ability automatically reload configuration when it changes. The fundamental
pieces are mostly in place to complete this integration, but as we approach
the finish line we're being faced with deeper questions that heavily affect the
UX. Such as how to properly deploy Vector and exclude it's own logs ([pr#2188]).
We had planned to perform a 3rd party audit on the integration before
announcement and we've decided to align this RFC with that process.**

## Motivation

Kubernetes is arguably the most popular container orchestration framework at
the time of writing this RFC; many large companies, with large production
deployments, depend heavily on Kubernetes. Kubernetes handles log collection
but does not facilitate shipping. Shipping is meant to be delegated to tools
like Vector. This is precisely the use case that Vector was built for. So,
the motivation is three-fold:

1. A Kubernetes integration is essential to achieving Vector's vision of being
   the dominant, single collector for observability data.
2. This will inherently attract large, valuable users to Vector since Kubernetes
   is generally used with large deployments.
3. It is currently the #1 requested feature of Vector.

## Guide-level Proposal

**Note: This guide largely follows the format of our existing guides
([example][guide_example]). There are two perspectives to our guides: 1) A new
user coming from Google 2) A user that is familar with Vector. This guide is
from perspective 2.**

This guide covers integrating Vector with Kubernetes. We'll touch on the basic
concepts of deploying Vector into Kubernetes and walk through our recommended
[strategy](#strategy). By the end of this guide you'll have a single,
lightweight, ultra-fast, and reliable data collector ready to ship your
Kubernetes logs and metrics to any destination you please.

### Strategy

#### How This Guide Works

Our recommended strategy deploys Vector as a Kubernetes [daemonset]. This is
the most efficient means of collecting Kubernetes observability data since
Vector is guaranteed to deploy _once_ on each of your Pods. In addition,
we'll use the [`kubernetes_pod_metadata` transform][kubernetes_pod_metadata_transform]
to enrich your logs with Kubernetes context. This transform interacts with
the Kubernetes watch API to collect cluster metadata and update in real-time
when things change. The following diagram demonstrates how this works:

TODO: insert diagram

### What We'll Accomplish

* Collect data from each of your Kubernetes Pods
  * Ability to filter by container name, Pod IDs, and namespaces.
  * Automatically merge logs that Kubernetes splits.
  * Enrich your logs with useful Kubernetes context.
* Send your logs to one or more destinations.

### Tutorial

#### Kubectl Interface

1.  Configure Vector:

    Before we can deplo Vector we must configure. This is done by creating
    a Kubernetes `ConfigMap`:

    ...insert selector to select any of Vector's sinks...

    ```bash
    echo '
    apiVersion: v1
    kind: ConfigMap
    metadata:
      name: vector-config
      namespace: logging
      labels:
        k8s-app: vector
    data:
      vector.toml: |
        # Docs: https://vector.dev/docs/

        # Set global options
        data_dir = "/var/tmp/vector"

        # Ingest logs from Kubernetes
        [sources.kubernetes]
          type = "kubernetes"

        # Enrich logs with Pod metadata
        [transforms.pod_metadata]
          type = "kubernetes_pod_metadata"
          inputs = ["kubernetes"]

        # Send data to one or more sinks!
        [sinks.aws_s3]
          type = "aws_s3"
          inputs = ["pod_metadata"]
          bucket = "my-bucket"
          compression = "gzip"
          region = "us-east-1"
          key_prefix = "date=%F/"
    ' > vector-configmap.toml
    ```

2.  Deploy Vector!

    Now that you have your custom `ConfigMap` ready it's time to deploy
    Vector. To ensure Vector is isolated and has the necessary permissions
    we must create a `namespace`, `ServiceAccount`, `ClusterRole`, and
    `ClusterRoleBinding`:

    ```bash
    kubectl create namespace logging
    kubectl create -f vector-service-account.yaml
    kubectl create -f vector-role.yaml
    kubectl create -f vector-role-binding.yaml
    kubectl create -f vector-configmap.yaml
    kubectl create -f vector-daemonset.yaml
    ```

    * *See [outstanding questions 3, 4, 5, 6, and 7](#outstanding-questions).*

    That's it!

#### Helm Interface

TODO: fill in

## Prior Art

1. [Filebeat k8s integration]
1. [Fluentbit k8s integration]
2. [Fluentd k8s integration]
3. [LogDNA k8s integration]
4. [Honeycomb integration]
3. [Bonzai logging operator] - This is approach is likely outside of the scope
   of Vector's initial Kubernetes integration because it focuses more on
   deployment strategies and topologies. There are likely some very useful
   and interesting tactics in their approach though.
4. [Influx Helm charts]

## Sales Pitch

See [motivation](#motivation).

## Drawbacks

1. Increases the surface area that our team must manage.

## Alternatives

1. Not do this integration and rely solely on external community driven
   integrations.

## Outstanding Questions

1. What is the minimal Kubernetes version that we want to support. See
   [this comment][kubernetes_version_comment].
1. What is the best to avoid Vector from ingesting it's own logs? I'm assuming
   that my [`kubectl` tutoria](#kubectl-interface) handles this with namespaces?
   We'd just need to configure Vector to excluse this namespace?
1. I've seen two different installation strategies. For example, Fluentd offers
   a [single daemonset configuration file][fluentd_daemonset] while Fluentbit
   offers [four separate configuration files][fluentbit_installation]
   (`service-account.yaml`, `role.yaml`, `role-binding.yaml`, `configmap.yaml`).
   Which approach is better? Why are they different?
1. Should we prefer `kubectl create ...` or `kubectl apply ...`? The examples
   in the  [prior art](#prior-art) section use both.
1. From what I understand, Vector requires the Kubernetes `watch` verb in order
   to receive updates to k8s cluster changes. This is required for the
   `kubernetes_pod_metadata` transform. Yet, Fluentbit [requires the `get`,
   `list`, and `watch` verbs][fluentbit_role]. Why don't we require the same?
1. What is `updateStrategy` ... `RollingUpdate`? This is not included in
   [our daemonset][vector_daemonset] or in [any of Fluentbit's config
   files][fluentbit_installation]. But it is included in both [Fluentd's
   daemonset][fluentd_daemonset] and [LogDNA's daemonset][logdna_daemonset].
1. I've also noticed `resources` declarations in some of these config files.
   For example [LogDNA's daemonset][logdna_daemonset]. I assume this is limiting
   resources. Do we want to consider this?
1. What the hell is going on with [Honeycomb's integration
   strategy][Hoenycomb integration]? :) It seems like the whole "Heapster"
   pipeline is specifically for system events, but Heapster is deprecated?
   This leads me to my next question...
1. How are we collecting Kubernetes system events? Is that outside of the
   scope of this RFC? And why does this take an entirely different path?
   (ref [issue#1293])
1. What are some of the details that sets Vector's Kubernetes integration apart?
   This is for marketing purposes and also helps us "raise the bar".

## Plan Of Attack

- [ ] Setup a proper testing suite for k8s.
      - [ ] Support for customizable k8s clusters. See [issue#2170].
      - [ ] Stabilize k8s integration tests. See [isue#2193], [issue#2216],
            and [issue#1635].
      - [ ] Ensure we are testing all supported minor versions. See
            [issue#2223].
- [ ] Audit and improve the `kubernetes` source.
      - [ ] Handle the log recursion problem where Vector ingests it's own logs.
            See [issue#2218] and [issue#2171].
      - [ ] Audit the `file` source strategy. See [issue#2199] and [issue#1910].
      - [ ] Merge split logs. See [pr#2134].
- [ ] Audit and improve the `kubernetes_pod_matadata` transform.
      - [ ] Use the `log_schema.kubernetes_key` setting. See [issue#1867].
- [ ] Ensure our config reload strategy is solid.
      - [ ] Don't exit when there are configuration errors. See [issue#1816].
      - [ ] Test this. See [issue#2224].
- [ ] Add `kubernetes` source reference documentation.
- [ ] Add Kubernetes setup/integration guide.
- [ ] Release `0.10.0` and announce.

[Bonzai logging operator]: https://github.com/banzaicloud/logging-operator
[daemonset]: https://kubernetes.io/docs/concepts/workloads/controllers/daemonset/
[Filebeat k8s integration]: https://www.elastic.co/guide/en/beats/filebeat/master/running-on-kubernetes.html
[Fluentbit k8s integration]: https://docs.fluentbit.io/manual/installation/kubernetes
[fluentbit_daemonset]: https://raw.githubusercontent.com/fluent/fluent-bit-kubernetes-logging/master/output/elasticsearch/fluent-bit-ds.yaml
[fluentbit_installation]: https://docs.fluentbit.io/manual/installation/kubernetes#installation
[fluentbit_role]: https://raw.githubusercontent.com/fluent/fluent-bit-kubernetes-logging/master/fluent-bit-role.yaml
[Fluentd k8s integration]: https://docs.fluentd.org/v/0.12/articles/kubernetes-fluentd
[fluentd_daemonset]: https://github.com/fluent/fluentd-kubernetes-daemonset/blob/master/fluentd-daemonset-papertrail.yaml
[guide_example]: https://vector.dev/guides/integrate/sources/syslog/aws_kinesis_firehose/
[Honeycomb integration]: https://docs.honeycomb.io/getting-data-in/integrations/kubernetes/
[Influx Helm charts]: https://github.com/influxdata/helm-charts
[issue#1293]: https://github.com/timberio/vector/issues/1293
[issue#1635]: https://github.com/timberio/vector/issues/1635
[issue#1816]: https://github.com/timberio/vector/issues/1867
[issue#1867]: https://github.com/timberio/vector/issues/1867
[issue#1910]: https://github.com/timberio/vector/issues/1910
[issue#2170]: https://github.com/timberio/vector/issues/2170
[issue#2171]: https://github.com/timberio/vector/issues/2171
[issue#2199]: https://github.com/timberio/vector/issues/2199
[issue#2216]: https://github.com/timberio/vector/issues/2216
[issue#2218]: https://github.com/timberio/vector/issues/2218
[issue#2223]: https://github.com/timberio/vector/issues/2223
[issue#2224]: https://github.com/timberio/vector/issues/2224
[kubernetes_version_comment]: https://github.com/timberio/vector/pull/2188#discussion_r403120481
[LogDNA k8s integration]: https://docs.logdna.com/docs/kubernetes
[logdna_daemonset]: https://raw.githubusercontent.com/logdna/logdna-agent/master/logdna-agent-ds.yaml
[pr#2134]: https://github.com/timberio/vector/pull/2134
[pr#2188]: https://github.com/timberio/vector/pull/2188
[vector_daemonset]: 2020-04-04-2221-kubernetes-integration/vector-daemonset.yaml
