---
sut_path: /home/ssm-user/src/vector
updated: 2026-06-02
external_references:
  - path: lib/vector-buffers/src/variants/disk_v2/mod.rs
    why: Module-level doc comment is the authoritative design spec
  - path: GitHub issues #21683 #24948 #24606 #24144 #23995 #17666 #23456; PRs #23561 #24949
    why: Bug/regression context
---

# Property Relationships

Cross-cluster dominance and aliasing. One line per cluster: which properties are
the *same bug* from different vantage points, which faults hit several at once.
Per-property mechanism lives in `properties/<slug>.md` and `_shelved.md`. Slugs not
in the live set point to `properties/_shelved.md`.

## Cluster A — `total_buffer_size` underflow / writer deadlock (master)

One unguarded subtraction, everything else the same failure elsewhere. Root
`total-buffer-size-never-underflows`; manifestation
`writer-eventually-makes-progress`; aliased symptoms
`reader-drains-and-terminates-cleanly` (termination uses `total_buffer_size==0`),
`buffer-size-within-max` (holds *vacuously* under the deadlock),
`acked-files-eventually-deleted` (delete-time size-delta is one of two underflow
triggers). Verifying the root neutralizes the deadlock-shaped failures, but keep
multiple independent assertions on the same dangerous state. **The
accounting-underflow invariants moved OUT to in-tree `proptest`** (grind-plan
pivot) — deterministically reproducible in-process, where Antithesis adds only
scheduler/coverage.

## Cluster B — crash-time durability & recovery (shared fault: node-kill in the fsync window)

`durable-unacked-events-survive-crash`, `every-written-event-eventually-delivered`
(product-level, dominates the buffer-internal one at workload level),
`recovery-completes-after-crash`, `partial-write-at-rotation-recovers`,
`multi-hop-conservation-no-loss` (cross-process generalization).
`partial-write-at-rotation-recovers` is the mechanism the others fail by (torn tail
→ wrong fast-forward → skip or panic) and feeds A (a partial write is the canonical
file-size vs. record-bytes discrepancy). `multi-hop-conservation-no-loss` bridges
B→A: inter-node partitions drive a buffer full under `when_full: block`, organically
reaching the Cluster-A state.

## Cluster C — corruption detection & record integrity (shared fault: bit-flip / partial-write)

`no-corrupted-record-delivered` (never bad output) and
`corruption-is-detected-and-recovered` (recovery branch reachable) are two sides of
the same CRC32C/`CheckBytes` guard, neither dominates.
`record-id-monotonicity-holds` bridges to B (torn-tail) and D (the file-id rollover
`>` bug is a path to a monotonicity violation). `record-never-spans-files` is
framing integrity.

## Cluster D — boundary arithmetic (shared mechanism: non-wrap-aware / unguarded integer ops)

`file-id-rollover-stays-coordinated`, `record-id-wraparound-accounting-holds`, and
(by blast radius, filed under A) `total-buffer-size-never-underflows` are the same
defect *class* — shared root cause and fix pattern, not a runtime path. Worth one
"audit all buffer integer arithmetic for boundary safety" note.

## Cluster E — delivery-accounting bugs (silent loss invisible to operators)

`sink-failure-not-silently-acked`, `dropped-events-are-counted`, and (lifecycle)
`config-reload-no-silent-loss` share the "system loses data but metrics don't say
so" failure mode — no shared code path, shared observability gap.

## Cluster F — lifecycle / shutdown flush (shared mechanism: `BufferWriter::Drop` does not flush)

`config-reload-no-silent-loss` (hot-reload, harder — old+new topology contend the
per-process advisory lock) and `graceful-shutdown-flushes-all` (steady-state) both
hinge on whether the topology flushes before drop.

## Cluster G — cross-cutting & operational gaps

`foreign-data-file-no-writer-stall` is a **non-crash** path into A (wrong scan
scope, not arithmetic); `finalizer-task-drains-pending-acks` bridges A (no acks → no
deletion → stall) and E (delivered-not-acked → silent loss);
`ledger-corruption-no-sigbus-crashloop` is the ledger-mmap analogue of C;
`fsync-window-bounded-under-clock-jitter` strengthens B (bounds the 500ms window B
assumes); `overflow-chain-no-unaccounted-gap` is an E silent-loss instance for the
uncovered `WhenFull::Overflow` mode; `buffer-survives-version-upgrade` reaches C's
"garbage delivered as valid" via upgrade; `throughput-progresses-under-contention`
is the degenerate-but-alive companion to `writer-eventually-makes-progress`.

## Cluster H — checksum-skip silent data loss

Shared mechanism: `roll_to_next_data_file` abandons the whole file tail on the first
bad read. `corruption-skip-loss-bounded` (how much is lost — tightens C's
reachability check), `corruption-skip-loss-is-counted` (joins E — read-side
companion to #24606, strictly more silent),
`corruption-skip-record-id-accounting-consistent` (bridges A — the abandoned tail is
a real underflow trigger). One mid-file bit-flip in a multi-record file exercises
all three plus C's reachability.

## Cluster I — semantic claim/reality divergences (doctrine, see `semantic-claims-ledger.md`)

Claim-first re-cut, not a new mechanism. `ack-does-not-imply-durability` (C1, root
of F at the semantic level, the launched-run exhibition),
`ack-is-per-hop-not-transitive` (C2, resolves `multi-hop-conservation-no-loss` OQ#1
— acks are NOT transitive), `delivery-is-at-least-once-not-exactly-once` (C11, not a
defect — constrains the oracle to sets-not-order, duplicates as anti-vacuity).

## Cross-cluster dominance summary

- `total-buffer-size-never-underflows` (A) is the highest-leverage single property.
- Checksum-skip (H) is the highest-leverage *data-loss* entry: one mid-file
  corruption fault hits bounded-loss (H), silent-loss counting (E/H), the reader
  underflow (A), and the monotonicity guard (C/D) at once.
- `partial-write-at-rotation-recovers` (B) is the most connected *trigger* — feeds
  A, B, and C.
- C1 dominates the semantic exhibition: a truly durable 200 makes the C2/C6/C10
  losses impossible.
- Observability-gap properties (E, F) are independent of the durability/deadlock
  machinery and need their own workload setup.
