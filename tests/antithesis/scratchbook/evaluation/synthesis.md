---
sut_path: /home/ssm-user/src/vector
commit: 049eec79b737450c4669b7f8aa1dd814551ec466
updated: 2026-06-02
external_references:
  - path: (internal design doc, not linked)
    why: Bug context for evaluated properties
  - path: (internal design doc, not linked)
    why: Lock-contention performance issue informing the throughput gap
---

# Property Evaluation Synthesis

Four lenses (antithesis-fit, coverage-balance, implementability, wildcard)
evaluated the 19-property catalog as a portfolio. Findings categorized below as
**Refinement** (applied to the catalog), **Gap** (filled via targeted discovery),
or **Bias** (escalated to the user). Evidence: `evaluation/{lens}.md`.

## Addendum — Semantics-first doctrine pass (2026-06-02, Category 8)

Evaluation of the 3 new Category-8 properties (`semantic-claims-ledger.md`):

- **Antithesis-fit:** strong. C1/C2 are exactly the rare, timing-sensitive,
  externally-invisible divergences Antithesis is built to surface (a 200 followed by
  a reload/crash inside the fsync window). The `assert_unreachable` loss-magnet is
  the right shape for *exhibition* — it makes the search hunt the violation. No new
  fault types: C1/C2 reuse the existing reload/kill/partition faults.
- **Coverage-balance:** these do NOT add a new mechanism — they re-cut Clusters
  A/B/E/F at the *claim* level. Their value is the doctrine (claim-first), not new
  surface. Kept distinct because the *claim* is the unit the user wants demonstrated,
  and C2 resolved a standing critical open question (acks not transitive).
- **Implementability:** C1/C2 are already implemented by the launched harness
  (`ack-does-not-imply-durability` IS the experiment). C11/C12 are oracle
  *constraints* (sets-not-order, duplicates-as-anti-vacuity), already honored.
- **Wildcard / false-red watch:** C11 and C12 exist precisely to prevent false reds
  (duplicates and reordering are legal). Recording them as non-assertions is the
  refinement.

No bias escalations. This was an extension/reframing pass, not a re-evaluation of the
existing 30; their refinements stand.

## Headline

The lenses **agree the deadlock/durability/recovery cluster is the correct
high-value core** and the assertion types there are right. The most important
new findings are subtle and would have silently undermined the test:

1. **The deadlock is intermittent, not permanent** (wildcard W-M1): `u64::MAX +
   unflushed_bytes` wraps back to a *small* number, so the writer escapes for
   exactly one write whenever `unflushed_bytes > 0`, then re-deadlocks. A naïve
   `Sometimes(writer_unblocked)` or "throughput→0" check produces a **false
   green** while the system is broken. → Refinement to compound stall detection.
2. **The durability oracle was conflated** (wildcard W-F2): using e2e-ack
   *delivery* as the "durably written" marker means the deadlock suppresses
   deliveries → the durability property passes *vacuously*. → Refinement to a
   wall-clock-timestamp oracle + `flush_interval=0`.
3. **Two preconditions gate ~14/19 properties**: node-termination faults enabled,
   and the buffer `data_dir` surviving node-kill on a persistent volume. If
   either is false, most of the catalog is vacuous. → Escalated to user.

## Refinements (applied to the catalog)

- **R-A — `every-written-event-eventually-delivered` assertion semantics.**
  `Sometimes(all_produced_delivered)` is wrong for at-least-once (it passes on any
  one good timeline, hiding loss on others). Changed to: per-event
  `Always(produced ⊆ delivered)` checked after each quiet-period drain, plus a
  `Sometimes(delivery_path_reachable)` for exploration. (wildcard W-O2)
- **R-B — `writer-eventually-makes-progress` stall signal.** Because the deadlock
  is intermittent (W-M1) and externally indistinguishable from healthy
  `WhenFull::Block` backpressure (W-O3), the signal is now **compound**: write
  throughput ≈ 0 AND sink/ack throughput ≈ 0 AND buffer ≥ ~90% full AND duration
  > drain-time bound ⇒ `assert_unreachable!("persistent_deadlock")`. A single
  "any wakeup" or "throughput→0" check is insufficient.
- **R-C — `buffer-size-within-max` is compound-only.** Under the deadlock the
  bound holds vacuously (no writes ⇒ no overflow). Marked: meaningful only when
  evaluated jointly with `writer-eventually-makes-progress`; never report alone.
