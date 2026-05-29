---
sut_path: /home/ssm-user/src/vector
commit: 049eec79b737450c4669b7f8aa1dd814551ec466
updated: 2026-06-02
external_references:
  - path: lib/vector-buffers/
    why: Scanned the crate (and whole repo) for Antithesis SDK imports and assertion calls
  - path: tests/antithesis/scenarios/vector_to_vector_e2e_disk/
    why: The committed harness now carries workload-side and oracle-side assertions
  - path: https://github.com/vectordotdev/vector/issues/21683
    why: The total_buffer_size underflow / writer-deadlock bug the SUT-side assertions target
---

# Existing Antithesis SDK Assertions

## Summary

**Instrumentation now exists.** An earlier pass recorded zero Antithesis SDK
usage anywhere in the repo. That is no longer true: the `blt/antithesis-research`
branch committed both SUT-side assertions (in `lib/vector-buffers`, behind the
`antithesis` feature) and a full harness of workload/oracle assertions (in
`tests/antithesis/scenarios/vector_to_vector_e2e_disk`). This file is the
reconciled inventory at commit `049eec79b`.

Every assertion below is the Antithesis SDK macro form. Per the SDK, these
report to Antithesis and **do not panic** — they are detectors, not guards. The
SUT-side ones are compiled only under `--features antithesis`; production builds
are unchanged.

## SUT-side assertions (`lib/vector-buffers`, `#[cfg(feature = "antithesis")]`)

All three are `assert_always_greater_than_or_equal_to!`, each placed immediately
before the unguarded subtraction it watches. They are the in-process witnesses
for the #21683 underflow family.

| File:line | Watches | Message |
| --- | --- | --- |
| `ledger.rs:271` | `get_total_records`'s `… - 1` (wrapped-id difference must be ≥ 1) | "ledger get_total_records never underflows on a drained buffer" |
| `ledger.rs:313` | `decrement_total_buffer_size`'s `fetch_sub(amount)` | "ledger total_buffer_size decrement never underflows" |
| `reader.rs:529` | the `metadata.len() - bytes_read` size-delta in the file-deletion path | "reader data-file size delta never underflows" |

Detector-not-guard caveat (from code review): because the macros report rather
than abort, the underflowing arithmetic still executes on the line after a
failing assertion — Antithesis records the violation, the wrap still happens.
The `ledger.rs:313` check loads `total_buffer_size` separately from the
subsequent `fetch_sub`; given disk_v2's single-reader/single-writer model with
increment-only concurrency from the writer, this TOCTOU cannot mask a real
underflow (a concurrent increment only makes the subtraction safer). See
`properties/total-buffer-size-never-underflows.md`.

## Harness assertions (`tests/antithesis/scenarios/vector_to_vector_e2e_disk/src/bin`)

Workload/oracle side, always compiled (the harness crate always depends on the
SDK with `full`).

**oracle.rs** — the conservation judge (separate process from the SUT):

| Line | Macro | Property |
| --- | --- | --- |
| `oracle.rs:118` | `assert_always!(was_issued, …)` | online integrity / no-spurious; fires on every `/ingest` |
| `oracle.rs:128` | `assert_reachable!` | first end-to-end delivery is reached |
| `oracle.rs:202` | `assert_reachable!` | oracle came up after SUT readiness |

**parallel_driver_produce.rs** — the producer driver:

| Line | Macro | Property |
| --- | --- | --- |
| `parallel_driver_produce.rs:105` | `assert_reachable!` | the e2e-ack path is exercised |

**eventually_conservation.rs** — end-of-test conservation + liveness:

| Line | Macro | Property |
| --- | --- | --- |
| `eventually_conservation.rs:157` | `assert_unreachable!` | oracle must be reachable at judgment time |
| `eventually_conservation.rs:165` | `assert_always_less_than_or_equal_to!(missing_count, 0, …)` | conservation: every acked id came back (**gated on quiescence**) |
| `eventually_conservation.rs:174` | `assert_unreachable!` | redundant loss flag (**gated on quiescence**) |
| `eventually_conservation.rs:181` | `assert_always_less_than_or_equal_to!(spurious_count, 0, …)` | no invented/corrupted ids (**gated on quiescence**) |
| `eventually_conservation.rs:190` | `assert_sometimes_greater_than!(acked, 100, …)` | coverage: a large set was acked and conserved |
| `eventually_conservation.rs:196` | `assert_sometimes_greater_than!(delivered_total, delivered, …)` | coverage: the at-least-once replay path ran |
| `eventually_conservation.rs:218` | `assert_always!(progressed, …)` | liveness: a fresh event still round-trips |

Lifecycle: `oracle.rs:201` calls `lifecycle::setup_complete(...)` after the SUT
metrics endpoint answers; all three binaries call `antithesis_init()` first.

## Soundness notes carried from code review (2026-06-02)

These affect what a green run means and are tracked as open questions on the
conservation properties:

- **Quiescence-gated conservation.** The `missing_count`/`spurious_count`
  `assert_always_*` checks fire only inside `if quiescent`. The target bug is a
  permanent writer wedge; a wedge that prevents the counters from settling for
  5 polls within the 240s deadline leaves these checks *skipped* (a skipped
  `assert_always` is not a failure). Only the online `oracle.rs:118` spurious
  check is unconditional — conservation has no unconditional equivalent. See
  `properties/multi-hop-conservation-no-loss.md`.
- **Best-effort ack relay.** `parallel_driver_produce.rs` records the obligation
  by POSTing `/acked` with the HTTP result discarded (`let _ =`). A dropped
  relay over a fault-injected link erases the obligation, so a later genuine
  loss of that id is invisible to `missing = acked - delivered`. See
  `properties/every-written-event-eventually-delivered.md`.

## Anchors still useful for future instrumentation

- `tracing` (`trace!`/`debug!`/`error!`) marks where invariants already matter —
  e.g. the wrapping `trace!` in `decrement_total_buffer_size` itself.
- `metrics`-based internal events (`internal_events.rs`, `buffer_usage_data.rs`)
  are the existing observability surface; #21683 lives in the gap between these
  metrics and reality (PR #23561 saturated only the reporter gauge).
- `debug_assert!`/`assert!` plus the `proptest` + model tests under
  `variants/disk_v2/tests/` show where the authors already considered invariants
  worth checking.

## Assumptions / Open Questions

- The SUT-side assertions are the committed form (`assert_always_greater_than_or_equal_to!`),
  not the `assert_unreachable!`/`assert_always!(amount <= current)` framing that
  earlier evidence files proposed. Evidence files have been reconciled to the
  committed form; do not re-propose them as "missing."
- Whether to add a fourth SUT-side `assert_reachable!` at the underflow-recovery
  branch once a real saturation fix lands (to confirm Antithesis triggers the
  scenario) remains open — see `properties/total-buffer-size-never-underflows.md`.
