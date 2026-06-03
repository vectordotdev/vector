---
sut_path: /home/ssm-user/src/vector
updated: 2026-06-02
external_references:
  - path: lib/vector-buffers/src/variants/disk_v2/mod.rs
    why: Module-level doc comment is the authoritative design spec
  - path: rfcs/2021-10-14-9477-buffer-improvements.md
    why: Original buffer-rework RFC; intended design and guarantees
  - path: docs/specs/buffer.md
    why: Buffer component spec / claimed behavior
  - path: GitHub issues #21683 #24948 #24606 #24144 #23995 #17666 #23456; PRs #23561 #24949
    why: Bug/regression context
---

# Property Catalog: Disk Buffer v2

Thin index over the property files. The 13 live properties each have a
`properties/<slug>.md` evidence file; 20 explored-but-not-selected ones are
digested in `properties/_shelved.md`. Live grind plan `grind-plan.md`, experiment
`experiment-spec.md`, semantics-first claim→reality index
`semantic-claims-ledger.md`.

## Live properties

| Property | Category / cluster | Assertion | Status / verdict | Grind |
|---|---|---|---|---|
| [total-buffer-size-never-underflows](properties/total-buffer-size-never-underflows.md) | Accounting / A | Always≥ (SUT-side, `antithesis` feature) | VIOLATED — #21683 root; moved to in-tree proptest | G2 |
| [writer-eventually-makes-progress](properties/writer-eventually-makes-progress.md) | Liveness / A | compound stall → assert_unreachable | VIOLATED — intermittent deadlock | G2 |
| [record-id-wraparound-accounting-holds](properties/record-id-wraparound-accounting-holds.md) | Accounting / D | Always≥ (SUT-side) | VIOLATED — empty-buffer `0-1`=2^64; moved to proptest | G3 |
| [durable-unacked-events-survive-crash](properties/durable-unacked-events-survive-crash.md) | Crash durability / B | Always (workload) | core guarantee, should hold | G1 |
| [every-written-event-eventually-delivered](properties/every-written-event-eventually-delivered.md) | Crash durability / B | per-event Always(produced⊆delivered) | at-least-once contract | G1 |
| [foreign-data-file-no-writer-stall](properties/foreign-data-file-no-writer-stall.md) | Operational / A | Always(progress after drain) | VIOLATED — non-crash stall, wrong scan scope | G4 |
| [dropped-events-are-counted](properties/dropped-events-are-counted.md) | Delivery / E | Always (workload metric) | VIOLATED — #24606/#24144; logic bug (Bias B1) | G5 |
| [sink-failure-not-silently-acked](properties/sink-failure-not-silently-acked.md) | Delivery / E | Always (workload) | VIOLATED — `_status` discard; logic bug (Bias B1) | G6 |
| [config-reload-no-silent-loss](properties/config-reload-no-silent-loss.md) | Lifecycle / F | Always (workload) | likely VIOLATED — #24948 Drop-no-flush | G7 |
| [fsync-window-bounded-under-clock-jitter](properties/fsync-window-bounded-under-clock-jitter.md) | Operational / G→B | Always | likely VIOLATED under clock faults | G8 |
| [ack-does-not-imply-durability](properties/ack-does-not-imply-durability.md) | Semantic / I (C1) | Always(missing==0) + unreachable loss-magnet | exhibition — expected FAIL is the point | exp-spec |
| [ack-is-per-hop-not-transitive](properties/ack-is-per-hop-not-transitive.md) | Semantic / I (C2) | tail conservation + unreachable | semantic divergence, loss via C1 | exp-spec |
| [delivery-is-at-least-once-not-exactly-once](properties/delivery-is-at-least-once-not-exactly-once.md) | Semantic / I (C11) | set-membership + Sometimes(dup) | NOT a defect — oracle anti-vacuity guard | exp-spec |

## Status rollup

**Currently-violated** (assertions encode the CORRECT invariant so they fire
against current behavior — demonstrations, not regressions to fix here):
`total-buffer-size-never-underflows`, `writer-eventually-makes-progress`,
`sink-failure-not-silently-acked`, `dropped-events-are-counted`,
`record-id-wraparound-accounting-holds` (empty-buffer case),
`foreign-data-file-no-writer-stall`, likely `config-reload-no-silent-loss`, likely
`fsync-window-bounded-under-clock-jitter` (under clock faults).

**Underflow cluster descoped to proptest** (grind-plan pivot 2026-06-02):
`total-buffer-size-never-underflows` and `record-id-wraparound-accounting-holds`
are deterministically reproducible in-process, so they become in-tree
`lib/vector-buffers` `proptest` tests; Antithesis keeps the distributed work (the
`vector_to_vector_e2e_disk` conservation experiment + #24948 SIGHUP reload). The
SUT-side `assert_always_greater_than_or_equal_to!` detectors stay committed under
the `antithesis` feature at `get_total_records`, `decrement_total_buffer_size`, and
the reader size-delta site — detectors not guards (they report the wrap, the
subtraction still runs).

**Release-build note:** observing the wrap requires a release build — the debug
`trace!` in `decrement_total_buffer_size` evaluates the wrapping subtraction and
panics before the release-mode wrap is observable.

## Workload / exerciser mapping

`disk_v2_lossfinder` (`lib/vector-buffers/examples/disk_v2_lossfinder.rs`)
implements a no-silent-loss oracle across a 7-scenario RNG fault menu, covering:
`every-written-event-eventually-delivered` (Baseline),
`config-reload-no-silent-loss`/#24948 (WriterDropNoFlush),
`sink-failure-not-silently-acked` (RejectDeliveries — already surfaced the
finalizer status-discard loss locally), `durable-unacked-events-survive-crash`
(CrashReopen), `dropped-events-are-counted`/#24606 (DropNewestOverfill —
reachability only, metric oracle TODO), and the shelved corruption-skip trio
(Corruption/TruncateTail). The earlier `disk_v2_antithesis` exerciser covers
the #21683 accounting-underflow cluster (reproduced in run D0). Both are
demonstrations: assertions encode the CORRECT no-loss invariant so they fire
against current behavior.

## File-level open questions

- Config-reload (G7) and sink-error injection (G6) need **custom faults** — SIGHUP
  is wired and `snouty validate`-green; sink-error is workload-driven.
- `(needs human input)`: is the finalizer's `BatchStatus` discard intentional
  (retry assumed upstream) or a bug? Is **filesystem-fault injection** (truncate
  `buffer.db`/`.dat`) available in the tenant? (Gates the shelved corruption and
  ledger rows.)
- Does Vector topology call `writer.flush()` on graceful shutdown, and does tokio
  drain the detached finalizer before exit? (`Runtime::drop` has a ~2s window;
  `running.rs` stop drop-order vs. final flush is unresolved.)
- **Build profile:** release for underflow/wrap properties; test build (or a runtime
  `MAX_FILE_ID` knob) for the shelved `file-id-rollover-stays-coordinated`.
