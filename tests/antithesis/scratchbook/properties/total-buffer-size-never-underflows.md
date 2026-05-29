---
slug: total-buffer-size-never-underflows
type: Safety / Unreachable
sut_path: lib/vector-buffers/src/variants/disk_v2/
commit: b7aae737cef5dd37d1445915443a1eb97b584f85
updated: 2026-05-28
linked_bugs:
  - vectordotdev/vector#21683
  - PR #23561 (metrics reporter only — control-path UNFIXED)
---

# Property: total-buffer-size-never-underflows

## Catalog Entry

**Type:** Safety / Unreachable

**Property:** The in-memory `total_buffer_size` AtomicU64 in `Ledger` is never
decremented by an amount greater than its current value; the atomic never wraps
toward u64::MAX.

**Invariant:** For every call to `decrement_total_buffer_size(amount)`:

```
amount <= self.total_buffer_size.load(Ordering::Acquire)
```

If this is violated the atomic wraps to approximately 2^64 − amount, which
permanently poisons `is_buffer_full()` (writer.rs:993-996) and deadlocks the
writer forever.

**Antithesis Angle:** Requires a node-kill fault at a file-rotation or
partial-write boundary, followed by restart. After restart `update_buffer_size`
(ledger.rs:653-697) re-seeds `total_buffer_size` from the *file sizes* of all
`.dat` files on disk. The reader then seeks forward through partially-written
records, calling `decrement_total_buffer_size` by *record bytes* — which are
strictly smaller than the file's on-disk size if the tail was never fully
written. This is the mismatch that causes underflow.

**Why It Matters:** This is the root cause of GitHub issue #21683. The control-path
atomic is still unfixed at this commit. PR #23561 applied `saturating_sub` only to
the metrics *reporter* (the gauge that users see on dashboards), not to the atomic
that gates the writer's progress. A wrapped value makes `is_buffer_full()` return
`true` permanently, starving the write path with no error, no log at ERROR level,
and no crash — the pipeline silently stalls.

---

## Trigger Paths (verified against source)

### Path A: seek-on-restart mismatch (primary trigger)

1. Writer writes a partial record to `buffer-data-N.dat` at the end of a 128MB
   data file, then crashes before `fsync` completes.
2. On restart, `Ledger::new` calls `update_buffer_size` (ledger.rs:653-697),
   which calls `increment_total_buffer_size(file_size_of_N)` — the *whole* file
   size, including the partial tail.
3. The reader calls `seek_to_next_record`, which calls `track_read` (reader.rs:448)
   for every record it skips, each time calling:

   ```rust
   // reader.rs:469
   self.ledger.decrement_total_buffer_size(record_bytes);
   ```

   where `record_bytes` is the serialized-record length for that valid record.
4. When the reader encounters the torn tail it stops. The sum of `record_bytes`
   decremented is less than the file size that was added at step 2, so
   `total_buffer_size` is positive and correct so far.
5. HOWEVER: when `delete_completed_data_file` is called (reader.rs:489) with
   `bytes_read = Some(self.bytes_read)`, the adjustment at reader.rs:521-535 is:

   ```rust
   let size_delta = metadata.len() - bytes_read;  // reader.rs:524
   ```

   This subtraction is plain `u64 -`; if `bytes_read > metadata.len()` (reachable
   if two decrements race or if the file was truncated externally) the subtraction
   panics in debug or wraps in release. More critically, this delta is then passed
   to `decrement_total_buffer_size` (reader.rs:538), which itself does:

   ```rust
   // ledger.rs:292
   self.total_buffer_size.fetch_sub(amount, Ordering::AcqRel);
   ```

   with no saturation guard.

### Path B: double-decrement on skip

When the reader "fast-forwards" (skips an entire file during initialization with
`bytes_read = None`), `delete_completed_data_file` passes the full `metadata.len()`
as the decrement amount (reader.rs:522). If the reader had already called
`track_read` for records inside that file (decrementing by their record bytes),
those bytes are subtracted twice: once via `track_read` and once via
`delete_completed_data_file`. The net decrement exceeds the original file-size
increment, causing underflow.

