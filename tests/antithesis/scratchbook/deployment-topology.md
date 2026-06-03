---
sut_path: /home/ssm-user/src/vector
commit: 2dae1f421
updated: 2026-06-03
---

# Deployment Topology: Disk Buffer v2 — design rationale

The shipped `tests/antithesis/scenarios/vector_to_vector_e2e_disk/`
(docker-compose.yaml + head.yaml + tail.yaml + README.md) is the **source of
truth** for ports, volumes, sizes, and the env knob. This stub keeps only the
rationale not captured there. (Cross-reference values: `head` is an `http_server`
source on 8080 → `vector` sink to `tail:6000`; `tail` is a `vector` source →
`http` sink to the oracle on 8686; both buffers `when_full: block`, `max_size:
8388608` (8 MiB), metrics on 9598, per-node persistent volume at `/var/lib/vector`,
`VECTOR_DISK_V2_MAX_DATA_FILE_SIZE=2097152` (2 MiB) to force rotation.)

## Why each node is its own pod

Antithesis faults are **pod-level** (`environment/fault_injection`): two processes
in one pod never see network faults between them. To make the inter-node `vector`
links partitionable — the point of the conservation chain — each Vector node must
be its own container/pod. Co-locating two silently disables the partition lever.
(The disk buffer is single-process, so partitions never touch it directly; the
chain shape turns a partition into buffer pressure on a `block`-mode buffer.)

## Start small, earn the ring

Shipped **N=2, no loop** (head → tail → oracle collector): a real conservation
oracle, two buffer crossings, no self-deadlock risk. The **ring** (close tail→head,
add lap-drain) is a stress upgrade to earn once the chain is green — the lap-drain
keeps a `block`-mode ring from self-deadlocking on legitimate backpressure (a false
positive). Still absent: an `assert_sometimes!` proving a writer actually parked on
a full `block`-mode buffer, confirming the partition-fills-buffer lever is
exercised, not just configured.

## Persistent volume is mandatory

Each node's `data_dir` must survive node-termination restart. If a modeled crash
recreates the container with a fresh filesystem, that hop's buffer is wiped and
every crash-recovery property passes vacuously (or fails spuriously). Confirm how
the tenant's node-termination interacts with filesystem persistence.

## Open questions

- e2e-ack transitivity across `vector` hops: does a head ack mean durable-to-tail
  or durable-at-hop-0? (see `properties/ack-is-per-hop-not-transitive.md`.)
- Does mid-chain `block` backpressure propagate to the head injector, or get
  absorbed by an upstream source buffer?
- Quiescence signal for the `eventually_` oracle: every node's buffer gauge ~0 AND
  collector count stable for K seconds (not a fixed sleep).
- Node-termination and clock faults enabled in the target tenant? Nearly every
  crash-class property needs them.
