---
slug: writer-eventually-makes-progress
type: Liveness / Sometimes(writer_unblocked_after_full)
sut_path: lib/vector-buffers/src/variants/disk_v2/
commit: 049eec79b737450c4669b7f8aa1dd814551ec466
updated: 2026-06-02
linked_bugs:
  - vectordotdev/vector#21683 (root cause: see total-buffer-size-never-underflows)
  - L1 / L8 in sut-analysis.md §5 (liveness claims that fail under underflow)
---

# Property: writer-eventually-makes-progress

## Catalog Entry

**Type:** Liveness / Sometimes(writer_unblocked_after_full)

**Property:** A writer blocked because `is_buffer_full()` returned `true`
eventually performs another successful `write_record` after the reader acks and
deletes at least one data file. `is_buffer_full()` never stays `true` permanently
(writer deadlock).

**Invariant:** After every `delete_completed_data_file` that calls
`notify_reader_waiters()`, the writer unblocks and completes at least one
`write_record` within a bounded time. The tuple `(is_buffer_full() == true,
buffer_is_drained == false)` is not a permanent fixed point.

**Antithesis Angle:** The user-visible manifestation of #21683 — a silent pipeline
stall. Workload follows the fault sequence below (fill → node-kill at a rotation /
partial-write boundary → restart → reader drains → STOP_FAULTS), then asserts
`Sometimes`: the writer completes at least one write after a post-restart file
delete. If the underflow fires during the kill/restart, the writer never unblocks
and `Sometimes` is never satisfied — a liveness failure.

**Why It Matters:** A pipeline stall with no error, no crash, no alert is the
worst operational failure mode. Dashboards look normal (PR #23561 saturates the
buffer-size gauge to 0 instead of u64::MAX). The sink stops delivering, no alert
fires, the customer suffers silent data loss with no signal — exactly what the
disk buffer is supposed to prevent.

---

## The Deadlock Chain

Mechanism stated once in `total-buffer-size-never-underflows.md`. Once
`total_buffer_size` wraps to ~u64::MAX:

- `is_buffer_full()` is permanently `true`; the wrapped value is `Acquire`-visible
  to the writer at once.
- `ensure_ready_for_write` loops forever on `wait_for_reader().await`, logging only
  at `trace!`. Each reader file-delete fires `notify_reader_waiters`, the writer
  wakes, re-checks full, re-blocks. When the reader drains the buffer it stops
  notifying and the writer sleeps forever.
- `can_write_record` is poisoned the same way, and the reader's own
  `total_buffer_size == 0` shutdown signal is never reached (L8) — no clean
  termination either.

Wakeup chain (break any link → stall): sink delivers → BatchNotifier dropped →
finalizer `increment_pending_acks` + `notify_writer_waiters` (wakes the READER,
misleading name) → reader `handle_pending_acknowledgements` →
`delete_completed_data_file` → `decrement_total_buffer_size` (UNDERFLOW
post-restart) → `notify_reader_waiters` (wakes the WRITER) → re-checks full →
re-blocks. Kill at multiple points here.

Non-determinism: `unflushed_bytes + u64::MAX` can wrap a second time near 0 on some
inputs, giving intermittent false negatives — the stall is input-dependent.

---

## Observable Signals and committed coverage

Workload-observable: write throughput AND sink-delivery throughput both drop to
zero after node-kill-and-restart under buffer-full, with no ERROR/WARN log
(dashboards look healthy via PR #23561's gauge saturation). The G2-Phase-1
compound detector must use BOTH rates plus buffer >~90% plus duration > drain-bound
to distinguish deadlock from healthy block-mode backpressure (grind-plan G2).

Committed coverage: the post-recovery liveness probe in the e2e oracle
(`eventually_conservation.rs`, `assert_always!(progressed, ...)`) drains and
reasserts delivery progress after restart, so the claim IS exercised workload-side.
The committed SUT-side underflow detectors catch the root-cause wrap. A
`writer_unblocked_after_full` `assert_sometimes!` and a stall-count
`assert_unreachable!` inside `ensure_ready_for_write` are NOT committed — still-to-add
precision signals.

---

## Antithesis Fault Strategy

### Recommended fault sequence

1. **Fill:** Send events to bring `total_buffer_size` near `max_buffer_size`.
   Writer blocks (normal backpressure).
2. **Fault:** SIGKILL at a file-rotation boundary — most reliably during the
   `fsync` just before/after a data file reaches 128MB, leaving a partial tail.
3. **Restart:** `update_buffer_size` re-seeds from file sizes.
4. **Reader drain:** Reader seeks, acks, deletes files normally.
5. **STOP_FAULTS:** `ANTITHESIS_STOP_FAULTS` or equivalent quiet period.
6. **Verify:** Assert `Sometimes(writer_unblocked_after_full)` within the quiet
   period. Never seen → test fails.

### Why Antithesis over a fixed chaos test

The internal chaos test uses SIGKILL ×3 at fixed points. The underflow depends on
the *exact byte offset* of the crash relative to the file boundary. Antithesis's
systematic fault-timing exploration finds the specific windows (kill during file
rename/open at rotation, or during the first `write_all` to the new file) a
fixed-timing test misses.

### CPU throttling amplification

CPU throttling widens the gap between `fetch_sub` and the next `is_buffer_full`
check, increasing the window for a reader wakeup to arrive between underflow and
re-check. Throttling the writer process may surface race variants.

The vacuity interaction with `buffer-size-within-max` (passing trivially under a
dead writer) is recorded in `_shelved.md`.

---

## Open Questions

- **`Sometimes` reachable without faults?** Yes — any fill-then-drain cycle
  satisfies it, covering the non-fault path before the fault path.
- **Grace period for the quiet-phase check:** ~10s after STOP_FAULTS for a healthy
  system; tune to `max_buffer_size` and throughput.
- **`Notify` miss-wakeup:** a non-issue healthy (`notify_one` stores one permit,
  consumed by the next `notified().await`). Under underflow the reader stops
  notifying once drained and the writer sleeps forever — the bug, not a wakeup
  race. `wait_for_reader` has no timeout (no watchdog).
- **Finalizer-death is a separate independent trigger** for the same stall
  (`_shelved.md`), observable via this same `Sometimes`.
- **Node-termination required** — else the underflow trigger is unreachable and the
  property passes trivially.