- **R-D — `record-id-wraparound-accounting-holds` refocused.** The true u64 wrap
  is unreachable on a production binary; the reachable, real bug is the
  empty-buffer equality case (`wrapping_sub(x,x) - 1 = u64::MAX` at `ledger.rs:281`)
  firing on every clean restart of a drained buffer. Property prose refocused to
  the empty-buffer case (workload-observable: drain→restart→gauge≈0); true-wrap
  explicitly descoped. (fit-1, impl-F5)
- **R-E — `file-id-rollover-stays-coordinated` build requirement.** Production
  `MAX_FILE_ID=65535` needs ~8TB of writes to roll; `MAX_FILE_ID=6` is only in
  `#[cfg(test)]`. Added requirement: run a test-binary or add a runtime-tunable
  `MAX_FILE_ID`, else descope. (impl-F4)
- **R-F — durability oracle.** `durable-unacked-events-survive-crash` and
  `every-written-event-eventually-delivered` now specify the **wall-clock-timestamp
  oracle** (events produced > 2×`flush_interval` ago are "past the fsync window")
  and recommend `flush_interval=0` to make every `flush()` a `sync_all`, removing
  clock dependence and the delivery-vs-fsync conflation. Resolves the standing
  "what does durably-written mean?" open question. (W-F2, W-C1)
- **R-G — `total-buffer-size-never-underflows` build note.** The `trace!` at
  `ledger.rs:322` evaluates `last_total_buffer_size - amount`, which panics in a
  debug build *before* the release-mode wrap is observable. Harness must use a
  **release build** for this property (and the `trace!` itself should be fixed to
  `wrapping_sub`). (impl-F6)
- **R-H — priority note on the two "logic-bug" properties.**
  `dropped-events-are-counted` and `sink-failure-not-silently-acked` are missing
  function-call / discarded-status bugs better caught by deterministic
  unit/integration tests; kept as **workload-side secondary checks with no
  dedicated fault-search budget** (don't shape the fault strategy around them).
  (fit-3, fit-4)
- **R-I — corruption abandonment sub-concern.** The "skip rest of file after first
  bad record" loss (valid records after a corrupt one in the same 128MB file are
  abandoned) is now an explicit, measurable angle under
  `corruption-is-detected-and-recovered` (correlate corruption-injection timing
  with event IDs; assert post-corruption-point events still arrive). (coverage T1,
  W-C2)

## Gaps (filled via targeted discovery — 7 new properties)

New Category 7 in the catalog. All are squarely timing/concurrency/partial-failure
or claimed-guarantee gaps the focus-based discovery missed:

- **`foreign-data-file-no-writer-stall`** (coverage F1) — a stray `.dat` file
  inflates `update_buffer_size` → permanent writer stall **without any crash**
  (operator-error path to the #21683 symptom; distinct root cause: wrong scan
  scope, not arithmetic).
- **`ledger-corruption-no-sigbus-crashloop`** (coverage F1, wildcard) — external
  truncation of the mmap'd `buffer.db` → unhandled SIGBUS / crash loop; should be
  a clean detected init error.
- **`finalizer-task-drains-pending-acks`** (coverage F6) — the unmonitored
  detached finalizer task dying strands acks → silent loss / stall, distinct from
  the arithmetic deadlock.
- **`fsync-window-bounded-under-clock-jitter`** (coverage F3, wildcard W-M2) —
  clock faults can suppress `should_flush`'s `Instant::elapsed` gate, silently
  extending the loss window beyond the 500ms SLA (only rotation is
  clock-independent).
- **`overflow-chain-no-unaccounted-gap`** (coverage F4, wildcard W-M3) — the
  entire `WhenFull::Overflow` mode was uncovered; crash during overflow loses
  *later* in-memory events while *earlier* disk events survive → a middle-of-stream
  gap that breaks dedup-based at-least-once reasoning.
- **`buffer-survives-version-upgrade`** (coverage F5) — rkyv layout change /
  `DiskBufferV1CompatibilityMode` flag inversion → old buffer files unreadable or
  silently mis-decoded; should be a clean detected error, never garbage.
- **`throughput-progresses-under-contention`** (coverage F2) — the writer-mutex
  lock-contention ceiling under CPU throttle can collapse throughput to near-zero
  *without* tripping the permanent-deadlock property (degenerate-but-alive).

