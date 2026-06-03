---
slug: total-buffer-size-never-underflows
type: Safety / Unreachable
sut_path: lib/vector-buffers/src/variants/disk_v2/
commit: 049eec79b737450c4669b7f8aa1dd814551ec466
updated: 2026-06-02
linked_bugs:
  - vectordotdev/vector#21683
  - PR #23561 (metrics reporter only — control-path UNFIXED)
---

# Property: total-buffer-size-never-underflows

## Catalog Entry

**Type:** Safety / Unreachable

**Property:** The `total_buffer_size` AtomicU64 in `Ledger` is never decremented
by more than its current value; the atomic never wraps toward u64::MAX.

**Invariant:** For every `decrement_total_buffer_size(amount)`:

```
amount <= self.total_buffer_size.load(Ordering::Acquire)
```

Violation wraps the atomic to ~2^64 − amount, permanently poisoning
`is_buffer_full()` and deadlocking the writer.

**Antithesis Angle:** Needs a node-kill at a file-rotation or partial-write
boundary, then restart. After restart `update_buffer_size` re-seeds
`total_buffer_size` from the on-disk *file sizes* of all `.dat` files. The reader
seeks forward through partial records, decrementing by *record bytes* — strictly
smaller than the file's on-disk size when the tail was never fully written. That
mismatch underflows.

**Why It Matters:** Root cause of #21683. The control-path atomic is unfixed at
this commit. PR #23561 applied `saturating_sub` only to the metrics *reporter*
(the dashboard gauge), not the atomic gating the writer. A wrapped value makes
`is_buffer_full()` permanently `true`, starving the write path with no error, no
ERROR log, no crash — a silent stall.

---

## Trigger Paths (verified against source)

### Path A: seek-on-restart mismatch (primary trigger)

1. Writer writes a partial record at the end of a 128MB `buffer-data-N.dat`, then
   crashes before `fsync` completes.
2. On restart `update_buffer_size` calls `increment_total_buffer_size(file_size_of_N)`
   — the *whole* file size, including the partial tail.
3. The reader's `seek_to_next_record` calls `track_read` for every record it
   skips, each doing `decrement_total_buffer_size(record_bytes)` (the
   serialized-record length).
4. At the torn tail the reader stops. The summed `record_bytes` is less than the
   file size added at step 2, so `total_buffer_size` is still correct.
5. HOWEVER: `delete_completed_data_file` with `bytes_read = Some(self.bytes_read)`
   computes `size_delta = metadata.len() - bytes_read` — plain `u64 -`. If
   `bytes_read > metadata.len()` (racing decrements or external truncation) it
   panics in debug, wraps in release. The committed
   `assert_always_greater_than_or_equal_to!(metadata.len(), bytes_read, ...)`
   reports this as a detector before the subtraction. The delta then flows into
   `decrement_total_buffer_size`, whose `self.total_buffer_size.fetch_sub(amount,
   Ordering::AcqRel)` has no saturation guard — its committed assert reports the
   underflow but does not prevent the `fetch_sub`.

### Path B: double-decrement on skip

When the reader fast-forwards a whole file at init (`bytes_read = None`),
`delete_completed_data_file` decrements the full `metadata.len()`. If the reader
already called `track_read` for records in that file, those bytes subtract twice,
and the net decrement exceeds the original increment. (RESOLVED non-issue — see
Open Questions.)

### Raw underflow site

`decrement_total_buffer_size` does an unguarded `fetch_sub` (untouched by PR
23561). Its wrapping `trace!` `last_total_buffer_size - amount` also emits a
nonsense value, hampering diagnosis.

---

## Downstream Effect: Permanent Writer Deadlock

