---
date: "2026-07-01"
title: Load balancing and scaling Vector on Kubernetes
short: K8s autoscaling
description: Observe a single Vector pod reaching its CPU ceiling, eliminate the ceiling by manually scaling horizontally behind an L7 load balancer, and then automate that scaling with the Kubernetes HPA to reach a stable replica count that maintains target average CPU utilization.
authors: ["thomasqueirozb"]
domain: platforms
weight: 7
tags: ["level up", "guides", "guide", "kubernetes", "load balancing", "nginx"]
---

Running a CPU-intensive Vector pipeline in Kubernetes without the right setup
is likely to hit a ceiling and fail under load. Vector is a fast, focused data
pipeline, not a platform that handles scaling or load balancing automatically.

In this guide, we'll show how a single Vector pod reaches its CPU ceiling while
parsing [Apache Common Log Format](https://httpd.apache.org/docs/current/logs.html#common) data. We'll then eliminate that ceiling by manually
scaling Vector horizontally behind the [NGINX](https://www.nginx.com/) Ingress Controller, an L7 load balancer. Finally, we'll set up automatic
scaling by using the Kubernetes [Horizontal Pod Autoscaler (HPA)](https://kubernetes.io/docs/tasks/run-application/horizontal-pod-autoscale/)
to reach a stable replica count that maintains a target average CPU utilization of 70%.

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
Controller and the load generator experience stalled connections. This only
avoids event loss as long as those stalled connections stay open. If the
NGINX Ingress Controller or the load generator times out and closes one
first, the in-flight request's events are lost along with it.

## Test environment

To evaluate Vector's scaling behavior under a sustained CPU-bound workload, we used a **[K3s](https://k3s.io/) single-node cluster hosted on an [Amazon EC2](https://aws.amazon.com/ec2/) c5.4xlarge** instance
(16 vCPU, 32 GiB RAM). We chose a single-node cluster to eliminate latency and
network overhead as factors, making the collected metrics more precise.
We used the following configuration for the tests:

- **Load generator:** [lading](https://github.com/DataDog/lading),
  generating `apache_common` log lines at a configurable byte rate. It
  maintains persistent parallel connections and is capable of generating sustained
  high-throughput HTTP load.
- **Load level:** **55 MiB/s** across all tests to get comparable
  throughput measurements.
- **Vector pod resources:** **1 vCPU and 2 GiB of memory**, with `requests == limits`
  (Guaranteed QoS) to ensure that CPU throttling, not memory pressure or scheduling
  variance, was the only bottleneck tested.

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

### Why HTTP with L7 load balancing?

A plain TCP connection has no request boundary: Once a client is connected to
a pod, a Kubernetes ClusterIP Service (which load-balances at L4) cannot
redistribute that traffic to a newly created pod. By contrast, HTTP
defines a request boundary, so an L7 load balancer such as the NGINX Ingress Controller can route
each request independently. As new pods become Ready, they can pick up load immediately.

A similar setup using [HAProxy](https://www.haproxy.org/) in TCP mode has the same limitation as a Kubernetes ClusterIP Service: It
load-balances at the connection level, so a single producer's connection stays
pinned to one consumer for its lifetime and can leave some consumers starved
of data entirely.

This is why we installed an NGINX Ingress Controller in front of Vector instead of exposing
Vector through a ClusterIP Service.

## Prerequisites

- [`helm`](https://helm.sh/) version 3.0 or later, configured against a target cluster
- [`kubectl`](https://kubernetes.io/docs/reference/kubectl/) for read-only cluster inspection and port-forwarding
- At least 9 allocatable CPUs total (8 for Vector at max scale, 0.5 for the consumer, 0.2 for the producer)
- [`grpcurl`](https://github.com/fullstorydev/grpcurl) for metric collection
- [Kubernetes Metrics API](https://github.com/kubernetes-sigs/metrics-server) (`metrics-server`) installed (This is required for `kubectl top pods` and HPA CPU targets. K3s bundles `metrics-server` by default. On other clusters, run `kubectl top nodes` to verify that `metrics-server` is available before you start.)

## Collecting throughput and CPU metrics

Each Vector pod exposes [`ObservabilityService`](https://github.com/vectordotdev/vector/blob/master/proto/vector/observability.proto) on port 8686 ([gRPC](https://grpc.io/)). For
each phase of our testing, we measured throughput by port-forwarding to a pod,
capturing two `GetComponents` samples 30 seconds apart, and calculating the difference in `receivedBytesTotal` for
the `in` source component to determine a per-pod throughput rate. Per-pod CPU was
read via `kubectl top pods` and averaged across all Vector pods.

The following commands collect the data used to calculate throughput for a single pod:

```bash
kubectl port-forward -n vector-perf pod/<pod-name> 18686:8686 &

grpcurl -plaintext -d '{}' localhost:18686 \
  vector.observability.v1.ObservabilityService/GetComponents > t0.json
sleep 30
grpcurl -plaintext -d '{}' localhost:18686 \
  vector.observability.v1.ObservabilityService/GetComponents > t30.json
```

The difference in `receivedBytesTotal` for the `in` component between `t0.json` and
`t30.json`, divided by 30 seconds, gives that pod's throughput.

See [Replicating these results](#replicating-these-results) for a link to the script that
automates this process.

## Setup

The following Helm release creates the namespace and deploys the consumer that drains all data forwarded by Vector:

{{< embed file="content/en/guides/level-up/k8s-autoscaling/manifests/consumer-chart/templates/consumer.yaml" dir="true" >}}

```bash
helm upgrade --install consumer manifests/consumer-chart \
  -n vector-perf --create-namespace --wait --timeout=3m

helm repo add ingress-nginx https://kubernetes.github.io/ingress-nginx
helm upgrade --install ingress-nginx ingress-nginx/ingress-nginx \
  -n ingress-nginx --create-namespace \
  --version 4.15.1 \
  --set controller.service.type=ClusterIP \
  --set controller.replicaCount=1 \
  --wait --timeout=3m

helm repo add vectordotdev https://helm.vector.dev
helm repo update
```

## Phase 1: Single pod

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
60s defaults. Under overload, a stalled connection that exceeds those
timeouts is closed by NGINX before Vector finishes processing it, losing
that request's events rather than just delaying them.

The producer is [lading](https://github.com/DataDog/lading), configured to
generate `apache_common` log lines at 55 MiB/s across 100 parallel connections:

{{< embed file="content/en/guides/level-up/k8s-autoscaling/manifests/producer-chart/templates/producer.yaml" dir="true" >}}

At 55 MiB/s, the workload is expected to overwhelm a single pod's regex-parsing capacity.
When the pod reaches CPU saturation, Vector applies backpressure, reducing the rate at which lading can send data.

The resulting throughput and CPU utilization are shown in the following table:

<!-- RESULTS-SINGLE-START -->

| Metric | Value |
| ------ | ----- |
| Throughput | **16.93 MiB/s** |
| Events/s | **133,098 ev/s** |
| Pod CPU | **1000m (100%)** |
| Bottleneck | **Vector CPU** |

<!-- RESULTS-SINGLE-END -->

The pod is pinned at its 1000m CPU limit, and throughput tops out at
16.93 MiB/s, confirming the expected CPU ceiling. This per-pod throughput is the
baseline that the next two phases are measured against.

## Phase 2: 3 pods

The following command scales the deployment to three replicas through its Helm release:

```bash
helm upgrade vector vectordotdev/vector --namespace vector-perf --version 0.58.0 \
  -f values.yaml --set replicas=3 --set autoscaling.enabled=false --wait --timeout=3m
```

At 55 MiB/s, the workload still exceeds the combined throughput ceiling of three
pods (3 × 16.93 MiB/s = 50.79 MiB/s). All three pods remain CPU-bound.

<!-- RESULTS-LB-START -->

| Metric | Value |
| ------ | ----- |
| Throughput | **49.07 MiB/s** |
| Events/s | **385,840 ev/s** |
| Pod CPU | **~970m (97%)** |
| Scaling vs. Phase 1 | **2.90×** |
| Bottleneck | **Vector CPU** |

<!-- RESULTS-LB-END -->

## Phase 3: 8 pods

The following command scales the deployment to eight replicas through its Helm release:

```bash
helm upgrade vector vectordotdev/vector --namespace vector-perf --version 0.58.0 \
  -f values.yaml --set replicas=8 --set autoscaling.enabled=false --wait --timeout=3m
```

Eight pods provide a combined throughput ceiling of approximately 135.4 MiB/s (8 × 16.93 MiB/s = 135.4 MiB/s), well above the workload's 55 MiB/s. The bottleneck is
eliminated. The full workload flows through, and the pods have ample CPU headroom.

<!-- RESULTS-8W-START -->

| Metric | Value |
| ------ | ----- |
| Throughput | **62.52 MiB/s** |
| Events/s | **491,589 ev/s** |
| Pod CPU | **~470m (47%)** |
| Bottleneck | **None, spare capacity** |

Each pod handles approximately 7.8 MiB/s at about 47% CPU utilization,
leaving over half of each pod's capacity unused. With L7 per-request routing,
load is distributed evenly across all eight pods.

<!-- RESULTS-8W-END -->

## Comparison: Phases 1–3

<!-- RESULTS-COMPARE-START -->

All phases use a **55 MiB/s lading workload** (100 parallel connections through the L7 NGINX Ingress Controller),
with Vector pods limited to **1 vCPU and 2 GiB of memory**.

| | Phase 1 (1 pod) | Phase 2 (3 pods) | Phase 3 (8 pods) |
| - | ----------------- | ------------------ | ------------------ |
| Throughput | 16.93 MiB/s | 49.07 MiB/s | **62.52 MiB/s** |
| Events/s | 133,098 | 385,840 | 491,589 |
| CPU per pod | 1000m (100%) | ~970m (97%) | ~470m (47%) |
| Bottleneck | Vector CPU | Vector CPU | **None** |
| Scaling vs. Phase 1 | 1× | 2.90× | **3.69×** |

<!-- RESULTS-COMPARE-END -->

We can see that eight pods is too many, but three pods is too few. At eight pods, we're not
properly utilizing each pod's capacity (only 47% average CPU utilization).

## Phase 4: HPA finds equilibrium

Based on the results of Phase 1, we can estimate how many pods we would need
to spin up to stay under CPU saturation while keeping some headroom. The
saturation crossover is 55 / 16.93 ≈ **3.25 pods** at 100% CPU. At a 70%
utilization target, the expected equilibrium is ⌈3.25 / 0.70⌉ = ⌈4.64⌉ = **5 pods**.

We can now configure the HPA to find the minimum pod count that keeps CPU
utilization around the 70% target.

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

The following timeline shows how the HPA scales the deployment from one replica to five replicas:
<!-- RESULTS-HPA-START -->

**Scale-up timeline (no manual intervention):**

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

### Stabilizing at 6?

All the calculations made and empirical evidence suggests that 5 is the correct
number of pods for the HPA to find the equilibrium. However, running this a few
times might yield different results.

| Time | Replicas | Avg CPU | Event |
| ---- | -------- | ------- | ----- |
| t=0 s | **1** | 100% | Load starts |
| t=30 s | **2** | 100% | HPA scales 1→2 |
| t=61 s | **3** | 98% | HPA scales 2→3 |
| t=91 s | **4** | 96% | HPA scales 3→4 |
| t=122 s | **6** | 91% | HPA scales 4→6 |
| t=137 s | **6** | 67% | — |
| t=182 s | **6** | **60%** | **Stable, equilibrium** |

But... Why? We are using the 70% CPU threshold and didn't alter the autoscaler's
default 10% tolerance band. 60% is clearly outside the 63-77% band. This only
happened because the HPA overshot the pod count — and it's possible this occured
due to a variety of reasons, with the likely explanation that some pods running slower than expected.
However, according to the HPA algorithm both are valid resting points. After
determining that the current CPU load falls outside the threshold, it then
calculates the number of pods according to the following formula
([source](https://github.com/kubernetes/kubernetes/blob/v1.36.2/pkg/controller/podautoscaler/replica_calculator.go#L117-L118)):

```text
desired = ⌈ currentReplicas × (currentAvgCPU / 70%) ⌉
```

This can lead to some very interesting results, just like the 6 pod stabilization,
even though the CPU load falls squarely out of bounds.

```text
desired = ⌈ 6 × (60% / 70%) ⌉ = ⌈ 5.1428571429 ⌉ = 6
```

Past the saturation point, the workload is no longer CPU-bound. The *total* CPU
demand is fixed, and the HPA spreads it across the available pods. A pod that
parses 10% slower needs approximately 10% more CPU for the same 55 MiB/s
workload, increasing the total CPU demand. Faster pods reduce it.

Based on Phase 1's results, the total workload demand is:

```text
total CPU demand = (55 / 16.93) × 100% = 324.9 pod-percent
```

Based on the calculated total CPU demand we can calculate theoretical
stabilization pod counts. The table below sweeps the per-pod speed ±10% (and
also 15% slower).

A ✅ marks a stable resting point where the HPA does not scale.

| Per-pod speed vs. benchmark | +10% faster | Benchmark | 10% slower | 15% slower |
| --------------------------- | ----------- | --------- | ---------- | ---------- |
| Per-pod throughput (MiB/s)  | 18.62       | 16.93     | 15.24      | 14.39      |
| Total CPU demand            | 295%        | 325%      | 361%       | 382%       |
| **4 pods**                  | 74% ✅      | 81%       | 90%        | 96%        |
| **5 pods**                  | 59% ✅      | 65% ✅    | 72% ✅     | 76% ✅     |
| **6 pods**                  | 49%         | 54%       | 60% ✅     | 64% ✅     |
| **7 pods**                  | 42%         | 46%       | 52%        | 55%        |

We can see that these values are theoretical, since they're based on Phase 1's results.
Even when we stabilized at the expected 5 pods the CPU utilization was around 70%
instead of the projected 65%. Real world scenarios will likely fall somewhere
in between the benchmark and the 10% slower band, which can lead to the results
we observed.

```text
((70% - 64.97%) / 64.97%) × 100 = 7.74%
```

In the original Phase 4 run we're about 7.74% slower than the predicted benchmark.

## Results summary

| | Phase 1 (1 pod) | Phase 2 (3 pods) | Phase 3 (8 pods) | Phase 4 (HPA) |
| - | ----------------- | ------------------ | ------------------ | ------------------ |
| Throughput | 16.93 MiB/s | 49.07 MiB/s | 62.52 MiB/s | **60.69 MiB/s** |
| Events/s | 133,098 | 385,840 | 491,589 | **477,203** |
| CPU per pod | 1000m (100%) | ~970m (97%) | ~470m (47%) | **~700m (70%)** |
| Bottleneck | Vector CPU | Vector CPU | None | None |
| Scaling vs. Phase 1 | 1× | 2.90× | 3.69× | **3.58×** |
| Pod count | manual (1) | manual (3) | manual (8) | **auto (5)** |

Phase 4 delivers throughput comparable to Phase 3 with three fewer pods and no manual scaling.
The HPA scales to five pods, matching the prediction
and keeping CPU at its 70% target instead of
leaving each pod with roughly 53% of unused CPU capacity.

## Key takeaways

1. **A CPU-bound workload eventually reaches the processing capacity of a
   single Vector pod**. When that happens, backpressure prevents any event
   loss\*.

2. **L7 per-request routing distributes load uniformly.**  Because the NGINX Ingress Controller
   routes each HTTP request independently, every pod (old or newly
   Ready) receives a share of traffic proportional to the current replica
   count, with no idle pods.

3. **Adding pods beyond the saturation point removes the CPU bottleneck entirely.** Once the workload is no longer CPU-bound, throughput increases while CPU utilization per pod decreases.

4. **The HPA determines the right pod count automatically.**  With HTTP and L7 routing,
   each new pod starts receiving traffic immediately after becoming Ready.

\* This holds only as long as the stalled connection stays open. If the NGINX
Ingress Controller or the load generator times out and closes a stalled
connection first, the in-flight request's events are lost along with it.

---

## Replicating these results

The Helm values, charts, and scripts used throughout this guide live in
[`k8s-autoscaling/`](https://github.com/vectordotdev/vector/tree/master/website/content/en/guides/level-up/k8s-autoscaling).

The [`terraform/`](https://github.com/vectordotdev/vector/tree/master/website/content/en/guides/level-up/k8s-autoscaling/terraform)
directory provisions the K3s single-node cluster (EC2 `c5.4xlarge`) that
we used, if you don't already have a cluster to test
against.

Once the [Setup](#setup) steps are complete and Phase 1's producer and ingress
are deployed, `run-experiment.sh` can run all four phases or one selected phase.
It updates the Vector release, waits for the deployment to become ready,
measures throughput, and manages the chart-provided HPA for Phase 4.

The script first scales Vector to 0 replicas and waits for its pods to
terminate, so every invocation starts from the same clean state instead of
measuring a transition from the replica count left by a previous run.

{{< embed file="content/en/guides/level-up/k8s-autoscaling/scripts/run-experiment.sh" open="false" >}}

```bash
# Run all phases.
KUBECONFIG=/path/to/kubeconfig ./scripts/run-experiment.sh

# Run one phase (1, 2, 3, or 4).
KUBECONFIG=/path/to/kubeconfig ./scripts/run-experiment.sh 4
```
