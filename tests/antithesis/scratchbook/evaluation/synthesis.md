---
sut_path: /home/ssm-user/src/vector
updated: 2026-06-02
---

# How the plan converged

Compact record of the evaluation pass. Live design in `../grind-plan.md`,
`../experiment-spec.md`, `../semantic-claims-ledger.md`; the per-lens scoring rounds
were folded in and deleted.

## Headline findings

1. **The deadlock/durability/recovery cluster is the correct high-value core**, its
   assertion types right (the lenses agreed).
2. **Two preconditions gate most of the catalog** — node-termination faults enabled,
   and the buffer `data_dir` surviving node-kill on a persistent volume. Both
   CONFIRMED by the user (2026-05-28); without them most properties pass vacuously.
3. **The accounting-underflow invariants are deterministic**, so they later moved
   OUT to in-tree `proptest`; Antithesis keeps the distributed conservation
   experiment and the #24948 SIGHUP reload.

## Refinements that changed the design

- **W-M1 — intermittent deadlock false-green.** `u64::MAX + unflushed_bytes` wraps
  back to a small value, so the writer escapes for exactly one write then
  re-deadlocks. A naïve `Sometimes(wakeup)` or "throughput→0" check false-greens.
- **W-O3 — compound stall detector.** Real deadlock is externally indistinguishable
  from healthy `WhenFull::Block` backpressure, so `writer-eventually-makes-progress`
  requires write-rate≈0 AND ack-rate≈0 AND buffer≥~90% AND duration>drain-bound
  before asserting unreachable.
- **Ungate loss and integrity (W-F2 + W-O2).** Do NOT use e2e-ack *delivery* as the
  durability marker — the deadlock suppresses deliveries → vacuous pass. Use a
  wall-clock-timestamp oracle with `flush_interval=0` and per-event
  `Always(produced⊆delivered)` (not `Sometimes(all)`, which hides loss on other
  timelines).

## Descope decision

- **Bias B1 — logic bugs to unit tests.** Deterministic missing-call / metric-blind
  defects (`dropped-events-are-counted`, `sink-failure-not-silently-acked`,
  `record-never-spans-files`, the empty-buffer wrap, parts of graceful shutdown) are
  kept as workload-side secondary checks with **no dedicated fault-search budget** —
  fault injection does not help reach them.