Once wrapped, `is_buffer_full()` is permanently true, `ensure_ready_for_write`
loops forever on `wait_for_reader().await`, `can_write_record` is equally
poisoned — silent stall, no ERROR log, no panic, dashboard gauge a healthy 0 (the
PR #23561 reporter `saturating_sub`). Chain in `writer-eventually-makes-progress.md`.

---

## SUT-Side Instrumentation

Three committed underflow DETECTORS (report, then the subtraction runs anyway):
ledger `total_buffer_size` decrement, reader `metadata.len() - bytes_read`
size-delta, and ledger `get_total_records` (the `next_writer_id.wrapping_sub(last_reader_id) - 1`
that wraps to ~2^64 on a drained buffer). Authoritative list in `_shelved.md` header
and `../existing-assertions.md`. Control-path arithmetic still unfixed.

---

## Why Existing Tests Cannot Catch This

- The model proptest (`tests/model/`) uses `TestFilesystem` with no-op
  `sync_all`/`flush` — no partial writes on disk, so `update_buffer_size` re-seeds
  zero and the mismatch never materializes.
- `LedgerModel::decrement_buffer_size` mirrors the unguarded `fetch_sub` (would
  reproduce the underflow) but the trigger is unreachable via the in-memory fs.
- `writer_waits_when_buffer_is_full` (`size_limits.rs`) — the backpressure test on
  the deadlock path — is `#[ignore]`.

---

## Open Questions

- **Path B is RESOLVED as a non-issue:** the `bytes_read = None` fast-forward
  decrements the full file size exactly once, and `bytes_read` plus the delete-time
  `metadata.len() - bytes_read` equals `metadata.len()` for a fully-read file. The
  real Path A risk is `update_buffer_size` re-seeding on-disk bytes exceeding
  valid-record bytes (torn tail). The unguarded site is `metadata.len() - bytes_read`,
  wrapping when `bytes_read > metadata.len()` via a racing decrement, external
  truncation, or over-counted `bytes_read`.
- **Node-termination required** — needs kill+restart.
- **Build mode:** the wrapping `trace!` PANICs under `debug_assertions`, so the
  harness must run release (grind-plan G2).

---

## Test plan (2026-06-02, expert-chorus decision)

**Home:** in-tree `proptest` in `lib/vector-buffers`, NOT an Antithesis scenario —
deterministically reproducible in-process. (Antithesis keeps the distributed
`vector_to_vector_e2e_disk` conservation experiment + the #24948 SIGHUP fault.)

**This site needs a REOPEN** — not reachable purely in-process.
`decrement_total_buffer_size` `fetch_sub` and the reader's delete-time
`metadata.len() - bytes_read` disagree only across a restart: `from_config_inner`
→ `update_buffer_size` re-seeds from the SUM of on-disk `.dat` bytes, then the
reader's seek decrements per replayed record. They diverge only when on-disk size
exceeds valid-record bytes — a torn tail.

**Test B — `tests/reopen.rs`** (`current_thread` runtime + in-memory `TestFilesystem`):

1. Write / read / ack a generated prefix through the real buffer.
2. Drop reader + writer + ledger, then drain the finalizer task via `yield_now` so
   it releases the ledger lock (holds an `Arc<Ledger>`; precedent model/mod.rs:890).
   On `current_thread` the task won't release until polled — else reopen hits
   `LedgerLockAlreadyHeld`.
3. INJECT a torn tail: truncate the last `.dat` by a generated N bytes via
   `TestFilesystem`. LOAD-BEARING — its no-op `sync_all`/`flush` and always-complete
   writes can't produce the divergence otherwise.
4. Reopen via `from_config_inner` (same fs + data_dir) under a `timeout`.
5. Assert the underflow directly (NOT `< ceiling`): `total_buffer_size` did not
   wrap high (no value near `u64::MAX`) and `bytes_read <= file_size`. `proptest`
   over {prefix len, truncation bytes, max_data_file_size}; commit a regression
   seed; demote to plain `#[test]` if the space collapses.

**Discipline:** must FAIL on current code (TDD). PR #23561 saturated only the gauge
*reporter*, leaving the control-path `fetch_sub` broken, so a fix must address the
control path. Do NOT add a reopen action to `model_check` — the model would
re-derive `update_buffer_size` (the logic under test → false failures) and its
no-op fs can't make a torn tail. No `sane`/`insane` in names.