### Raw underflow site

```rust
// ledger.rs:291-298  (UNGUARDED — the fix in PR #23561 did NOT touch this)
pub fn decrement_total_buffer_size(&self, amount: u64) {
    let last_total_buffer_size = self.total_buffer_size.fetch_sub(amount, Ordering::AcqRel);
    trace!(
        previous_buffer_size = last_total_buffer_size,
        new_buffer_size = last_total_buffer_size - amount,  // also wraps in trace!
        "Updated buffer size.",
    );
}
```

Note: the `trace!` log at line 295 (`last_total_buffer_size - amount`) also wraps
and would emit a nonsensical value, making post-hoc diagnosis harder.

---

## Downstream Effect: Permanent Writer Deadlock

Once `total_buffer_size` wraps to ~u64::MAX, the following chain locks up:

1. `is_buffer_full` (writer.rs:993-996):

   ```rust
   fn is_buffer_full(&self) -> bool {
       let total_buffer_size = self.ledger.get_total_buffer_size() + self.unflushed_bytes;
       let max_buffer_size = self.config.max_buffer_size;
       total_buffer_size >= max_buffer_size  // always true: u64::MAX >= any max_size
   }
   ```

2. `ensure_ready_for_write` (writer.rs:1001-1019) enters an infinite `loop`:

   ```rust
   loop {
       if !self.is_buffer_full() || !self.ready_to_write {
           break;  // never taken: is_buffer_full() is always true
       }
       self.ledger.wait_for_reader().await;  // woken, re-checks, loops forever
   }
   ```

3. The reader *does* drain and delete files — calling `notify_reader_waiters()`
   each time (reader.rs:555) — but the writer wakes, re-evaluates `is_buffer_full`
   (still true), and blocks again immediately. The wakeups are real; the accounting
   is permanently wrong.
4. `can_write_record` (writer.rs:793-798) has the same `get_total_buffer_size()`
   call and is similarly poisoned.

The stall is externally **invisible**: no ERROR log, no metric spike, no panic.
The `buffer_events` / `buffer_byte_size` gauges may already be corrupted (stuck
at very large values), but PR #23561's `saturating_sub` in the reporter makes
the *dashboard* gauge appear normal at 0, hiding the deadlock.

---

## SUT-Side Instrumentation (MISSING — must be added)

All Antithesis SDK calls below are **absent** from the codebase (confirmed by
grep over the entire repo). The Antithesis Rust SDK must be added as a dependency.

### Assertion 1 — Unreachable guard on the decrement

```rust
// ledger.rs, inside decrement_total_buffer_size, before fetch_sub
let current = self.total_buffer_size.load(Ordering::Acquire);
antithesis_sdk::assert_unreachable!(
    "total_buffer_size underflow: amount exceeds current value",
    &serde_json::json!({
        "current_total_buffer_size": current,
        "decrement_amount": amount,
        "overflow_would_be": current.wrapping_sub(amount),
    })
);
// Alternatively, assert_always! framing:
antithesis_sdk::assert_always!(
    amount <= current,
    "decrement_total_buffer_size: amount must not exceed current value",
    &serde_json::json!({ "current": current, "amount": amount })
);
```

Placement: ledger.rs, in `decrement_total_buffer_size`, line ~291, before
`fetch_sub`.

### Assertion 2 — Unreachable guard on the reader.rs delta subtraction

```rust
// reader.rs:521-535, before the plain `-`
let file_size = metadata.len();
antithesis_sdk::assert_always!(
    bytes_read <= file_size,
    "delete_completed_data_file: bytes_read exceeds on-disk file size",
    &serde_json::json!({
        "file_size": file_size,
        "bytes_read": bytes_read,
        "data_file_path": data_file_path.to_string_lossy(),
    })
);
let size_delta = file_size - bytes_read;  // safe after the assertion
```

Placement: reader.rs, inside `delete_completed_data_file`, before line 524.

### Assertion 3 — Always: post-decrement value is sane