These additions are substantial enough (a new category + 7 properties) that a
second light evaluation pass is warranted before workload construction;
see "Residual" below.

## Biases (escalated to the user)

- **Bias B1 — Portfolio orientation: timing-cluster vs. logic-bug properties.**
  ~6 properties (`dropped-events-are-counted`, `sink-failure-not-silently-acked`,
  `record-id-wraparound` empty case, `record-never-spans-files`, parts of
  `graceful-shutdown-flushes-all`) are deterministic logic/metric bugs that unit
  or integration tests would catch more cheaply than Antithesis search. Keeping
  them in the Antithesis catalog spends search budget on states fault injection
  doesn't help reach. **Judgment needed:** include them (broader regression net)
  or hand them to unit tests and focus Antithesis purely on the
  timing/crash/concurrency cluster?
- **Precondition P1 (escalation, not opinion) — node-termination faults.**
  ~14/19 (now ~21/26) properties require kill/restart faults, often disabled by
  default in Antithesis tenants. If disabled, the highest-value cluster yields no
  signal.
- **Precondition P2 (escalation) — persistent buffer storage across node-kill.**
  Disk-buffer durability is only meaningfully testable if the `data_dir` survives
  a modeled crash on a persistent volume. If node-kill wipes the container FS, all
  crash-recovery properties pass vacuously. The catalog recommends a pre-fault
  **sentinel** (write+fsync, kill, assert files survive; gate Category 2–6
  assertions on it) — wildcard W-F1.

## Passes (independently confirmed correct)

- The deadlock cluster's 4-vantage-point design (root, writer liveness, reader
  termination, safety-bound vacuity) with explicit cross-references.
- `record-id-monotonicity-holds` as `Unreachable`; `no-corrupted-record-delivered`
  as `AlwaysOrUnreachable`.
- `memmap2::MmapMut::flush()` confirmed `msync(MS_SYNC)` (blocking/synchronous) —
  no MS_ASYNC concern.
- The unlink-before-ledger-flush window in `delete_completed_data_file` is safe
  (idempotent NotFound→skip on restart).
- `file-id-rollover-stays-coordinated` correctly scoped to test-time `MAX_FILE_ID`.

## Residual / next steps

- The 7 gap properties + 9 refinements were applied to the catalog and
  relationships; `commit`/`updated` refreshed. Because the additions form a new
  category, a brief re-evaluation of integration is advisable but not blocking for
  `antithesis-setup`/`antithesis-workload` to begin on the core cluster.
- Several open questions remain partial and need code-tracing or human input:
  graceful-shutdown drop ordering in `running.rs`; whether sinks emit
  `Errored`/`Rejected` status in practice; whether the tokio runtime drains the
  finalizer task before exit. Tracked per-property in evidence files.

---

## 2026-05-29 — Data-loss expansion (user-driven, no full eval pass)

User is "seriously concerned about data loss," seed: *"if the checksum fails we
skip records."* Added 3 properties to Category 1 (silent data-loss cluster) +
Cluster H in relationships. Per the property-expansion workflow, 3 new properties
in an existing category do **not** trigger a full evaluation ensemble; recorded
here instead.

- **Gap found:** the catalog confirmed corruption *recovery runs*
  (`corruption-is-detected-and-recovered`, a `Sometimes`) but never bounded or
  counted the loss. `dropped-events-are-counted` covers only `drop_newest`/#24606
  (write-side), not the read-side corruption roll.
- **Grounding:** `roll_to_next_data_file` (reader.rs:705-753) abandons the whole
  file tail, accounting only records read; abandoned records never reach
  `track_dropped_events`. internal doc *internal buffer design notes* (loss window =
  500ms unsynced; synced not lost with e2e acks) and *an internal telemetry-correctness report* (silent loss via `component_discarded_events_total`)
  anchor the severity.
- **New:** `corruption-skip-loss-bounded`, `corruption-skip-loss-is-counted`,
  `corruption-skip-record-id-accounting-consistent`.
- **Bias check:** the existing catalog under-weighted the *read-side* silent-loss
  surface relative to the write-side (#24606) and the deadlock cluster — this
  expansion rebalances toward the user's stated concern. No properties invalidated.
- **Workload note for next setup pass:** all three share one fault — a mid-file
  bit-flip in a multi-record data file, corrupted in a *live* read — so they can
  be implemented as one test scenario with a delivered-set/metric/underflow oracle.
