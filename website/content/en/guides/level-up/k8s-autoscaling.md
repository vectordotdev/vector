---
date: "2026-07-01"
title: Load balancing and scaling Vector on Kubernetes
short: K8s autoscaling
description: Learn how to take a CPU-bound Vector deployment beyond one pod on Kubernetes by measuring capacity, distributing HTTP traffic, and configuring autoscaling.
authors: ["thomasqueirozb"]
domain: platforms
weight: 7
tags: ["level up", "guides", "guide", "kubernetes", "load balancing", "nginx"]
---

This guide is for engineers who already run a Vector pipeline on Kubernetes
and need to scale a CPU-bound deployment beyond one pod. It walks through how
to measure capacity, make additional replicas useful, and automate replica
counts with the Kubernetes [Horizontal Pod Autoscaler (HPA)](https://kubernetes.io/docs/tasks/run-application/horizontal-pod-autoscale/).

We start by measuring a single pod and testing manual scaling to verify that
additional pods increase throughput before handing replica management to the HPA.

{{< requirement >}}
This guide assumes you understand Vector sources, transforms, and sinks, and
are comfortable with Kubernetes Deployments, Services, CPU requests and
limits, and Helm releases. You do not need prior experience configuring an HPA.

If you are new to Vector or Kubernetes, start with the
[Vector quickstart](/docs/setup/quickstart/) and the
[Kubernetes installation guide](/docs/setup/installation/platforms/kubernetes/).
This guide builds on a working deployment rather than teaching those basics.
{{< /requirement >}}

## What you'll learn

By the end of the walkthrough, you should be able to:

1. **Measure a single pod's CPU-bound capacity** and use that baseline to
   estimate replica requirements ([Manual scaling](#manual-scaling)).
2. **Distribute HTTP requests to new replicas** with L7 load balancing, then
   verify that manual scaling improves throughput ([Manual scaling](#manual-scaling)).
3. **Configure CPU-based autoscaling with headroom** and interpret the replica
   count it settles on ([Automatic scaling with HPA](#automatic-scaling-with-hpa)).
4. **Recognize when autoscaling cannot protect against event loss** and choose
   how to handle load while pods start ([Handling sudden bursts](#handling-sudden-bursts)).

The experiment uses a stateless pipeline parsing
[Apache Common Log Format](https://httpd.apache.org/docs/current/logs.html#common)
data over HTTP behind the [NGINX](https://www.nginx.com/) Ingress Controller.
It isolates Vector CPU capacity, not downstream sink limits or cluster node
scaling. Treat the replica counts and 70% CPU target as results and settings
for this workload, not production sizing recommendations.

If you already operate autoscaled Vector deployments, you can skip the basic
setup and scaling sections. The sections on
[burst handling](#handling-sudden-bursts) and
[HPA rounding and stabilization](#deep-dive-why-the-hpa-can-stabilize-at-six-pods)
are useful when diagnosing slow scale-up or an unexpected steady-state replica
count; the walkthrough is not a comprehensive production operations guide.

All steps in this guide are reproducible. See [Replicating these results](#replicating-these-results)
for the manifests and Helm values used.

## Background

Vector's `parse_regex!` transform is CPU-bound: For every incoming log line, the transform
executes a compiled Rust regex, allocates capture-group values, and writes a
structured event downstream. Under sustained parallel HTTP load, a single Vector pod limited to 1 vCPU will
saturate that core due to the regex
parsing.

When CPU saturation occurs, Vector applies **backpressure instead of dropping
events**. Vector's `http_server` source keeps accepting connections but stalls
on responses until it can process the backlog, so the NGINX Ingress
Controller and the load generator experience stalled connections. This backpressure mechanism
avoids event loss only as long as those stalled connections stay open. If the
NGINX Ingress Controller or the load generator times out and closes a connection
first, the in-flight request's events are lost along with it.

## Architecture

```goat
+-----------------------------------------------+
|                  lading pod                   |
|      (100 parallel connections, 55 MiB/s)     |
+----------------------+------------------------+
                       |
                       | HTTP POST
                       v ingress-nginx ClusterIP :80
+-----------------------------------------------+
|          NGINX Ingress Controller             |
|        (L7 round-robin per request)           |
+----------------------+------------------------+                +----------------------------------+
                       |                                        /                                  /
                       | distributes requests                  /     Vector pod configuration     /
                       | across available pods                |                                  |
                       |                                      |       1 vCPU · 2 GiB each        |
         .-------------+-------------------.                  |                                  |
        |              |                    |     .-----------+   +---------------------------+  |
        v              v                    v    |            |   | source: http_server :9000 |  |
    +--------+     +--------+    .-.    +--------|-+          |   +-----------+---------------+  |
    | Vector |     | Vector |   | … |   | Vector   |          |               |                  |
    +---+----+     +---+----+    '-'    +---+------+          |               v                  |
        |              |                    |                 |  +----------------------------+  |
         '-------------+---+---------------'                  |  |  transform: parse_regex!() |  |
                       |                                      |  | (Apache Common Log Format) |  |
                       |                                      |  +------------+---------------+  |
                       | TCP consumer service                 |               |                  |
                       v                                      |               v                  |
        +---------------------------------+                   |  +--------------------------+    |
        |          consumer pod           |                   |  |     sink: socket (TCP)   |    |
        | (socat -u, drains to /dev/null) |                  /   +--------------------------+   /
        +---------------------------------+                 /                                  /
                                                           +----------------------------------+
```

### Load balancing with NGINX Ingress Controller

To distribute traffic across available pods as replicas are created and
removed, this architecture uses the NGINX Ingress Controller as an HTTP
layer 7 (L7) load balancer. It routes each request independently, so new pods
can receive traffic as soon as they become Ready.

Layer 7 routing is necessary because a TCP connection has no request boundary.
A Kubernetes ClusterIP Service load-balances at layer 4 (L4), so after a client
connects to a pod, the Service cannot redistribute that connection to a new
replica. HAProxy in TCP mode has the same limitation: a producer's connection
remains pinned to one consumer for its lifetime and can leave other consumers
without traffic.

For this reason, the experiments place the NGINX Ingress Controller in front
of Vector instead of exposing Vector directly through a ClusterIP Service.

## Methodology

The experiments apply a sustained CPU-bound workload to a stateless Vector
pipeline and compare one, three, and eight fixed replicas with an HPA-managed
deployment. We used a [K3s](https://k3s.io/) single-node cluster on an
[Amazon EC2](https://aws.amazon.com/ec2/) `c5.4xlarge` instance (16 vCPU,
32 GiB RAM). The single-node cluster reduces latency and network variance in
the measurements. The same setup can run on a local Kubernetes cluster with
sufficient capacity, although the results may be less consistent.

All runs use the same workload and pod configuration:

- **Load generator:** [lading](https://github.com/DataDog/lading), generating
  `apache_common` log lines over 100 persistent parallel connections.
- **Load level:** 55 MiB/s across all tests.
- **Vector pod resources:** 1 vCPU and 2 GiB of memory, with
  `requests == limits` for Guaranteed QoS.

## Manual scaling

Deploy the shared test components before starting the measurements. The
consumer drains all data forwarded by Vector, and the NGINX Ingress Controller
distributes requests across the Vector replicas:

{{< embed file="content/en/guides/level-up/k8s-autoscaling/manifests/consumer-chart/templates/consumer.yaml" dir="true" >}}

```bash
helm repo add ingress-nginx https://kubernetes.github.io/ingress-nginx
helm repo add vectordotdev https://helm.vector.dev
helm repo update

helm upgrade --install consumer manifests/consumer-chart \
  -n vector-perf --create-namespace --wait --timeout=3m

helm upgrade --install ingress-nginx ingress-nginx/ingress-nginx \
  -n ingress-nginx --create-namespace \
  --version 4.15.1 \
  --set controller.service.type=ClusterIP \
  --set controller.replicaCount=1 \
  --wait --timeout=3m
```

The following Helm values configure Vector with an
`http_server` source, the `parse_regex!` transform, and the `socket` sink that forwards data to
the consumer:

{{< embed file="content/en/guides/level-up/k8s-autoscaling/values.yaml" dir="true" >}}

```bash
helm upgrade --install vector vectordotdev/vector --namespace vector-perf --version 0.58.0 \
  -f values.yaml --set replicas=1 --set autoscaling.enabled=false --wait --timeout=3m

helm upgrade --install vector-ingress manifests/ingress-chart \
  -n vector-perf --wait --timeout=3m
helm upgrade --install producer manifests/producer-chart \
  -n vector-perf --wait --timeout=3m
```

The following Ingress routes HTTP POST requests to the Vector Service at the request level (L7),
so every pod receives a share of traffic as soon as it's Ready, independent of how or why the replica count changes:

{{< embed file="content/en/guides/level-up/k8s-autoscaling/manifests/ingress-chart/templates/ingress.yaml" dir="true" >}}

Note that `proxy-read-timeout` and `proxy-send-timeout` are left at their
60-second defaults. Under overload, a stalled connection that exceeds those
timeouts is closed by NGINX before Vector finishes processing it, losing
that request's events rather than just delaying them.

The producer is [lading](https://github.com/DataDog/lading), configured to
generate `apache_common` log lines at 55 MiB/s across 100 parallel connections:

{{< embed file="content/en/guides/level-up/k8s-autoscaling/manifests/producer-chart/templates/producer.yaml" dir="true" >}}

After the environment is running, collect two samples of Vector's per-component
counters 30 seconds apart. Calculate throughput from the change in
`receivedBytesTotal` for the `in` source. This input byte rate is a proxy for
pipeline throughput, not a measure of successful delivery to the downstream
consumer. Read per-pod CPU through `kubectl top pods` and average the value
across all Vector pods.

The scripts in [Replicating these results](#replicating-these-results) automate
these measurements.

At 55 MiB/s, the workload is expected to overwhelm a single pod's regex-parsing capacity.
When the pod reaches CPU saturation, Vector applies backpressure, reducing the rate at which lading can send data.

The pod is pinned at its 1000m CPU limit, and throughput tops out at
16.93 MiB/s, confirming the expected CPU ceiling. This per-pod throughput is the
baseline for the three- and eight-pod measurements.

After collecting the single-pod metrics, scale to three replicas, then eight.
Collect metrics after each upgrade before continuing to the next replica count:

```bash
# Three pods.
helm upgrade vector vectordotdev/vector --namespace vector-perf --version 0.58.0 \
  -f values.yaml --set replicas=3 --set autoscaling.enabled=false --wait --timeout=3m

# Eight pods.
helm upgrade vector vectordotdev/vector --namespace vector-perf --version 0.58.0 \
  -f values.yaml --set replicas=8 --set autoscaling.enabled=false --wait --timeout=3m
```

### Results

<!-- RESULTS-COMPARE-START -->

All three runs use a **55 MiB/s lading workload** (100 parallel connections through the L7 NGINX Ingress Controller),
with Vector pods limited to **1 vCPU and 2 GiB of memory**.

| | 1 pod | 3 pods | 8 pods |
| - | ----------------- | ------------------ | ------------------ |
| Throughput | 16.93 MiB/s | 49.07 MiB/s | **62.52 MiB/s** |
| Events/s | 133,098 | 385,840 | 491,589 |
| CPU per pod | 1000m (100%) | ~970m (97%) | ~470m (47%) |
| Bottleneck | Vector CPU | Vector CPU | **None** |
| Scaling vs. single pod | 1× | 2.90× | **3.69×** |

<!-- RESULTS-COMPARE-END -->

Three pods remain CPU-bound: based on the single pod results, their combined
throughput ceiling should be approximately 3 × 16.93 MiB/s = 50.79 MiB/s, below
the 55 MiB/s workload. The measured 49.07 MiB/s is 2.90× the single-pod
baseline, confirming that L7 routing makes the added replicas useful.

Eight pods remove the CPU bottleneck, but leave over half of each pod's capacity
unused at 47% average CPU utilization. Each pod handles approximately 7.8 MiB/s,
with load distributed evenly across all eight pods. Three pods are too few;
eight provide more capacity than this workload needs.

## Automatic scaling with HPA

Manual scaling confirmed that additional pods increase throughput, but choosing
a fixed replica count means trading spare capacity for the risk of CPU saturation.
As traffic changes, we would need to keep monitoring utilization and adjusting
that count ourselves. For CPU-bound workloads like this one, the HPA automates
those adjustments, adding pods when CPU utilization rises and removing them
when demand falls. This makes it a better fit for most deployments with varying
traffic than a fixed, manually managed replica count.

We can now configure the HPA to keep average CPU utilization around a 70%
target, leaving headroom without keeping all eight pods running:

```bash
# Reset to 1 pod and keep autoscaling disabled until the scale-down completes.
helm upgrade vector vectordotdev/vector --namespace vector-perf --version 0.58.0 \
  -f values.yaml --set replicas=1 --set autoscaling.enabled=false --wait --timeout=3m

# Create HPA (70% CPU target, 1–8 replicas) through the Vector release.
helm upgrade vector vectordotdev/vector --namespace vector-perf --version 0.58.0 \
  -f values.yaml --set replicas=1 \
  --set autoscaling.enabled=true \
  --set autoscaling.minReplicas=1 \
  --set autoscaling.maxReplicas=8 \
  --set autoscaling.targetCPUUtilizationPercentage=70 \
  --set autoscaling.behavior.scaleDown.stabilizationWindowSeconds=60 \
  --wait --timeout=3m
```

Based on the single-pod results, we can estimate how many pods we would need
to spin up to stay under CPU saturation while keeping some headroom. The
saturation crossover is 55 / 16.93 ≈ **3.25 pods** at 100% CPU. At a 70%
utilization target, the expected equilibrium is ⌈3.25 / 0.70⌉ = ⌈4.64⌉ = **5 pods**.

The following timeline shows how the HPA scales the deployment from one replica to five replicas:
<!-- RESULTS-HPA-START -->

| Time | Replicas | Avg CPU | Event |
| ---- | -------- | ------- | ----- |
| t=0 s | **1** | — | HPA starts |
| t=30 s | **2** | 100% | HPA scales 1→2 |
| t=61 s | **3** | 99% | HPA scales 2→3 |
| t=107 s | **5** | 99% | HPA scales 3→5 |
| t=138 s | **5** | 71% | — |
| t=169 s | **5** | **70%** | **Stable, equilibrium** |

Time to equilibrium: **169 seconds (approximately 3 minutes)**, 3 scale events, no manual scaling.

**Throughput at equilibrium: 60.69 MiB/s, 477,203 ev/s, 5 pods, 70% average CPU.**

The HPA settles at five pods: CPU converges from 99% during the 3→5
scale-up event to 70%, within the ±10% tolerance band (63–77%) set by the
[`--horizontal-pod-autoscaler-tolerance`](https://kubernetes.io/docs/reference/command-line-tools-reference/kube-controller-manager/)
flag's `0.1` default, and holds stable for three consecutive 15-second intervals.

<!-- RESULTS-HPA-END -->

Repeated runs can settle at a different replica count because the HPA rounds
its recommendation up to a whole pod. See [Why the HPA can stabilize at six
pods](#deep-dive-why-the-hpa-can-stabilize-at-six-pods) for the algorithm and
an example run that settles at six pods.

### Results comparison

| | 1 pod | 3 pods | 8 pods | HPA |
| - | ----------------- | ------------------ | ------------------ | ------------------ |
| Throughput | 16.93 MiB/s | 49.07 MiB/s | 62.52 MiB/s | **60.69 MiB/s** |
| Events/s | 133,098 | 385,840 | 491,589 | **477,203** |
| CPU per pod | 1000m (100%) | ~970m (97%) | ~470m (47%) | **~700m (70%)** |
| Bottleneck | Vector CPU | Vector CPU | None | None |
| Scaling vs. single pod | 1× | 2.90× | 3.69× | **3.58×** |
| Pod count | manual (1) | manual (3) | manual (8) | **auto (5)** |

The HPA delivers throughput comparable to eight manually scaled pods with three fewer pods and no manual scaling.
The HPA scales to five pods, matching the prediction
and keeping CPU at its 70% target instead of
leaving each pod with roughly 53% of unused CPU capacity.

## Handling sudden bursts

The HPA is a reactive controller. It can adjust capacity for sustained changes,
but it cannot make new pods Ready immediately when a burst begins. It only
reconciles and issues a scaling decision every [15 seconds by
default](https://kubernetes.io/docs/concepts/workloads/autoscaling/horizontal-pod-autoscale/),
and each new pod still needs to start and pass its readiness probe. The
binding deadline for an in-flight request is the [NGINX Ingress Controller's
proxy read and send
timeouts](https://kubernetes.github.io/ingress-nginx/user-guide/nginx-configuration/configmap/):
**60 seconds by default**, and left unchanged in this guide.

Consider a sudden burst that requires nine pods when only two are Ready. With
the pods limited to 1 vCPU, observed CPU cannot exceed 100%. At the 70% target,
the HPA formula therefore increases the recommendation through approximately
2→3→5→8→9 instead of detecting the full nine-pod demand immediately, and each
decision depends on another reconciliation cycle and fresh metrics. Even if
`maxReplicas` is raised to nine, scaling takes about one minute or longer after
pod startup and readiness are included. With this guide's `maxReplicas: 8`,
the deployment can never reach nine pods.

During that interval, Vector applies backpressure and HTTP requests stall. If
NGINX goes 60 seconds without progress while reading from or writing to the
Vector upstream, it closes the connection. The events in that request are then
lost. New replicas can accept later requests, but they cannot recover a request
that has already timed out.

Even when running these experiments with a constant log stream of 55 MiB/s some
logs were inevitably lost. The HPA took over 100 seconds to scale to 5 pods and
the CPU was bottlenecked before scaling. A single pod receiving a large influx
of data is essentially a burst — even if the throughput is constant it still
overwhelms the single pod.

If sudden bursts must not lose data, use one or more of the following:

- **Put a durable queue before Vector:** Send events to [Kafka](/docs/reference/configuration/sources/kafka/),
  [Google Cloud Pub/Sub](/docs/reference/configuration/sources/gcp_pubsub/), or
  another durable broker, then let Vector consume the backlog. Queue depth or
  consumer lag is also a better scaling signal than CPU for burst demand.
- **Pre-provision capacity:** Set `minReplicas` high enough to absorb the burst,
  or scale up on a schedule before a predictable burst.
- **Scale from backlog:** Use external metrics to scale on queue depth or
  consumer lag. The queue provides durability while the new pods start; the
  autoscaler reduces the time needed to drain it.
- **Make producers retry safely:** Producers can retain unacknowledged events
  and retry failed requests. Account for possible duplicates when retrying.
- **Increase proxy timeouts:** Set the read and send timeouts above the expected
  scale-up and backlog-drain time. This keeps requests open longer but does not
  provide durable storage, so it is not sufficient by itself.


## Key takeaways

1. **Avoid the connection-pinning trap.** With long-lived connections behind
   L4 load balancing, adding pods does not move existing traffic from overloaded
   pods. Use L7 per-request routing so new replicas receive load immediately.

2. **Measure one pod before sizing the deployment.** Saturate one pod, record
   its throughput and CPU utilization, and use that baseline to calculate how
   many replicas the workload requires.

3. **Configure the HPA for headroom, not saturation.** Set CPU requests, then
   choose a target below 100%. For this workload, a 70% target reduced the
   required deployment from eight manually provisioned pods to five
   automatically managed pods.

4. **Do not rely on the HPA alone for sudden bursts.** Repeated 15-second HPA
   reconciliations plus pod startup and readiness can make scale-up take about
   one minute or longer. That can exceed the NGINX Ingress Controller's
   60-second default proxy timeouts and cause in-flight events to be lost. Keep
   enough pods Ready to absorb the burst or put a durable queue before Vector.

For your own deployment, the next step is to repeat the single-pod measurement
with representative traffic in a test environment, then verify traffic reaches
added replicas before enabling the HPA. Test burst behavior as well as steady-state
throughput; a stable replica count alone does not establish reliable delivery.

## Replicating

The Helm values, charts, and scripts used throughout this guide live in
[`k8s-autoscaling/`](https://github.com/vectordotdev/vector/tree/master/website/content/en/guides/level-up/k8s-autoscaling).

### Tools and cluster requirements

- [`helm`](https://helm.sh/) version 3.0 or later, configured for the target cluster
- [`kubectl`](https://kubernetes.io/docs/reference/kubectl/) for cluster inspection and port-forwarding
- Python 3.12 or later and [Ansible Core](https://docs.ansible.com/ansible/latest/index.html) 2.20.5
- The `kubernetes.core` Ansible collection
- [`grpcurl`](https://github.com/fullstorydev/grpcurl) for metric collection
- The [Kubernetes Metrics API](https://github.com/kubernetes-sigs/metrics-server);
  run `kubectl top nodes` to verify that it is available
- At least 9 allocatable CPUs: 8 for Vector at maximum scale, 0.5 for the
  consumer, and 0.2 for the producer

You can use any Kubernetes cluster with enough capacity, including a local
cluster. The [`terraform/`](https://github.com/vectordotdev/vector/tree/master/website/content/en/guides/level-up/k8s-autoscaling/terraform)
directory provisions the K3s single-node EC2 environment used for the published
measurements if you want to reduce infrastructure variance.

After deploying the components in [Manual scaling](#manual-scaling), the
`run-experiment.yaml` Ansible playbook can run all four experiments or one
selected experiment. It updates the Vector release, waits for the deployment
to become ready, measures throughput, and manages the chart-provided HPA for
the autoscaling experiment.

The playbook first scales Vector to 0 replicas with autoscaling disabled and
waits for its pods to terminate, so every invocation starts from the same clean
state instead of measuring a transition from the replica count left by a
previous run. It then installs the producer once with a unique `runId` so the
lading workload restarts, waits 20 seconds for lading to initialise, and runs
each experiment in sequence.

Install the collection once from the `k8s-autoscaling/` directory:

```bash
ansible-galaxy collection install -r scripts/requirements.yaml
```

The playbook deploys the workloads and runs each experiment in sequence; the
probe helper (`scripts/probe.py`) waits for the pods to stabilise, measures
throughput over a 30-second window, and watches the HPA until it reaches
equilibrium. Helm remains the sole owner of the Deployment and HPA
configuration. The probe inherits the same `KUBECONFIG` as the playbook and
prints one JSON result per experiment; the playbook aggregates those results
and prints the summary.

{{< embed file="content/en/guides/level-up/k8s-autoscaling/scripts/run-experiment.yaml" open="false" >}}

{{< embed file="content/en/guides/level-up/k8s-autoscaling/scripts/experiment.yaml" open="false" >}}

```bash
# Run all experiments.
KUBECONFIG=/path/to/kubeconfig ansible-playbook -i localhost, scripts/run-experiment.yaml

# Run one experiment (1, 3, or 8 pods, or hpa).
KUBECONFIG=/path/to/kubeconfig ansible-playbook -i localhost, scripts/run-experiment.yaml -e experiment=hpa
```

## Deep dive: Why the HPA can stabilize at six pods

### Stabilizing at 6?

All the calculations and empirical evidence suggest that 5 is the correct
number of pods for the HPA to find the equilibrium. However, repeating the HPA
experiment a few times might yield different results.

The following timeline shows a test run in which the HPA stabilized at six pods
instead of the expected five:

| Time | Replicas | Avg CPU | Event |
| ---- | -------- | ------- | ----- |
| t=0 s | **1** | 100% | Load starts |
| t=30 s | **2** | 100% | HPA scales 1→2 |
| t=61 s | **3** | 98% | HPA scales 2→3 |
| t=91 s | **4** | 96% | HPA scales 3→4 |
| t=122 s | **6** | 91% | HPA scales 4→6 |
| t=137 s | **6** | 67% | — |
| t=182 s | **6** | **60%** | **Stable, equilibrium** |

But ... why did the HPA settle at six pods? We are using the 70% CPU threshold
and didn't alter the HPA's default 10% tolerance band. Yet the observed CPU
utilization of 60% is clearly outside the resulting 63–77% target range. This
happened because the HPA overshot the pod count, likely because some pods
parsed data more slowly than the benchmark predicted.

However, according to the HPA algorithm, both five pods and six pods are valid
stable replica counts. When the HPA determines that the current CPU utilization
falls outside the target range, it calculates the desired number of pods
according to the following formula
([source](https://github.com/kubernetes/kubernetes/blob/v1.36.2/pkg/controller/podautoscaler/replica_calculator.go#L117-L118)):

```text
desired = ⌈ currentReplicas × (currentAvgCPU / 70%) ⌉
```

This calculation can produce unexpected but valid outcomes, such as the
six-pod stabilization observed in the repeated HPA run, even when the
average CPU utilization falls outside the configured target range. When the
HPA recalculates the desired replica count using the observed 60% CPU
utilization, it still selects six pods:

```text
desired = ⌈ 6 × (60% / 70%) ⌉ = ⌈ 5.1428571429 ⌉ = 6
```

Once the deployment passes the saturation point, the workload is no longer
CPU-bound. The *total* CPU demand becomes fixed, and the HPA spreads that
demand across the available pods. Slower pods change this number: A pod that
parses 10% slower needs approximately 10% more CPU for the same 55 MiB/s
workload, increasing the total CPU demand. Faster pods reduce the total CPU demand.

Based on the single-pod results, the total workload demand is:

```text
total CPU demand = (55 / 16.93) × 100% = 324.9 pod-percent
```

Using the total CPU demand, we can calculate theoretical stabilization pod
counts. The following table shows the theoretical stabilization points for
per-pod speeds ranging from 10% faster than the benchmark to 15% slower. A ✅
indicates a stable resting point.

| Per-pod speed vs. benchmark | +10% faster | Benchmark   | 10% slower  | 15% slower  |
| --------------------------- | ----------- | ----------- | ----------- | ----------- |
| Per-pod throughput          | 18.62 MiB/s | 16.93 MiB/s | 15.24 MiB/s | 14.39 MiB/s |
| Total CPU demand            | 295%        | 325%        | 361%        | 382%        |
| **4 pods**                  | 74% ✅      | 81%         | 90%         | 96%         |
| **5 pods**                  | 59% ✅      | 65% ✅      | 72% ✅      | 76% ✅      |
| **6 pods**                  | 49%         | 54%         | 60% ✅      | 64% ✅      |
| **7 pods**                  | 42%         | 46%         | 52%         | 55%         |

These values are theoretical because they're based on the single-pod results. Even
when the HPA stabilized at the expected five pods, the observed CPU utilization
was around 70% instead of the projected 65%. Real-world scenarios will likely
fall somewhere in between the benchmark and the 10% slower band, which can lead
to the results we observed.

Comparing the observed CPU utilization (70%) with the theoretical prediction
(64.97%) shows a difference of 7.74%:

```text
((70% - 64.97%) / 64.97%) × 100 = 7.74%
```

This suggests that the pods in the original HPA run parsed data about
7.74% more slowly than the benchmark predicted.

---