```rust
// ledger.rs, after fetch_sub in decrement_total_buffer_size
let new_value = last_total_buffer_size.wrapping_sub(amount);
antithesis_sdk::assert_always!(
    new_value <= last_total_buffer_size,
    "total_buffer_size decreased monotonically after decrement",
    &serde_json::json!({
        "before": last_total_buffer_size,
        "amount": amount,
        "after": new_value,
    })
);
```

### Assertion 4 — Reachable: the underflow recovery path is never needed

If a saturation fix is later applied (the correct fix), add an `assert_reachable!`
at the saturation branch to confirm Antithesis actually triggers the bug
scenario, so that the fix can be validated with the harness.

---

## Why Existing Tests Cannot Catch This

- The model-based proptest (`tests/model/`) uses `TestFilesystem` whose `sync_all`
  is a no-op and whose `flush` is a no-op. No partial writes are ever left on disk.
  `update_buffer_size` sees zero bytes in the in-memory filesystem, so the
  re-seed is always zero. The mismatch never materializes.
- The model's own `LedgerModel::decrement_buffer_size` mirrors the unguarded
  `fetch_sub` (it would reproduce the underflow if triggered), but the trigger
  is unreachable via the in-memory filesystem.
- `writer_waits_when_buffer_is_full` (`size_limits.rs`) is `#[ignore]` — this
  is the backpressure test that sits directly on the deadlock path.

---

## Open Questions

- **Is `update_buffer_size` the only re-seed path?** Confirm that there is no
  second call to `increment_total_buffer_size` during reader initialization that
  could compound the over-seed. (Current reading: only one call at ledger.rs:695.)

- **Does the double-decrement via Path B (fast-forward + track_read) actually
  occur in the current code, or is it prevented by the `!self.ready_to_read`
  guard at reader.rs:468?** The guard routes early seek-reads only through
  `decrement_total_buffer_size(record_bytes)` (not through the file-deletion
  path); but when the file is subsequently deleted, `bytes_read` captures those
  same bytes, and `delete_completed_data_file` subtracts them again via the
  `size_delta`. This is worth a focused code trace.

- **Node-termination faults enabled?** This bug requires kill-and-restart. Confirm
  with the Antithesis tenant operator whether node termination is enabled by
  default or must be requested.

- **Does the `trace!` at ledger.rs:295 (`last_total_buffer_size - amount`) panic
  in debug mode** before the bug can be observed? If running with `debug_assertions`,
  the trace format would panic on the wrapped arithmetic. This affects harness
  build mode selection.

- **PR #23561 scope:** Verify that the metrics reporter fix (using `saturating_sub`
  in `buffer_usage_data.rs`) is the *only* change, and that `decrement_total_buffer_size`
  in `ledger.rs` is definitively unchanged. (Confirmed at this commit: ledger.rs:292
  still uses raw `fetch_sub`.)

- **Fault timing specificity:** How early in a file-write does the crash need to
  occur for the partial record to trigger the mismatch? Does the 500ms fsync
  window create a large enough target? Or is the real trigger the file-rotation
  boundary (the very first write to a new data file, which is the most common
  partial-write scenario)?

---

### Investigation Log

#### Does the double-decrement via fast-forward + track_read during seek get fully blocked by the `!self.ready_to_read` guard (reader.rs:~468), or can both fire for the same bytes?

**Examined:** `reader.rs:464–539` (`track_read` and `delete_completed_data_file`).

**Found:** The guard at `reader.rs:468` (`if !self.ready_to_read`) short-circuits `track_read` so that only `decrement_total_buffer_size(record_bytes)` fires and `return` is executed — the per-record ack machinery below line 471 is skipped. Crucially, `bytes_read` is still incremented at `reader.rs:467` (`self.bytes_read += record_bytes`) regardless of the `ready_to_read` state. When `delete_completed_data_file` is later called with `bytes_read = Some(self.bytes_read)`, it computes `size_delta = metadata.len() - bytes_read` (reader.rs:524) and calls `decrement_total_buffer_size(size_delta)` (reader.rs:538). The sum of the two decrements — one in `track_read` for each valid record, one in `delete_completed_data_file` for the remaining unread tail — equals exactly `metadata.len()` when the file was fully read; in that case there is no double-decrement.

