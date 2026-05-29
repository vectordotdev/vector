---
sut_path: /home/ssm-user/src/vector
commit: 049eec79b737450c4669b7f8aa1dd814551ec466
updated: 2026-06-02
external_references:
  - path: lib/vector-buffers/src/variants/disk_v2/mod.rs
    why: Module-level doc comment is the authoritative design spec
  - path: rfcs/2021-10-14-9477-buffer-improvements.md
    why: Original buffer-rework RFC
  - path: docs/specs/buffer.md
    why: Buffer component spec
  - path: (internal design doc, not linked)
    why: fsync window, ack flow, at-least-once semantics
  - path: (internal design doc, not linked)
    why: Root-cause writeups of #21683, #24948, #24606
  - path: (internal design doc, not linked)
    why: Existing chaos test + lock-contention issue
  - path: GitHub issues #21683 #24948 #24606 #24144 #23995 #17666 #23456; PRs #23561 #24949
    why: Bug/regression context
---

# Property Relationships

Clusters of related properties, suspected dominance, and shared
faults/code-paths. Lightweight — connections noticed during synthesis.

## Cluster A — The `total_buffer_size` underflow / writer deadlock (the master cluster)

The single unguarded-subtraction bug radiates into many properties.

- **Root:** `total-buffer-size-never-underflows` (committed `assert_always_greater_than_or_equal_to!`
  detectors at the subtraction sites: `ledger.rs:313` and `reader.rs:529`, under
  the `antithesis` feature).
- **Direct manifestation:** `writer-eventually-makes-progress` (the deadlock the
  underflow causes).
- **Dominated / aliased symptoms** (likely the *same* failure observed elsewhere):
  - `reader-drains-and-terminates-cleanly` — `total_buffer_size == 0` is the
    termination condition; if it wrapped, the reader also never terminates.
  - `buffer-size-within-max` — the deadlock makes this **vacuously pass** (no
    writes → no overflow); must be read jointly with `writer-eventually-makes-progress`.
  - `acked-files-eventually-deleted` — the delete-time `metadata.len() - bytes_read`
    (`reader.rs:535`, watched at `reader.rs:529`) is one of the two underflow triggers, and stalled deletion is
    also an upstream *cause* of the full-buffer state.

**Dominance:** if `total-buffer-size-never-underflows` holds, the deadlock-shaped
failures of `writer-eventually-makes-progress`, `reader-drains-and-terminates-cleanly`,
and the vacuity of `buffer-size-within-max` largely disappear. But keep all four:
they observe the bug from different vantage points (SUT-side root, writer liveness,
reader termination, safety-bound vacuity) and Antithesis benefits from multiple
independent assertions on the same dangerous state.

## Cluster B — Crash-time durability & recovery (shared fault: node-kill in the fsync window)

All depend on node-termination faults and the 500ms fsync window + non-atomic
data-fsync/ledger-msync pair.

- `durable-unacked-events-survive-crash` (synced data not lost)
- `every-written-event-eventually-delivered` (end-to-end at-least-once — the
  product-level expression; **dominates** `durable-unacked-events-survive-crash`
  at the workload level, but the latter is the tighter buffer-internal invariant)
