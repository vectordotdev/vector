# RFC 2221 - 2020-04-04 - Kubernetes Integration

This RFC outlines how the Vector will integration with Kubernetes (k8s).

**Note: This RFC is retroactive and meant to serve as an audit to complete our
Kubernetes integration. At the time of writing this RFC, Vector has already made
considerable progress on it's Kubernetes integration. It has a `kubernetes`
source, `kubernetes_pod_metadata` transform, an example `DaemonSet` file, and the
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
motivation is three-fold:

1. A Kubernetes integration is essential to achieving Vector's vision of being
   the dominant, single collector for observability data.
2. This will inherently attract large, valuable users to Vector since Kubernetes
   is generally used with large deployments.
3. It is currently the #1 requested feature of Vector.

## Guide-level Proposal

**Note: This guide largely follows the format of our existing guides
([example][guide_example]). There are two perspectives to our guides: 1) A new
user coming from Google 2) A user that is familiar with Vector. This guide is
from perspective 2.**

This guide covers integrating Vector with Kubernetes. We'll touch on the basic
concepts of deploying Vector into Kubernetes and walk through our recommended
[strategy](#strategy). By the end of this guide you'll have a single,
lightweight, ultra-fast, and reliable data collector ready to ship your
Kubernetes logs and metrics to any destination you please.

### Strategy

#### How This Guide Works

Our recommended strategy deploys Vector as a Kubernetes [DaemonSet]. This is
the most efficient means of collecting Kubernetes observability data since
Vector is guaranteed to deploy _once_ on each of your Nodes. In addition,
we'll use the [`kubernetes_pod_metadata` transform][kubernetes_pod_metadata_transform]
to enrich your logs with the Kubernetes context. This transform interacts with
the Kubernetes watch API to collect cluster metadata and update in real-time
when things change. The following diagram demonstrates how this works:

TODO: insert diagram

### What We'll Accomplish

- Collect data from each of your Kubernetes Pods
  - Ability to filter by container names, Pod IDs, and namespaces.
  - Automatically merge logs that Kubernetes splits.
  - Enrich your logs with useful Kubernetes context.
- Send your logs to one or more destinations.

### Tutorial

#### Kubectl Interface

1.  Configure Vector:

    Before we can deploy Vector we must configure. This is done by creating
    a Kubernetes `ConfigMap`:

    ...insert selector to select any of Vector's sinks...

    ```bash
    cat <<-CONFIG > vector-configmap.yaml
    apiVersion: v1
    kind: ConfigMap
    metadata:
      name: vector-config
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
    CONFIG
    ```

2.  Deploy Vector!

    Now that you have your custom `ConfigMap` ready it's time to deploy Vector.
    Create a `Namespace` and apply your `ConfigMap` and our recommended
    deployment configuration into it:

    ```shell
    kubectl create namespace vector
    kubectl apply --namespace vector -f vector-configmap.yaml
    kubectl apply --namespace vector -f https://packages.timber.io/vector/latest/kubernetes/vector.yaml
    ```

    - _See [outstanding questions 3, 4, 5, 6, and 7](#outstanding-questions)._

    That's it!

#### Helm Interface

TODO: fill in

## Design considerations

### Minimal supported Kubernetes version

The minimal supported Kubernetes version is the earliest released version of
Kubernetes that we intend to support at full capacity.

We use minimal supported Kubernetes version (or MSKV for short), in the
following ways:

- to communicate to our users what versions of Kubernetes Vector will work on;
- to run our Kubernetes test suite against Kubernetes clusters starting from
  this version;
- to track what Kubernetes API feature level we can use when developing Vector
  code.

We can change MSKV over time, but we have to notify our users accordingly.

There has to be one "root" location where current MSKV for the whole Vector
project is specified, and it should be a single source of truth for all the
decisions that involve MSKV, as well as documentation. A good candidate for
such location is a file at `.meta` dir of the Vector repo. `.meta/mskv` for
instance.

#### Initial Minimal Supported Kubernetes Version

Kubernetes 1.14 introduced some significant improvements to how logs files are
organized, putting more useful metadata into the log file path. This allows us
to implement more high-efficient flexible ways to filter what log files we
consume, which is important for preventing Vector from consuming logs that
it itself produces - which is bad since it can potentially result in an
flood-kind DoS.

We can still offer support for Kubernetes 1.13 and earlier, but it will be
limiting our high-efficient filtering capabilities significantly. It will
also increase the maintenance costs and code complexity.

On the other hand, Kubernetes pre-1.14 versions are quite rare these days.
At the time of writing, the latest Kubernetes version is 1.18, and, according
to the [Kubernetes version and version skew support policy], only versions
1.18, 1.17 and 1.16 are currently maintained.

Considering all of the above, we assign **1.14** as the initial MSKV.

### Helm vs raw YAML files

We consider both raw YAML files and Helm Chart officially supported installation
methods.

With Helm, people usually use the Chart we provide, and tweak it to their needs
via variables we expose as the chart configuration. This means we can offer a
lot of customization, however, in the end, we're in charge of generating the
YAML configuration that will k8s will run from our templates.
This means that, while it is very straightforward for users, we have to keep in
mind the compatibility concerns when we update our Helm Chart.
We should provide a lot of flexibility in our Helm Charts, but also have sane
defaults that would be work for the majority of users.

With raw YAML files, they have to be usable out of the box, but we shouldn't
expect users to use them as-is. People would often maintain their own "forks" of
those, tailored to their use case. We shouldn't overcomplicate our recommended
configuration, but we shouldn't oversimplify it either. It has to be
production-ready. But it also has to be portable, in a sense that it should work
without tweaking with as much cluster setups as possible.
We should support both `kubectl create` and `kubectl apply` flows.

### Reading container logs

#### Kubernetes logging architecture

Kubernetes does not directly control the logging, as the actual implementation
of the logging mechanisms is a domain of the container runtime.
That said, Kubernetes requires container runtime to fulfill a certain contract,
and allowing it to enforce desired behavior.

Kubernetes tries to store logs at consistent filesystem paths for any container
runtime. In particular, `kubelet` is responsible of configuring the container
runtime it controls to put the log at the right place.
Log file format can vary per container runtime, and we have to support all the
formats that Kubernetes itself supports.

Generally, most Kubernetes setups will put the logs at the `kubelet`-configured
locations in a .

There is [official documentation][k8s_log_path_location_docs] at Kubernetes
project regarding logging. I had a misconception that it specifies reading these
log files as an explicitly supported way of consuming the logs, however, I
couldn't find a confirmation of that when I checked.
Nonetheless, Kubernetes log files is a de-facto well-settled interface, that we
should be able to use reliably.

#### File locations

We can read container logs directly from the host filesystem. Kubernetes stores
logs such that they're accessible from the following locations:

- [`/var/log/pods`][k8s_src_var_log_pods];
- `/var/log/containers` - legacy location, kept for backward compatibility
  with pre `1.14` clusters.

To make our lives easier, here's a [link][k8s_src_build_container_logs_directory]
to the part of the k8s source that's responsible for building the path to the
log file. If we encounter issues, this would be a good starting point to unwrap
the k8s code.

#### Log file format

As already been mentioned above, log formats can vary, but there are certain
invariants that are imposed on the container runtimes by the implementation of
Kubernetes itself.

A particularity interesting piece of code is the [`ReadLogs`][k8s_src_read_logs]
function - it is responsible for reading container logs. We should carefully
inspect it to gain knowledge on the edge cases. To achieve the best
compatibility, we can base our log files consumption procedure on the logic
implemented by that function.

Based on the [`parseFuncs`][k8s_src_parse_funcs] (that
[`ReadLogs`][k8s_src_read_logs] uses), it's evident that k8s supports the
following formats:

- Docker [JSON File logging driver] format - which is essentially a simple
  [`JSONLines`][jsonlines] (aka `ndjson`) format;
- [CRI format][cri_log_format].

We have to support both formats.

### Helm Chart Repository

We should not just maintain a Helm Chart, we also should offer Helm repo to make
installations easily upgradable.

Everything we need to do to achieve this is outlined at the
[The Chart Repository Guide].

### Deployment Variants

We have two ways to deploy vector:

- as a [`DaemonSet`][daemonset];
- as a [sidecar `Container`][sidecar_container].

Deploying as a [`DaemonSet`][daemonset] is trivial, applies cluster-wide and
makes sense to as default scenario for the most use cases.

Sidecar container deployments make sense when cluster-wide deployment is not
available. This can generally occur when users are not in control of the whole
cluster (for instance in shared clusters, or in highly isolated clusters).
We should provide recommendations for this deployment variant, however, since
people generally know what they're doing in such use cases, and because those
cases are often very custom, we probably don't have to go deeper than explaining
the generic concerns. We should provide enough flexibility at the Vector code
level for those use cases to be possible.

It is possible to implement a sidecar deployment via implementing an operator
to automatically inject Vector `Container` into `Pod`s (via admission
controller), but that doesn't make a lot of sense for us to work on, since
[`DaemonSet`][daemonset] works for most of use cases already.

## Prior Art

1. [Filebeat k8s integration]
1. [Fluentbit k8s integration]
1. [Fluentd k8s integration]
1. [LogDNA k8s integration]
1. [Honeycomb integration]
1. [Bonzai logging operator] - This is approach is likely outside of the scope
   of Vector's initial Kubernetes integration because it focuses more on
   deployment strategies and topologies. There are likely some very useful
   and interesting tactics in their approach though.
1. [Influx Helm charts]

## Sales Pitch

See [motivation](#motivation).

## Drawbacks

1. Increases the surface area that our team must manage.

## Alternatives

1. Not do this integration and rely solely on external community-driven
   integrations.

## Outstanding Questions

### From Ben

1. What is the minimal Kubernetes version that we want to support. See
   [this comment][kubernetes_version_comment].
1. What is the best to avoid Vector from ingesting it's own logs? I'm assuming
   that my [`kubectl` tutorial](#kubectl-interface) handles this with namespaces?
   We'd just need to configure Vector to exclude this namespace?
1. I've seen two different installation strategies. For example, Fluentd offers
   a [single daemonset configuration file][fluentd_daemonset] while Fluentbit
   offers [four separate configuration files][fluentbit_installation]
   (`service-account.yaml`, `role.yaml`, `role-binding.yaml`, `configmap.yaml`).
   Which approach is better? Why are they different?
1. Should we prefer `kubectl create ...` or `kubectl apply ...`? The examples
   in the [prior art](#prior-art) section use both.
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
   strategy][honeycomb integration]? :) It seems like the whole "Heapster"
   pipeline is specifically for system events, but Heapster is deprecated?
   This leads me to my next question...
1. How are we collecting Kubernetes system events? Is that outside of the
   scope of this RFC? And why does this take an entirely different path?
   (ref [issue#1293])
1. What are some of the details that set Vector's Kubernetes integration apart?
   This is for marketing purposes and also helps us "raise the bar".

### From Mike

1. What significantly different k8s cluster "flavors" are there? Which ones do
   we want to test against? Some clusters use `docker`, some use `CRI-O`,
   [etc][container_runtimes]. Some even use [gVisor] or [Firecracker]. There
   might be differences in how different container runtimes handle logs.
1. How do we want to approach Helm Chart Repository management.
1. How do we implement liveness, readiness and startup probes?
1. Can we populate file at `terminationMessagePath` with some meaningful
   information when we exit or crash?

## Plan Of Attack

- [ ] Agree on minimal Kubernetes version.
- [ ] Agree on a list of Kubernetes cluster flavors we want to test against.
- [ ] Setup a proper testing suite for k8s.
  - [ ] Support for customizable k8s clusters. See [issue#2170].
  - [ ] Look into [issue#2225] and see if we can include it as part of this
        work.
  - [ ] Stabilize k8s integration tests. See [issue#2193], [issue#2216],
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
- [ ] Prepare YAML deployment config.
- [ ] Prepare Heml Chart.
- [ ] Prepare Heml Chart Repository.
- [ ] Integrate kubernetes configuration snapshotting into the release process.
- [ ] Add Kubernetes setup/integration guide.
- [ ] Release `0.10.0` and announce.

[bonzai logging operator]: https://github.com/banzaicloud/logging-operator
[container_runtimes]: https://kubernetes.io/docs/setup/production-environment/container-runtimes/
[cri_log_format]: https://github.com/kubernetes/community/blob/ee2abbf9dbfa4523b414f99a04ddc97bd38c74b2/contributors/design-proposals/node/kubelet-cri-logging.md
[daemonset]: https://kubernetes.io/docs/concepts/workloads/controllers/daemonset/
[filebeat k8s integration]: https://www.elastic.co/guide/en/beats/filebeat/master/running-on-kubernetes.html
[firecracker]: https://github.com/firecracker-microvm/firecracker
[fluentbit k8s integration]: https://docs.fluentbit.io/manual/installation/kubernetes
[fluentbit_daemonset]: https://raw.githubusercontent.com/fluent/fluent-bit-kubernetes-logging/master/output/elasticsearch/fluent-bit-ds.yaml
[fluentbit_installation]: https://docs.fluentbit.io/manual/installation/kubernetes#installation
[fluentbit_role]: https://raw.githubusercontent.com/fluent/fluent-bit-kubernetes-logging/master/fluent-bit-role.yaml
[fluentd k8s integration]: https://docs.fluentd.org/v/0.12/articles/kubernetes-fluentd
[fluentd_daemonset]: https://github.com/fluent/fluentd-kubernetes-daemonset/blob/master/fluentd-daemonset-papertrail.yaml
[guide_example]: https://vector.dev/guides/integrate/sources/syslog/aws_kinesis_firehose/
[gvisor]: https://github.com/google/gvisor
[honeycomb integration]: https://docs.honeycomb.io/getting-data-in/integrations/kubernetes/
[influx helm charts]: https://github.com/influxdata/helm-charts
[issue#1293]: https://github.com/timberio/vector/issues/1293
[issue#1635]: https://github.com/timberio/vector/issues/1635
[issue#1816]: https://github.com/timberio/vector/issues/1867
[issue#1867]: https://github.com/timberio/vector/issues/1867
[issue#1910]: https://github.com/timberio/vector/issues/1910
[issue#2170]: https://github.com/timberio/vector/issues/2170
[issue#2171]: https://github.com/timberio/vector/issues/2171
[issue#2193]: https://github.com/timberio/vector/issues/2193
[issue#2199]: https://github.com/timberio/vector/issues/2199
[issue#2216]: https://github.com/timberio/vector/issues/2216
[issue#2218]: https://github.com/timberio/vector/issues/2218
[issue#2223]: https://github.com/timberio/vector/issues/2223
[issue#2224]: https://github.com/timberio/vector/issues/2224
[issue#2225]: https://github.com/timberio/vector/issues/2225
[json file logging driver]: https://docs.docker.com/config/containers/logging/json-file/
[jsonlines]: http://jsonlines.org/
[k8s_log_path_location_docs]: https://kubernetes.io/docs/concepts/cluster-administration/logging/#logging-at-the-node-level
[k8s_src_build_container_logs_directory]: https://github.com/kubernetes/kubernetes/blob/31305966789525fca49ec26c289e565467d1f1c4/pkg/kubelet/kuberuntime/helpers.go#L173
[k8s_src_parse_funcs]: https://github.com/kubernetes/kubernetes/blob/e74ad388541b15ae7332abf2e586e2637b55d7a7/pkg/kubelet/kuberuntime/logs/logs.go#L116
[k8s_src_read_logs]: https://github.com/kubernetes/kubernetes/blob/e74ad388541b15ae7332abf2e586e2637b55d7a7/pkg/kubelet/kuberuntime/logs/logs.go#L277
[k8s_src_var_log_pods]: https://github.com/kubernetes/kubernetes/blob/58596b2bf5eb0d84128fa04d0395ddd148d96e51/pkg/kubelet/kuberuntime/kuberuntime_manager.go#L60
[kubernetes version and version skew support policy]: https://kubernetes.io/docs/setup/release/version-skew-policy/
[kubernetes_version_comment]: https://github.com/timberio/vector/pull/2188#discussion_r403120481
[logdna k8s integration]: https://docs.logdna.com/docs/kubernetes
[logdna_daemonset]: https://raw.githubusercontent.com/logdna/logdna-agent/master/logdna-agent-ds.yaml
[pr#2134]: https://github.com/timberio/vector/pull/2134
[pr#2188]: https://github.com/timberio/vector/pull/2188
[sidecar_container]: https://github.com/kubernetes/enhancements/blob/a8262db2ce38b2ec7941bdb6810a8d81c5141447/keps/sig-apps/sidecarcontainers.md
[the chart repository guide]: https://helm.sh/docs/topics/chart_repository/
[vector_daemonset]: 2020-04-04-2221-kubernetes-integration/vector-daemonset.yaml