**The separate, unguarded site:** `delete_completed_data_file` at reader.rs:521–538 performs a plain `u64 -` subtraction (`metadata.len() - bytes_read`) with no saturation guard. If `bytes_read > metadata.len()` (reachable if two decrements race, the file was externally truncated, or a caller error passes an over-counted `bytes_read`), the subtraction wraps in release mode or panics in debug mode, and the resulting large `size_delta` is passed directly to `decrement_total_buffer_size` (ledger.rs:292), which calls `fetch_sub` with no bounds check. The `!self.ready_to_read` guard at reader.rs:468 does NOT protect this site — it only guards the per-record `record_acks.add_marker` call. The delete-time subtraction is a distinct, unguarded decrement path.

**Conclusion:** Path B double-decrement (fast-forward case where `bytes_read = None` skips all `track_read` calls and `delete_completed_data_file` subtracts the full `metadata.len()`) is guarded by the `None` branch in the `bytes_read.map_or_else` at reader.rs:521 — in that case the full file size is decremented exactly once, not twice. The underflow risk in Path B is therefore not a double-decrement from track_read + delete, but from the re-seed in `update_buffer_size` exceeding what records actually represent, per Path A. The unguarded `metadata.len() - bytes_read` plain subtraction at reader.rs:524 remains a correctness risk for partial-read cases.

---

## Test plan (2026-06-02, expert-chorus decision)

**Home:** in-tree `proptest` property test in `lib/vector-buffers`, NOT an Antithesis
scenario. The bug is deterministically reproducible in-process; Antithesis would add only
scheduler/coverage. (Antithesis keeps the distributed `vector_to_vector_e2e_disk`
conservation experiment + the #24948 SIGHUP reload fault.)

**This site (the byte-count underflow) requires a REOPEN** — it is NOT reachable purely
in-process. `decrement_total_buffer_size` (ledger.rs:292 `fetch_sub`) and the reader's
delete-time `metadata.len() - bytes_read` (reader.rs:524) only disagree across a restart:
`from_config_inner` → `update_buffer_size` (ledger.rs:653-697) re-seeds `total_buffer_size`
from the SUM of on-disk `.dat` bytes, then the reader's seek decrements per replayed record
(reader.rs:469/538). They diverge only when on-disk file size exceeds valid-record bytes —
a torn / partial tail.

**Test B — `tests/reopen.rs`** (`current_thread` runtime + in-memory `TestFilesystem`):
1. Write / read / ack a generated prefix through the real buffer.
2. Drop reader + writer + ledger, then drain the spawned finalizer task via `yield_now` so
   it releases the ledger lock (it holds an `Arc<Ledger>`; basic.rs:161-164, ledger.rs:700-710;
   precedent model/mod.rs:890). On `current_thread` the task will not release until polled —
   without this, reopen hits `LedgerLockAlreadyHeld`.
3. INJECT a torn tail: truncate the last `.dat` by a generated N bytes via `TestFilesystem`
   introspection. LOAD-BEARING — `TestFilesystem` has no-op `sync_all`/`flush` and
   always-complete writes, so without injection it can never produce the divergence and the
   test cannot reach the bug.
4. Reopen via `from_config_inner` (same fs + data_dir) under a `timeout`.
5. Assert the underflow directly (NOT a `< ceiling`): `total_buffer_size` did not wrap high
   (no value near `u64::MAX`) and `bytes_read <= file_size`. `proptest` over
   {prefix len, truncation bytes, max_data_file_size}; commit a regression seed; demote to a
   plain `#[test]` if the space collapses.

**Discipline:** the test must FAIL on current code (TDD); PR #23561 only saturated the gauge
*reporter* (`buffer_usage_data.rs`), leaving the control-path `fetch_sub` broken, so a fix
must address the control path. Do NOT extend `model_check` with a reopen action — the model
would have to re-derive `update_buffer_size` (the logic under test → false failures) and its
no-op filesystem can't make the torn tail anyway. No `sane`/`insane` in names.