- `recovery-completes-after-crash` (init doesn't hang/fail)
- `partial-write-at-rotation-recovers` (torn-tail / rotation crash recovery)
- `multi-hop-conservation-no-loss` (at-least-once **composed across N nodes** —
  the cross-process generalization of `every-written-event-eventually-delivered`)

**Connections:** `partial-write-at-rotation-recovers` is the mechanism by which
`recovery-completes-after-crash` and `durable-unacked-events-survive-crash` can
fail (torn tail → wrong fast-forward → silent skip or monotonicity panic). It also
feeds Cluster A: a partial write is the canonical producer of the file-size vs.
record-bytes discrepancy that triggers the underflow.

`multi-hop-conservation-no-loss` is the **bridge between Cluster B and Cluster A**:
it is a delivery/at-least-once property (B) whose distinctive value is that, in a
multi-pod chain, inter-node network **partitions** drive a buffer to full under
`when_full: block` — organically reproducing the full-buffer backpressure state
where the Cluster-A underflow/deadlock lives, without a synthetic overfill
workload. It strictly dominates `every-written-event-eventually-delivered` on
coverage (N buffers crossed, N independent fault timings) but is harder to make
quiescent (needs a lap-drain in the ring form to avoid `block`-mode
self-deadlock false positives). Keep both: the single-hop property is the
cheaper, always-quiescent baseline.

## Cluster C — Corruption detection & record integrity (shared fault: bit-flip / partial-write)

- `no-corrupted-record-delivered` (never return garbage)
- `corruption-is-detected-and-recovered` (detection path fires)
- `record-id-monotonicity-holds` (the guardrail panic never trips)
- `record-never-spans-files` (record framing integrity)

**Connections:** `no-corrupted-record-delivered` and `corruption-is-detected-and-recovered`
are two sides of the same CRC32C/`CheckBytes` guard — the former asserts "never
bad output," the latter asserts "the recovery branch is reachable." Neither
dominates the other. `record-id-monotonicity-holds` bridges to Cluster B
(torn-tail) and to `file-id-rollover-stays-coordinated` (the `reader.rs:932` `>`
bug is a path to a monotonicity violation). `partial-write-at-rotation-recovers`
(Cluster B) and this cluster share the torn-tail evidence.

## Cluster D — Boundary arithmetic (shared mechanism: non-wrap-aware / unguarded integer ops)

- `file-id-rollover-stays-coordinated` (`reader.rs:932` raw `u16 >`)
- `record-id-wraparound-accounting-holds` (`ledger.rs:281` non-wrapping `- 1`, watched at `ledger.rs:271`)
- (related: `total-buffer-size-never-underflows` is also an arithmetic-safety bug,
  but its blast radius puts it in Cluster A)

**Connections:** all three are the same *class* of defect (Rust integer ops that
aren't saturating/wrap-aware in a context that can hit the boundary). They share
no runtime code path but share a root-cause pattern and a fix pattern — worth a
single "audit all buffer integer arithmetic for boundary safety" note to the team.
`file-id-rollover-stays-coordinated` connects to Cluster C via the monotonicity
panic.

## Cluster E — Delivery-accounting bugs (silent loss invisible to operators)

- `sink-failure-not-silently-acked` (`_status` discarded at `ledger.rs:731`)
- `dropped-events-are-counted` (`drop_newest` not surfaced to component metric)
- (related lifecycle: `config-reload-no-silent-loss`)

**Connections:** these are "the system loses data but the metrics don't say so"
properties — the internal config-reload incident theme. They don't share code paths with each other
but share the *observability gap* failure mode. `config-reload-no-silent-loss`
(Cluster F) is the most severe instance of the same theme.

## Cluster F — Lifecycle / shutdown flush (shared mechanism: `Drop` does not flush)

- `config-reload-no-silent-loss`
- `graceful-shutdown-flushes-all`

**Connections:** both hinge on the same fact — `BufferWriter::Drop` calls `close()`
but not `flush()`, so any guarantee of losslessness on teardown depends on an
*external* flush by the topology before drop. `graceful-shutdown-flushes-all` is
the steady-state version; `config-reload-no-silent-loss` is the hot-reload version
(harder, because old+new topologies may overlap and contend on the per-process
advisory lock). Resolving the open question "does the topology flush before drop?"
affects both.

## Cluster G — Cross-cutting & operational gaps (Category 7, from evaluation)

New properties spanning environment/operations. Several connect back to the
master deadlock cluster (A) and the silent-loss cluster (E).

- `foreign-data-file-no-writer-stall` — a **non-crash** path to the same stall as
  Cluster A (`is_buffer_full` permanently true), but via wrong scan scope in
  `update_buffer_size` rather than arithmetic underflow. **Joins Cluster A** as an
  alternative trigger; reachable without node-kill faults.
- `finalizer-task-drains-pending-acks` — a **distinct** stall/loss path: a dead
  finalizer strands acks. **Bridges Cluster A** (no acks → no deletion → writer
  stall) and **Cluster E** (delivered events never marked acked → silent loss).
  Also relates to `acked-files-eventually-deleted` (same dependency chain) and
  `reader-drains-and-terminates-cleanly` (a dead finalizer prevents clean
  termination).
- `ledger-corruption-no-sigbus-crashloop` — external-tampering analogue of the
  corruption cluster (C), but on the *ledger* mmap rather than data records;
  failure mode is SIGBUS/crash-loop, not bad-data delivery.
- `fsync-window-bounded-under-clock-jitter` — directly strengthens **Cluster B**:
  it bounds the durability window that `durable-unacked-events-survive-crash`
  assumes (≤500ms). The shared `flush_interval=0` oracle decision ties them
  together.
- `overflow-chain-no-unaccounted-gap` — a **Cluster E** silent-loss instance for
  the previously-uncovered `WhenFull::Overflow` mode; also touches Cluster B
  (crash) and can feed Cluster A (overflow/drain cycles confusing
  `total_buffer_size`).
- `buffer-survives-version-upgrade` — connects to corruption cluster (C): a
  layout-changed record passing `CheckBytes`+CRC is the same "garbage delivered as
  valid" failure as `no-corrupted-record-delivered`, reached via upgrade rather
  than bit-flip; can also trigger the monotonicity panic (Cluster C).
- `throughput-progresses-under-contention` — the **degenerate-but-alive** companion
  to `writer-eventually-makes-progress` (A): together they triangulate healthy vs.
  starved vs. deadlocked. Otherwise standalone (perf/contention).

## Cluster H — Checksum-skip silent data loss (added 2026-05-29, data-loss expansion)

Shared mechanism: `roll_to_next_data_file` (reader.rs:711-759) abandons the
entire remainder of a data file on the first bad read. Three new properties make
the loss precise where Cluster C only confirmed the recovery path runs:

- `corruption-skip-loss-bounded` — bounds *how much* is lost (valid records after
  the corrupt one survive). The `Always` safety bound that
  `corruption-is-detected-and-recovered` (Cluster C, a `Sometimes` reachability
  check) does not provide. **Tightens Cluster C.**
- `corruption-skip-loss-is-counted` — the abandoned records must be counted.
  **Joins Cluster E** (silent-loss invisible to operators): it is the read-side
  companion to `dropped-events-are-counted`/#24606 — strictly more silent (hits
  neither buffer- nor component-level counter).
- `corruption-skip-record-id-accounting-consistent` — the roll must not turn loss
  into accounting corruption. **Bridges Cluster A** (names the abandoned-tail as a
  concrete real trigger for the reader.rs:535 underflow (watched at reader.rs:529) → #21683, validated-as-
  reachable by run D0) and **Cluster D/C** (record-ID gap → monotonicity panic).

Dominance: `corruption-skip-loss-bounded` is the precondition (loss must be real);
the other two describe *consequences* (silent + accounting-corrupting). The single
fault — a mid-file bit-flip in a multi-record data file, read live — exercises all
three plus Cluster C's reachability check.

## Cluster I — Semantic claim/reality divergences (doctrine pass, 2026-06-02)

Claim-FIRST re-cut of the durability story (see `semantic-claims-ledger.md`). These
are not a new failure mechanism — they name, at the level of *what people believe*,
the gaps the mechanism clusters already encode, and pin the two foundational ones
the 2026-06-02 code investigation grounded.

- `ack-does-not-imply-durability` (C1) — the 200 fires at encode, before fsync
  (`writer.rs:472`). **Root of Cluster F** (Drop-no-flush) at the semantic level and
  the property the launched run exhibits; the *consequence* (lost acked data) is the
  same loss `config-reload-no-silent-loss` (F) and `durable-unacked-events-survive-crash`
  (B) measure mechanism-first.
- `ack-is-per-hop-not-transitive` (C2) — the disk buffer mints a fresh ack chain
  (`reader.rs:1117-1119`). **Resolves `multi-hop-conservation-no-loss` (B) OQ#1** and
  is the reason the tail collector is the sole delivery truth. Directly falsifies the
  "chain of durable nodes ⇒ no loss" reasoning.
- `delivery-is-at-least-once-not-exactly-once` (C11) — **not a defect**; constrains
  the oracle (sets not order/equality; duplicates as anti-vacuity). Guards every
  delivery property in B against the false-red of treating a legal duplicate as loss.

**Dominance:** C1 dominates the whole exhibition — if a 200 were truly durable,
C2/C6/C10 losses could not occur. C1+C2 together are the semantic statement of the
mission; Clusters A/B/E/F are the mechanism-level evidence and exhibits for them.

## Cross-cluster dominance summary

- **`total-buffer-size-never-underflows` (A)** is the highest-leverage single
  property: fixing/verifying it neutralizes the deadlock symptoms across A.
- **Checksum-skip cluster (H)** is the highest-leverage *data-loss* entry point:
  one mid-file-corruption fault simultaneously exercises bounded-loss (H),
  silent-loss counting (E/H), the reader.rs:535 underflow (A, watched at reader.rs:529), and the
  monotonicity guard (C/D).
- **`partial-write-at-rotation-recovers` (B)** is the most connected *trigger*: it
  feeds A (underflow), B (recovery), and C (torn-tail/monotonicity). Antithesis
  effort spent reaching the rotation-boundary crash window pays off across three
  clusters.
- **Observability-gap properties (E, F)** are independent of the durability/deadlock
  machinery and need their own workload setup (metric inspection, sink-error
  injection, config reload) — don't expect the crash-recovery workload to cover them.
