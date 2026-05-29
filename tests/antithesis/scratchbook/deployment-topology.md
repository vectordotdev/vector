---
sut_path: /home/ssm-user/src/vector
commit: a202ea3e1be8ea946d60f9e9fd0d9d4245bcb140
updated: 2026-06-01
external_references:
  - path: lib/vector-buffers/src/variants/disk_v2/mod.rs
    why: Confirms the buffer is single-process (intra-Vector reader+writer over an mmap'd ledger)
  - path: https://antithesis.com/docs/environment/fault_injection
    why: Faults are pod-level; node-termination "may lose all modified data and boot fresh from image" — drives one-pod-per-node + persistent-volume-per-node
  - path: (internal design doc, not linked)
    why: Disk buffer is configured per-sink; e2e acks require a supporting source; at-least-once semantics
  - path: (internal design doc, not linked)
    why: Existing chaos test crashes the worker with SIGKILL x3 + e2e acks — the topology must support repeated kill/restart
  - path: distribution/docker/
    why: Existing Vector Dockerfiles to reuse/adapt for the SUT container
---

# Deployment Topology: Disk Buffer v2

## Key fact driving the design

The disk buffer is **single-process**: the reader, writer, and finalizer all run
inside one Vector process, coordinating through an `mmap`'d ledger and the local
filesystem. There is **no network, no peer, no quorum**. Therefore:

- The strong fault levers are **node termination (kill/restart)**, **node hang**,
  **CPU throttling**, **clock jitter**, and **filesystem state across restart** —
  NOT network partitions or bad-node faults (those are irrelevant to the buffer).
- The topology is minimal: **one SUT container + one workload/client container.**
  No dependency containers are needed (no S3/Kafka/Postgres) — the buffer's only
  "dependency" is the local filesystem.

## Topology

```text
+-----------------------------+         events (HTTP, e2e-ack-capable source)
|  workload (client)          |  ----------------------------------------->  +-----------------------------+
|  - produces unique event IDs|                                              |  vector (SUT)               |
|  - HTTP collector endpoint  |  <-----------------------------------------  |  source -> sink(disk buffer)|
|  - tracks produced/delivered|         sink delivers here (HTTP sink)        |  data_dir on PERSISTENT vol |
|  - emits Antithesis asserts |                                              +-----------------------------+
|  - test template /opt/...   |                                                      |  Antithesis injects
+-----------------------------+                                                      |  node-kill / hang /
                                                                                     |  CPU-throttle / clock
                                                                                     v  faults HERE
                                                                            +-----------------------------+
                                                                            | persistent volume           |
                                                                            | <data_dir>/buffer/v2/<id>/  |
                                                                            +-----------------------------+
```

## Containers

### 1. `vector` — Service (the SUT)

- **Image:** adapt an existing Dockerfile from `distribution/docker/` (Debian or
  Distroless). Two build variants:
  - **Baseline build:** stock Vector — exercises all workload-observable
    properties (durability, at-least-once, deadlock-via-throughput-stall, metric
    correctness, recovery).
  - **Instrumented build (recommended for the deadlock/corruption cluster):**
    Vector built with the **Antithesis Rust SDK** added as a dependency to
    `lib/vector-buffers`, with the missing SUT-side assertions inserted (see
    "SUT-side instrumentation" below). This is the only way to directly assert
    the internal states (`total-buffer-size-never-underflows`,
    `record-id-monotonicity-holds`, `partial-write-at-rotation-recovers`,
    `graceful-shutdown-flushes-all`/`unflushed_bytes==0`) that are invisible from
    the workload.
- **Runs:** a single `vector` process with a config:
  - `source`: an e2e-ack-capable source the workload can push to. Prefer
    `datadog_agent` or `http_server` with `acknowledgements: true` (needed for
    `every-written-event-eventually-delivered` and the durable-survival
    properties). Keep one source.
  - `sink`: an `http` sink with `buffer: { type: disk, max_size: <~256MB+>,
    when_full: block }`, posting to the workload's collector. A second
    config/run uses `when_full: drop_newest` for `dropped-events-are-counted`.
  - Internal metrics exposed (e.g. `internal_metrics` → `prometheus_exporter`)
    so the workload can read `buffer_*` / `component_discarded_events_total` for
    the metric-correctness properties.
- **CRITICAL — persistent buffer storage:** the disk-buffer `data_dir` MUST be on
  storage that **survives the container's kill/restart**. Disk-buffer durability
  is the whole point; if Antithesis node-termination recreates the container with
  a fresh filesystem, the buffer is wiped and every crash-recovery property
  passes vacuously (or fails spuriously). Mount `<data_dir>` on a persistent
  volume. **Confirm with the user how their tenant's node-termination interacts
  with filesystem persistence.**
- **Faults target this container:** node kill/restart (required by Categories
  2–6), node hang, CPU throttle (widens fsync/lock-contention windows), clock
  jitter (perturbs the 500ms `should_flush` deadline).
- **Replica count:** 1. (No replication; more instances add nothing.)
- **Tuning for bug-finding:** set a small `max_data_file_size` (e.g. 1MB) and a
  small `max_size` to maximize file-rotation frequency and reach the rotation/
  partial-write window faster; optionally set `flush_interval` low to widen the
  durably-written set, or high to widen the loss window — test both.

### 2. `workload` — Client (the test driver)

- **Image:** a small Rust (or Go) container with the **Antithesis Rust SDK** (to
  match the SUT language and emit assertions). Includes the test template at
  `/opt/antithesis/test/v1/{name}/`.
- **Runs:**
  1. Starts an HTTP **collector** endpoint (the sink's destination) that records
     every delivered event ID (counting duplicates).
  2. Emits `setup_complete` once it and Vector are ready.
  3. Sleeps so Antithesis can run test-template commands.
- **Test-template commands** drive: produce a stream of uniquely-IDed events to
  Vector's source; periodically (via `ANTITHESIS_STOP_FAULTS` quiet periods)
  drain and assert liveness/at-least-once; inspect Vector's metrics; toggle the
  collector to return errors (for `sink-failure-not-silently-acked`); trigger a
  config reload (custom fault, for `config-reload-no-silent-loss`).
- **Assertions emitted here** (workload-observable properties): at-least-once
  set-difference, no-loss-on-graceful-shutdown, drop accounting vs metric, writer
  throughput resumes after recovery (deadlock signal), buffer gauges return to ~0
  on drained restart.
- **Replica count:** 1.

## SUT-side instrumentation (for the instrumented build)

No Antithesis SDK exists in the repo today (`existing-assertions.md`). For the
internal-state properties, add `antithesis-sdk` to `lib/vector-buffers/Cargo.toml`
and insert (all currently MISSING):

- `assert_unreachable!` / `assert_always!(amount <= current)` at the two unguarded
  subtraction sites: `ledger.rs:~292` and `reader.rs:~524`
  (`total-buffer-size-never-underflows`).
- `assert_sometimes!(writer_unblocked_after_full)` after `ensure_ready_for_write`
  exits its wait loop; `assert_unreachable!` on repeated no-progress wakeups
  (`writer-eventually-makes-progress`).
- `assert_unreachable!` at the monotonicity panic `reader.rs:~482`
  (`record-id-monotonicity-holds`).
- `assert_always_or_unreachable!` at the record-emission point `reader.rs:~1131`
  (`no-corrupted-record-delivered`) and `assert_sometimes!` in the
  `is_bad_read` branch `reader.rs:~1035` (`corruption-is-detected-and-recovered`).
- `assert_sometimes!(torn_tail_recovered)` in the `validate_last_write`
  recovery branches (`partial-write-at-rotation-recovers`).
- `assert_always!(unflushed_bytes == 0)` inside `close()`
  (`graceful-shutdown-flushes-all`).

These assertions are no-ops outside Antithesis, so the instrumented build is safe
to run normally.

## Custom faults required

- **Config reload** (`config-reload-no-silent-loss`): a custom fault that sends
  `SIGHUP` to the Vector process (or swaps the config file and triggers reload),
  fired under sustained load.
- **Downstream sink error** (`sink-failure-not-silently-acked`): the workload's
  collector returns 5xx for a window, or a custom fault toggles it.

## SDKs

- **Workload:** Antithesis Rust SDK (or Go SDK) — required to emit assertions and
  `setup_complete`, and to draw random numbers for the producer.
- **SUT:** Antithesis Rust SDK only for the instrumented build.

## Simplicity note

Two containers, one network link, no external dependency services. Every
container is justified: the SUT runs the buffer; the workload produces/observes
and asserts. We deliberately exclude S3/Kafka/etc. — the disk buffer has no such
dependency. The only non-obvious requirement is the **persistent volume for the
buffer data_dir**, which is essential for crash-durability testing to be
meaningful.

## Open Questions

- How does the target Antithesis tenant's node-termination fault interact with
  container filesystem persistence? (Determines whether the buffer survives a
  modeled crash — essential.)
- Are node-termination and clock faults enabled in the tenant? (Categories 2–6
  need kill/restart.)
- Which e2e-ack-capable source is easiest to drive from the workload —
  `http_server`, `datadog_agent`, or `socket`? (Affects workload protocol.)
- Is config reload feasible as a custom fault (SIGHUP) in the harness, or must the
  workload drive it via Vector's API?

---

## Topology B — Multi-node conservation chain (for `multi-hop-conservation-no-loss`)

The single-SUT topology above is intra-process: network faults never touch the
buffer. The **conservation chain** the user asked for ("what goes in comes back
out") is a deliberately different shape that turns network faults into buffer
pressure.

```text
 head injector (parallel_driver xN)                                tail collector
 mints unique ids, logs ACKED  --HTTP-->  [node_0]  --vector proto-->  [node_1]
                                            src                          src
                                            |                            |
                                          disk_v2                      disk_v2
                                          block+acks                   block+acks
                                            |                            |
                                          vsink ----vector proto--------> ...
                                                                          |
                                       [node_{N-1}] --HTTP--> collector (logs DELIVERED)
   ^                                                                      |
   |  (RING UPGRADE, later) node_{N-1}.sink --vector--> node_0.source, with a
   +----------------------- lap-counter remap+route at node_0 that exits a record
                            to the collector after one lap, else forwards it.
```

### Why each node is its own pod

Antithesis faults are **pod-level** (`environment/fault_injection`): two
processes in one pod never see network faults between them. To make the
inter-node `vector` links partitionable — the whole point of the chain — **each
Vector node must be its own container/pod.** Co-locating two nodes would silently
disable the new fault lever.

### Containers

- **`node_0` … `node_{N-1}`** — N copies of the **same instrumented Vector
  image** from Topology A. Config per node:
  - `source`: `vector` (native protocol, `acknowledgements: true`). `node_0`
    additionally takes an `http_server` source for head injection (or the
    injector speaks the `vector` protocol directly — TBD by OQ-1 / ack
    semantics).
  - `sink`: `vector` sink → next node's `vector` source, `buffer: { type: disk,
    when_full: block, max_size: … }`, `acknowledgements` wired through. The tail
    node's sink is `http` → collector.
  - Per-node **persistent volume** for `data_dir` (mandatory — fresh-boot
    restart otherwise wipes that hop's buffer → spurious loss).
  - Small `max_data_file_size` to force rotation; same Cluster-A/B SUT-side
    asserts as Topology A (they fire on whichever node hits the bad state).
- **`workload`** — head injector (`parallel_driver_`, N concurrent) + tail
  collector (long-lived `serve`) + `eventually_` drain-and-check oracle. Same
  container/SDK as Topology A; reuses `vdbuf-workload`'s produce + collector
  modes. Globally-unique ids, shared-volume `ACKED`/`DELIVERED` logs, dedup at
  collector.

### Faults

Node-kill/restart per node (independent), **partition between adjacent node
pods** (the new lever — fills a buffer under `block`), CPU throttle, clock
jitter. Each node killed/partitioned independently maximizes the cross-hop loss
surface.

### Start small

Ship **N=3, no loop** first (chain → collector): real conservation oracle, three
buffer crossings, no self-deadlock risk. Earn the **ring** (close tail→head +
lap-drain) as a stress upgrade once the chain is green — the lap-drain is
required to keep a `block`-mode ring from self-deadlocking on legitimate
backpressure (a false positive). See `properties/multi-hop-conservation-no-loss.md`.

### New open questions (Topology B)

- e2e-ack transitivity across `vector` hops (OQ-1 in the property file) — decides
  whether the head ack means durable-to-tail or durable-at-hop-0.
- Does mid-chain `block` backpressure propagate to the head injector, or get
  absorbed by an upstream source buffer?
- Quiescence signal for the `eventually_` oracle: every node's buffer gauge ~0
  AND collector count stable for K seconds (not a fixed sleep).
</content>
