---
date: "2026-07-01"
title: Load balancing and scaling Vector on Kubernetes
short: K8s autoscaling
description: Observe a single Vector pod hit its CPU ceiling and eliminate it by scaling horizontally behind an L7 load balancer, with the HPA finding its own equilibrium.
authors: ["thomasqueirozb"]
domain: platforms
weight: 7
tags: ["level up", "guides", "guide", "kubernetes", "load balancing", "nginx"]
---

This guide walks through observing a single Vector pod hit its CPU ceiling while
parsing Apache Common Log format, then eliminating that ceiling by scaling
horizontally behind an Nginx L7 load balancer.
All steps are reproducible using the manifests and Helm values in this repository.

## Background

Vector's `parse_regex!` transform is CPU-bound: for every incoming log line it
executes a compiled Rust regex, allocates capture-group values, and writes a
structured event downstream.  A single Vector pod limited to 1 vCPU will
saturate that core under sustained parallel HTTP load due to the regex
parsing.

When saturation is reached, Vector applies **backpressure rather than dropping
events**. The HTTP source stops accepting new requests; Nginx stalls the load
generator's connections.

## Why HTTP + L7 load balancing?

A Kubernetes ClusterIP Service load-balances at L4: kube-proxy (iptables/IPVS)
picks a backend pod at **connection establishment** and that mapping is fixed for
the lifetime of the connection.  With a persistent TCP producer, all connections
are already pinned to the original pods before the HPA fires.  When a new pod
becomes Ready, it receives zero connections and therefore zero load. The HPA
sees no CPU drop on the old pods, so it keeps scaling up until it hits
`maxReplicas`, never finding a stable equilibrium.

HTTP with an Nginx L7 ingress routes at the **request level**.  Each HTTP POST
is dispatched independently to a backend pod, regardless of which pod served the
previous request.  The moment a new pod becomes Ready it starts receiving its
share of requests.  CPU redistributes within seconds, and the HPA can converge
to a genuine equilibrium without any manual intervention, as Phase 4 below
shows.

This is why the setup below installs an Nginx ingress in front of Vector
instead of exposing it through a plain ClusterIP Service.

## Test environment

The benchmark was measured on a **K3s single-node cluster on an EC2 c5.4xlarge**
(16 vCPU, 32 GiB RAM). A single-node cluster was chosen so that latency and
network overhead are not a factor and collected metrics are precise.

