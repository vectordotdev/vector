# Property: durable-unacked-events-survive-crash

## Catalog Entry

**Type:** Safety / Always

**Property:** Every event durably synced (fsync'd) to a data file but not yet
acknowledged is readable after an ungraceful crash + restart. Recovery skips/loses
no synced event. Loss is bounded to the documented ≤500ms unsynced window.

**Invariant:** Let `S` be the events whose writes were confirmed durable by a
completed `sync_all` (data fsync) and a completed ledger `flush` (msync) before the
kill. After restart and full recovery (`from_config_inner` returns), draining to
the quiet-period boundary yields every event in `S` at least once. No event in `S`
is silently absent.

**Antithesis Angle:**

1. Workload writes events with monotonic unique IDs. It marks an event durable (adds
   its ID to `S`) only after an fsync window has closed — concretely, after an e2e
   ack returns for an event written ≥500ms ago (≥1 fsync cycle elapsed), or via an
   out-of-band fsync signal (OQ-1).
2. Antithesis injects SIGKILL before, during, or after the 500ms window.
3. Vector restarts. Workload waits for the quiet period (no new events, buffer
   drains to empty).
4. Workload computes `S_delivered` and asserts `S ⊆ S_delivered` (at-least-once,
   duplicates expected).
5. Events written within the 500ms unsynced window before the kill are excluded
   from `S` — their loss is in-contract.

**Why It Matters:** The core durability promise: "data synced to disk survives a
crash." The ≤500ms unsynced window is documented; silent loss of SYNCED data is
not, through three recovery seams between non-atomic syscalls the scheduler can
split:

- Kill between data `sync_all` and ledger `flush` → ledger lags data →
  `validate_last_write` `Ordering::Less` fast-forwards `next_record_id`; a bug here
  silently drops synced data.
- `validate_last_write` `Ordering::Greater` logs "Events have likely been lost" and
  rolls forward — correct detection, but the gap must not cover synced data (and
  risks the reader monotonicity panic, see record-id-monotonicity in `_shelved.md`).
- `update_buffer_size` over-seeds `total_buffer_size` vs what `seek_to_next_record`
  decrements → the #21683 wedge, zero further delivery, indistinguishable from loss
  (mechanism in `total-buffer-size-never-underflows.md`).

Recovery branches exercised: `validate_last_write` Less/Greater, and
`seek_to_next_record` fast-path (mmap-validate + delete acked files) vs slow-path
(read via `next()` until caught up).

**Fault Requirements:** SIGKILL required (confirm enabled). CPU throttle to stretch
the 500ms window is a secondary lever.

SUT-side: covered by the committed underflow detectors; no
durable-survives-crash-specific assert is committed and the e2e oracle owns the
verdict.

**Workload-level set-difference check:** maintain `DURABLE` (IDs confirmed synced
before kill) and `DELIVERED` (IDs received post-restart). After quiet period assert
`DURABLE.difference(DELIVERED).is_empty()`. `DELIVERED` IDs not in `DURABLE` are
fine (at-least-once).

---

## Open Questions

**OQ-1 (Critical): How does the workload establish "durably written" without
Vector internals?** The 500ms window makes the synced frontier hard to know
externally. Options:

- (a) e2e acks: durable if acked downstream before kill (full
  write→read→deliver→ack→delete cycle). Conservative but correct.
- (b) Instrument Vector to log `writer_next_record_id` after each `sync_all`; parse
  for the durable frontier. More precise.
- (c) `flush_interval=0` (fsync every flush) so every write is immediately durable;
  only the final partial write before kill is excluded. Cleanest but changes the
  production path.

Recommend (c) for initial testing, (b) for production-representative timing.

**OQ-2: Does `validate_last_write` `Ordering::Greater` skip cleanly, or produce a
gap overlapping synced records?** The case emits an error log, sets `should_skip =
true`, triggering `reset()` + `mark_for_skip()`, deferring the skip to the next
`ensure_ready_for_write`. If the writer rolls to the next file with valid unread
synced records in the old file, those should stay accessible (the old file is not
deleted), but the `writer_next_record_id` gap may read as a monotonicity violation
— verify against the `reader.rs` `MonotonicityViolation` panic.

**OQ-3 (RESOLVED):** `flush_inner` orders `writer.sync_all().await` then
`self.ledger.flush()`; the `await` is a yield point a kill can split — the exact
`Ordering::Less` scenario. Reachable.

The Drop-does-not-flush silent-loss vector (#24948) is owned by the live
`config-reload-no-silent-loss` and the shelved `graceful-shutdown-flushes-all`.
