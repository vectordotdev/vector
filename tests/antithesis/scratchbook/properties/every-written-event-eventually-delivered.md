# Property: every-written-event-eventually-delivered

## Catalog Entry

**Type:** Liveness / Sometimes (at-least-once) — `Sometimes(all_produced_delivered)`

**Property:** With e2e acks enabled, every event accepted by the source (written
into the disk buffer) is eventually delivered downstream at least once across
crashes. Duplicates allowed, silent loss not — the at-least-once contract the
product sells.

**Invariant:** Let `PRODUCED` be the unique IDs the workload submitted to the
source. After faults and a quiet period (no new events, no in-flight acks), every
ID in `PRODUCED` appears at least once in `DELIVERED` (IDs at the downstream sink).
`|DELIVERED| ≥ |PRODUCED|` is expected (crash+replay duplicates); the violation is
any ID in `PRODUCED \ DELIVERED`.

**Antithesis Angle:**

1. Workload assigns each event a unique ID (monotonic counter in the payload) and
   records it in `PRODUCED`.
2. The downstream sink (workload stub or structured log sink) records every
   received ID in `DELIVERED`.
3. Antithesis injects SIGKILL at arbitrary timing — during write, fsync, read,
   ack, file deletion, rotation. Vector restarts and resumes after each.
4. After the quiet period (production stopped, buffer drained, writer closed) the
   workload compares the sets.
5. `Sometimes(all_produced_delivered)` fires when `PRODUCED ⊆ DELIVERED` in at
   least one explored timeline.
6. A hard `assert_always` fires if `PRODUCED \ DELIVERED` is non-empty at
   quiescence — the primary falsification signal.

**Why It Matters:** The headline durability guarantee. A customer enabling disk
buffers + e2e acks opts into at-least-once. One event silently dropped across a
crash breaks the contract, and silent loss is the hardest production failure to
detect (no error, no alert, normal-looking throughput). Known silent-loss paths:

- `TrackingBufWriter` 256KB buffer on crash (not yet page-cache flushed):
  in-contract loss for unsynced events.
- Synced-but-unacked events: must survive crash via replay — the primary liveness
  test.
- Deleted-file-before-ledger-msync window: `delete_completed_data_file` unlinks the
  file then `ledger.flush` separately. A kill between leaves the ledger reader file
  id pointing at a deleted file; restart handles it via NotFound→advance, but
  unacked events in that file are lost — the most serious latent loss path.
- `BufferWriter::drop` does not flush (close only): graceful shutdown skipping an
  explicit flush loses up to 256 KiB (#24948 vector, owned by config-reload).
- Sink-error acks: `spawn_finalizer` discards `_status`, so `Errored`/`Rejected`
  still credits the ack → silent loss (owned by `sink-failure-not-silently-acked`).

**Fault Requirements:** SIGKILL required (confirm enabled). Valuable sequences:

- Kill in the `delete_completed_data_file` window: unlink done, ledger flush not.
- Kill in the page-cache-write-to-fsync window (≤500ms): in- vs out-of-contract loss.
- Kill during file rotation: `ensure_ready_for_write` partial rotation.
- CPU throttle: stretches the 500ms window, growing the expected-loss set.

**Workload Implementation Notes:**

Handling `PRODUCED vs DELIVERED` when duplicates are expected:

```
PRODUCED: Set<u64>  -- all IDs submitted to Vector source
DELIVERED: MultiSet<u64>  -- all IDs seen at downstream sink (may repeat)
DELIVERED_SET: Set<u64> = unique(DELIVERED)

-- After quiet period:
missing = PRODUCED.difference(DELIVERED_SET)
assert missing.is_empty()  // at-least-once: every produced ID must appear

-- Count duplicates (expected; used to verify replay, not assert on):
duplicate_count = |DELIVERED| - |DELIVERED_SET|
log("duplicate deliveries observed: {}", duplicate_count)
```

Dedup is at the workload level — the downstream sink dedups by ID before any
business logic, matching the contract.

SUT-side: the committed underflow detectors guard the arithmetic, not this
set-membership oracle. A `get_total_buffer_size() == 0` "drained" SUT assert was
judged UNSOUND (a wedged buffer can read a corrupted gauge; the workload oracle is
the authoritative drained signal — experiment-spec correction #1) and must not be
added. The verdict is the workload `assert_sometimes!(produced ⊆ delivered)` plus
the hard `assert_always` on `produced \ delivered` empty at quiescence.

---

## Open Questions

**OQ-1: finalizer drain vs SIGKILL.** On SIGKILL the `spawn_finalizer` task dies
undrained; ack-in-flight events (sink dropped the notifier, id not yet in
`pending_acks`) must replay on restart. If the lazily-flushed `reader_last_record`
was already persisted past them, they cannot replay — lost. The interaction of
finalizer lifecycle, lazy `reader_last_record` flush, and SIGKILL timing is the
most subtle loss path. (Finalizer detail in `_shelved.md`.)

**OQ-2: sink-error ack discarding** is out of scope (the `_status` discard always
credits the ack); use a reliable `Delivered` sink stub to avoid conflating with
`sink-failure-not-silently-acked`.

**OQ-3: keep many acks in flight** — size `max_size`/batches so events sit in
varied ack-flight states; a small buffer drains before faults hit timing windows.

**OQ-6 (Soundness — quiescence-gated conservation):** Conservation
`assert_always_less_than_or_equal_to!(missing_count, 0)` and the spurious-id check
fire only inside `if quiescent` (eventually_conservation.rs:165/174/181). The
target bug is a permanent writer wedge; a wedge that keeps counters from settling
for 5 polls within the 240s deadline leaves these checks **skipped**, and a
skipped `assert_always` is not a failure. Only the online integrity check
(oracle.rs:118) is unconditional. A permanent deadlock can therefore evade this
oracle entirely — needs an unconditional liveness/quiescence-timeout signal.

**OQ-7 (Soundness — best-effort ack relay):** The producer records each obligation
by POSTing `/acked` with the HTTP result discarded (parallel_driver_produce.rs:71).
A dropped relay over a fault-injected link erases the obligation, so a later
genuine loss of that id is invisible to `missing = acked - delivered` — the id
never entered the set. The relay must be durable/retried, or the obligation logged
before the POST.