- **Load generator:** [lading](https://github.com/DataDog/lading) v0.32.0,
  generating `apache_common` log lines at a configurable byte rate. It
  maintains persistent parallel connections and is capable of sustained
  high-throughput HTTP load.
- **Load level:** **65 MiB/s** across all phases: enough to overwhelm 1–3
  Vector pods but within what 4+ pods can absorb, so the HPA finds a natural
  equilibrium.
- **Vector pod resources:** **1 vCPU / 1 GiB**, with `requests == limits`
  (Guaranteed QoS), so CPU throttling, not memory pressure or scheduling
  variance, is the only bottleneck under test.

## Prerequisites

- `kubectl` configured against a target cluster
- `helm` ≥ 3.0
- Cluster nodes with at least 1 allocatable CPU per Vector pod
- `grpcurl` for metric collection


## Architecture

```
1 × lading pod  (100 parallel connections, 65 MiB/s)
        │  HTTP POST → ingress-nginx ClusterIP :80
        ▼
   Nginx ingress controller  (L7 round-robin per request)
        │
        ▼ (1, 3, or 8 pods depending on phase)
   Vector pod(s)  (1 vCPU / 1 GiB each)
   ┌──────────────────────────────────────┐
   │ source:    http_server :9000         │
   │ transform: parse_regex! (apache_clf) │
   │ sink:      socket TCP → consumer     │
   └──────────────────────────────────────┘
        │  TCP → consumer Service
        ▼
   consumer pod  (socat -u, drains to /dev/null)
```

## How the metrics are collected

Each Vector pod exposes `ObservabilityService` on port 8686 (gRPC). The
measurement approach used for every phase below is: port-forward to a pod,
take two `GetComponents` samples 30 s apart, and diff `receivedBytesTotal` on
the `in` source component to get a per-pod throughput rate. Per-pod CPU is
read via `kubectl top pods` and averaged across all Vector pods.

For example, against a single pod:

```bash
kubectl port-forward -n vector-perf pod/<pod-name> 18686:8686 &

grpcurl -plaintext -d '{}' localhost:18686 \
  vector.observability.v1.ObservabilityService/GetComponents > t0.json
sleep 30
grpcurl -plaintext -d '{}' localhost:18686 \
  vector.observability.v1.ObservabilityService/GetComponents > t30.json
```

Diffing `receivedBytesTotal` for the `in` component between `t0.json` and
`t30.json`, then dividing by 30 s, gives that pod's throughput.

`run-experiment.sh` automates this across all four phases end to end,
scaling the deployment, waiting for the rollout, measuring throughput, and
creating the HPA for Phase 4, and prints a single results table:

{{< embed file="content/en/guides/level-up/k8s-autoscaling/scripts/run-experiment.sh" >}}

```bash
KUBECONFIG=/path/to/kubeconfig ./scripts/run-experiment.sh
```

Multiply the per-pod throughput by the number of equally-loaded pods for the
cluster total (verify with `kubectl top pods -n vector-perf -l app.kubernetes.io/name=vector`).

---

## Setup

Create the namespace and the consumer that drains everything Vector forwards to it:

{{< embed file="content/en/guides/level-up/k8s-autoscaling/manifests/namespace.yaml" >}}

{{< embed file="content/en/guides/level-up/k8s-autoscaling/manifests/consumer.yaml" >}}

```bash
kubectl apply -f manifests/namespace.yaml
kubectl apply -f manifests/consumer.yaml

helm repo add ingress-nginx https://kubernetes.github.io/ingress-nginx
helm upgrade --install ingress-nginx ingress-nginx/ingress-nginx \
  -n ingress-nginx --create-namespace \
  --set controller.service.type=ClusterIP \
  --set controller.replicaCount=1 \
  --wait --timeout=3m

helm repo add vectordotdev https://helm.vector.dev
helm repo update
```

---

## Phase 1 — Single pod

Vector is installed with the shared base Helm values, which configure the
`http_server` source, the `parse_regex!` transform, and the `socket` sink to
the consumer:

{{< embed file="content/en/guides/level-up/k8s-autoscaling/scenarios/base/values.yaml" >}}

```bash
helm upgrade --install vector vectordotdev/vector --namespace vector-perf -f scenarios/base/values.yaml --set replicas=1

kubectl apply -f manifests/ingress.yaml
kubectl apply -f manifests/producer.yaml
```

The ingress routes HTTP POSTs to the Vector service at the request level (L7),
which is what lets the HPA find equilibrium in Phase 4:

{{< embed file="content/en/guides/level-up/k8s-autoscaling/manifests/ingress.yaml" >}}

The producer is [lading](https://github.com/DataDog/lading), configured to
generate `apache_common` log lines at 65 MiB/s across 100 parallel connections:

{{< embed file="content/en/guides/level-up/k8s-autoscaling/manifests/producer.yaml" >}}

65 MiB/s is expected to overwhelm a single pod's regex-parsing capacity, so
Vector should back-pressure lading down to whatever it can actually process.

### Phase 1 results

<!-- RESULTS-SINGLE-START -->

| Metric | Value |
|--------|-------|
| Throughput | **19.04 MiB/s** |
| Events/s | **149,710 ev/s** |
| Pod CPU | **1000m (100 %)** |
| Bottleneck | **Vector CPU** |

<!-- RESULTS-SINGLE-END -->

The pod is pinned at its 1000m CPU limit and throughput tops out at
19.04 MiB/s, confirming the expected CPU ceiling. That per-pod figure is the
baseline the next two phases are measured against.

---

## Phase 2 — Three pods (still bottlenecked)

```bash
kubectl scale deployment vector -n vector-perf --replicas=3
kubectl rollout status deployment/vector -n vector-perf
```

65 MiB/s > 3 × 19 MiB/s = 57 MiB/s combined capacity.  All three pods are
still fully saturated. Adding pods increases throughput, but the ceiling
hasn't been reached yet.

### Phase 2 results

<!-- RESULTS-LB-START -->

| Metric | Value |
|--------|-------|
| Throughput | **55.38 MiB/s** |
| Events/s | **435,543 ev/s** |
| Pod CPU | **~1000m (99 %)** |
| Scaling vs Phase 1 | **2.91×** |
| Bottleneck | **Vector CPU** |

<!-- RESULTS-LB-END -->

---

## Phase 3 — Eight pods (bottleneck removed)

```bash
kubectl scale deployment vector -n vector-perf --replicas=8
kubectl rollout status deployment/vector -n vector-perf
```

8 × 19 MiB/s = 152 MiB/s combined capacity >> 65 MiB/s.  Vector is no longer
the bottleneck; all 65 MiB/s flows through and pods have ample headroom.

### Phase 3 results

<!-- RESULTS-8W-START -->

| Metric | Value |
|--------|-------|
| Throughput | **62.32 MiB/s** |
| Events/s | **490,288 ev/s** |
| Pod CPU | **~480m (48 %)** |
| Bottleneck | **None, spare capacity** |

The bottleneck has been eliminated.  Each pod handles ~7.8 MiB/s at ~48 % CPU,
leaving over half of each pod's capacity unused.  With L7 per-request routing,
load is distributed evenly across all 8 pods.

<!-- RESULTS-8W-END -->

---

## Comparison

<!-- RESULTS-COMPARE-START -->

All phases: **65 MiB/s lading** (100 parallel connections, Nginx L7 ingress),
pods limited to **1 vCPU / 1 GiB**.

| | Phase 1 (1 pod) | Phase 2 (3 pods) | Phase 3 (8 pods) |
|-|-----------------|------------------|------------------|
| Throughput | 19.04 MiB/s | 55.38 MiB/s | **62.32 MiB/s** |
| Events/s | 149,710 | 435,543 | 490,288 |
| CPU per pod | 1000m (100 %) | ~1000m (99 %) | ~480m (48 %) |
| Bottleneck | Vector CPU | Vector CPU | **None** |
| Scaling vs Phase 1 | 1× | 2.91× | **3.27×** |

The throughput ceiling is reached somewhere between 3 and 8 pods, at exactly
65 / 19 ≈ **3.4 pods**.  Phase 4 confirms this: the HPA converges at 6 pods.

<!-- RESULTS-COMPARE-END -->

---

## Phase 4 — HPA finds equilibrium

With horizontal scaling working and the bottleneck removed, the HPA can now
find the minimum pod count that keeps CPU below the 70 % target.

```bash
# Reset to 1 pod
kubectl scale deployment vector -n vector-perf --replicas=1

# Create HPA (70 % CPU target, 1–8 replicas)
kubectl autoscale deployment vector -n vector-perf \
  --cpu-percent=70 --min=1 --max=8
```

### Phase 4 results

<!-- RESULTS-HPA-START -->

**Scale-up timeline (no manual intervention):**

| Time | Replicas | Avg CPU | Event |
|------|----------|---------|-------|
| t=0 s | **1** | 100 % | load starts |
| t=61 s | **2** | 100 % | HPA scales 1→2 |
| t=91 s | **3** | 99 % | HPA scales 2→3 |
| t=137 s | **5** | 95 % | HPA scales 3→5 |
| t=167 s | **6** | 72 % | HPA scales 5→6 |
| t=182 s | **6** | **63 %** | **Stable, equilibrium** |

Time to equilibrium: **484 s (~8 min)**, 4 scale events, 0 manual cycling.

**Throughput at equilibrium: 62.76 MiB/s, 493,392 ev/s, 6 pods, 63 % avg CPU.**

The HPA settled at 6 pods: at 5 pods CPU reached 83 % (above the 70 % target),
triggering a final scale-up. At 6 pods CPU stabilised at 63 %, within the ±10 %
tolerance band (63–77 %).

<!-- RESULTS-HPA-END -->

---

## Key takeaways

1. **A single pod caps at its CPU limit.**  At 65 MiB/s load, 1 pod can absorb
   only ~19 MiB/s.  Back-pressure prevents any event loss.

2. **L7 per-request routing distributes load uniformly.**  Because Nginx
   dispatches each HTTP request independently, every pod, old or newly
   Ready, receives a share of traffic proportional to the current replica
   count, with no idle pods.

3. **Adding pods beyond the saturation point removes the bottleneck entirely.**
   Phase 3 (8 pods) delivers the full 65 MiB/s with each pod at ~48 % CPU.
   The bottleneck crossover is at ~3.4 pods for this load level.

4. **HPA finds the right pod count automatically.**  With HTTP + L7 routing,
   every new pod starts receiving traffic immediately after becoming Ready.
   HPA converged at 6 pods in 484 s with zero manual intervention.
