# Property: partial-write-at-rotation-recovers

## Catalog Entry

**Type:** Safety + Liveness / Sometimes — `Sometimes(torn_tail_recovered)`

**Property:** A crash that leaves a torn/partial last record in a data file, an
empty just-created next file, or a ledger/data divergence at the file-rotation
boundary is recovered without deadlock and without returning garbage data or
fast-forwarding to a wrong record ID. The system reaches a consistent state
where the writer is ready to write and the reader is ready to read correct
records.

**Invariant (Safety):** After recovery from any rotation-boundary crash, no
record returned by `reader.next()` may have:

- A checksum mismatch (garbage payload delivered as valid).
- A record ID that is non-monotonic relative to the previous delivered ID
  (would panic at `reader.rs:482` `MonotonicityViolation`).
- A record ID synthesized from a torn rkyv footer read (F5: `archived_root`
  reads root pointer from last 8 bytes of buffer — if those bytes are crash-
  left garbage, the pointer may be plausible but wrong, yielding a `Valid`
  status with an incorrect `id` field that fast-forwards the ledger).

**Invariant (Liveness):** After recovery from any rotation-boundary crash,
`from_config_inner` completes and the writer accepts new writes within bounded
time (no deadlock on `wait_for_reader()`).

**Invariant (No phantom gap):** The `Ordering::Greater` path in
`validate_last_write` (`writer.rs:910-919`) may log "Events have likely been
lost" and skip to the next file — this gap must only occur when real data
divergence exists, not as a false positive triggered by a mis-read torn tail.
A false-positive skip silently discards valid synced records.

**Antithesis Angle:**

1. Workload drives the buffer to trigger file rotation regularly (write records
   until `RecordWriter::can_write` returns false, at `DEFAULT_MAX_DATA_FILE_SIZE`
   = 128MB, or more practically at a reduced `max_data_file_size` configuration).
2. Antithesis injects SIGKILL precisely during the rotation sequence (see
   windows below).
3. Vector restarts; workload asserts:
   - `Sometimes(torn_tail_recovered)`: the recovery path that handles a torn or
     partial last record (`RecordStatus::FailedDeserialization` or
     `RecordStatus::Corrupted` in `validate_record_archive`) is actually reached
     and handled.
   - `Always`: no garbage record (bad checksum, bad ID) is returned to the
     workload from `reader.next()`.
   - `Always`: no deadlock during init.

**Why It Matters:** File rotation is the most crash-sensitive state transition
in the buffer. It involves: (1) flushing the current file with `sync_all`, (2)
ledger msync, (3) creating a new data file with `open_file_writable_atomic`,
(4) `sync_all` the empty new file, (5) `increment_writer_file_id` in the ledger.
None of these steps are atomic with each other. A kill at any of the 5 seams
leaves a different disk state, each requiring a different recovery branch.

The F5 risk (`archived_root` / `check_archived_root` reads root pointer from
last 8 bytes of the buffer at `ser.rs:94` `check_archived_root::<T>(buf)`) is
the most subtle: rkyv's archived format stores the root object's position as a
relative offset in the last `size_of::<usize>()` bytes of the buffer. If the
last record's write was torn mid-payload (e.g., only the first 4 bytes of the 8-
byte footer were written before the kill), `check_archived_root` reads a
half-written value, interprets it as a plausible relative pointer, and may
navigate to a valid-looking-but-wrong byte offset within the buffer. If that
offset happens to contain bytes that pass `CheckBytes` validation (which is
hand-written at `record.rs:79-117` and only validates field types, not semantic
constraints), the result is `RecordStatus::Valid` with an incorrect `id` field.

This false-valid record then propagates to `validate_last_write`'s comparison:
if the wrong `id` + `record_events` > `ledger_next`, the `Ordering::Greater`
path fires and silently drops all synced records from that file. If the wrong
`id` + `record_events` < `ledger_next` (Less), the ledger fast-forwards to an
even larger ID, creating a phantom gap.

**Crash Windows at Rotation Boundary (code-precise):**

| Step | Code location | Kill here leaves... | Recovery branch |
|------|--------------|--------------------|-----------------|
| 1. `flush_inner(force=true)` flushing old file | `writer.rs:1041` → `writer.rs:1307-1308` `writer.flush()` (page-cache only) | Old file in page cache, no fsync | `validate_last_write` opens old file; last record valid → `Ordering::Equal`; ready to continue on old file |
| 2. `sync_all` of old file | `writer.rs:1314` `writer.sync_all()` | Old file partially synced (OS may batch) | Last record may be partial (torn tail) → `FailedDeserialization` or `Corrupted` → `should_skip=true` → skip to next |
| 3. `ledger.flush()` (msync) | `writer.rs:1317` `self.ledger.flush()` | Old file synced; ledger not updated | `validate_last_write` reads last record; ledger lags → `Ordering::Less`; fast-forward; OK |
| 4. `reset()` closes old file handle | `writer.rs:1044` `self.reset()` | Old file synced and ledger synced; writer in reset state | `validate_last_write` re-opens old file; last record valid; `Ordering::Equal` |
| 5. `open_file_writable_atomic` for new file | `writer.rs:1071` | New file may not exist, or empty O_CREAT result | `AlreadyExists` branch; if file is empty → `data_file_size==0`; treat as new. If file doesn't exist, creates it fresh |
| 6. `sync_all` of empty new file | `writer.rs:1124` `data_file.sync_all()` | New file exists but not durable on disk | Next init: `open_file_writable` opens it; size=0; `validate_last_write` exits early at `writer.rs:852-855` |
| 7. `increment_writer_file_id` in ledger | `writer.rs:1138` `ledger.state().increment_writer_file_id()` | New file open; ledger still says old file ID | `validate_last_write`: opens file pointed by ledger (old ID). If old file has valid last record → `Ordering::Equal`; writer resumes on old file. But new empty file is orphaned |
| 8. After `increment_writer_file_id`, before first write to new file | `writer.rs:1138` done; no records yet | Ledger on new file ID; file is empty | `validate_last_write`: opens new file; empty → early exit at `writer.rs:852-855`; ready to write |

**F5 Torn-Tail Mis-Recovery Path:**

The F5 risk materializes in windows 2-3 above. The precise sequence:

1. Writer is writing the last record of the old file (near the 128MB boundary).
2. `TrackingBufWriter.flush()` (page-cache flush) starts writing the rkyv
   archive to the data file.
3. The write is torn: the kernel writes only part of the serialized archive
   (e.g., the payload is written but the 8-byte rkyv footer — the root pointer
   — is partially written or not written).
4. Kill occurs.
5. On restart, `validate_last_write` opens the old file as an mmap (via
   `open_mmap_readable` at `writer.rs:862-867`).
6. `validate_record_archive` at `writer.rs:872` calls `try_as_record_archive`
   → `try_as_archive::<Record<'_>>(buf)` → `check_archived_root::<Record<'_>>(buf)`
   at `ser.rs:94`.
7. `check_archived_root` reads the last 8 bytes of `buf` as the root offset.
   If those bytes are garbage (partial write), they may happen to encode a
   plausible relative offset that lands within the buffer.
8. The `CheckBytes` implementation for `ArchivedRecord` at `record.rs:79-117`
   validates field types (`u32`, `u64`, `u32`, `ArchivedBox<[u8]>`) but does
   not validate semantic constraints (e.g., that `id` is monotonically greater
   than the ledger's `last_record_id`).
9. If the garbage-pointed bytes pass `CheckBytes`, `validate_record_archive`
   returns `RecordStatus::Valid { id: <garbage_id> }`.
10. `validate_last_write` proceeds to compare `garbage_id + record_events` vs
    `ledger_next`. Any comparison outcome can result: `Greater` (silent skip),
    `Less` (phantom fast-forward), or even `Equal` (lucky match that accepts
    garbage as valid last record, causing wrong `next_record_id`).

The CRC32C check (`archive.verify_checksum` at `record.rs:179`) is the second
gate: even if the archive struct is parsed, if the garbage bytes don't match
the checksum, `RecordStatus::Corrupted` is returned (→ `should_skip = true`,
safe). The F5 risk materializes only if the garbage bytes happen to produce a
CRC32C collision with the (also partially-written) payload — a low but nonzero
probability per restart. Antithesis's full coverage of timing windows makes this
reachable over many restarts across all explored timelines.

**`validate_last_write` `Ordering::Greater` / `Ordering::Less` paths:**

- `Ordering::Less` (`writer.rs:922-944`): ledger behind data → fast-forward
  ledger. The `ledger_record_delta = record_next - ledger_next` is computed and
  used to `increment_next_writer_record_id`. This is safe when the last record
  is genuinely valid. If F5 produces a false-valid record with a lower `id`
  than the true last record, the fast-forward moves `next_record_id` forward
  by the wrong amount — subsequent records may have unexpected IDs relative to
  the reader's expectations.

- `Ordering::Greater` (`writer.rs:910-919`): data behind ledger → log error,
  `should_skip = true`. This path is taken when the last record's `id + events`
  is less than `ledger_next` — meaning the ledger thinks we wrote more records
  than actually made it to the file. On skip, the writer rolls to the next file
  and never writes the "missing" records again. This is the intended behavior
  for partial writes, but must not be triggered by a F5 false-valid record with
  a too-low garbage `id`.

**`seek_to_next_record` at Rotation:**

During recovery, `seek_to_next_record` at `reader.rs:851-907` uses the same
`validate_record_archive` (mmap + `check_archived_root`) for its fast-path
file skip check. F5 can also manifest here: a false-valid last record with a
wrong `id` may incorrectly satisfy `ledger_last > last_record_id_in_data_file`
at `reader.rs:890`, causing the reader to delete a file it should not have
deleted (all remaining unread records in that file are lost).

**`reader.rs:943` `u16 >` comparison (file-ID rollover):**
`if reader_file_id > writer_file_id` at `reader.rs:943` uses raw `u16`
comparison. At `MAX_FILE_ID = 65535`, after rollover the reader file ID wraps
to 0 while the writer may be at 65535. `0 > 65535 == false`, so the
`seek_to_next_record` init stall condition is not detected — the reader
incorrectly believes it is still behind the writer and continues looping. This
is the file-ID rollover ordering bug (sut-analysis §6 item 6). Antithesis can
reach this with `MAX_FILE_ID` reduced via test configuration.

**Fault Requirements:** Node-termination faults (SIGKILL) required. Kill
precisely during the rotation sequence (windows 1-8 above) is the primary fault.
The Antithesis scheduler should concentrate kills in the time window between:

- The first `flush_inner(force=true)` call (start of rotation, `writer.rs:1041`)
- The first successful write to the new file (end of rotation, after
  `writer.rs:1138`).

To maximize rotation-boundary hits, configure a small `max_data_file_size`
(e.g., 1MB or even 256KB) so rotations happen frequently, giving the Antithesis
scheduler many opportunities.

**Antithesis SDK Assertions (SUT-side):**

The Antithesis SDK is a committed dependency under the `antithesis` feature, and three `assert_always_greater_than_or_equal_to!` underflow detectors already ship (ledger.rs:271, ledger.rs:313, reader.rs:529 — see `existing-assertions.md`). None of them covers the rotation-recovery paths, so the assertions below are genuine still-to-add suggestions:

```rust
// In validate_last_write, after RecordStatus::FailedDeserialization or Corrupted:
antithesis_sdk::assert_sometimes!(
    true,
    "torn_tail_recovered: validate_last_write detected corrupt/partial last record",
    json!({
        "data_file": format!("{:?}", data_file_path),
        "status": "FailedDeserialization or Corrupted"
    })
);

// In validate_last_write, after Ordering::Greater:
antithesis_sdk::assert_sometimes!(
    true,
    "validate_last_write Ordering::Greater path exercised (data lags ledger)",
    json!({ "ledger_next": ledger_next, "record_next": record_next })
);

// In validate_last_write, after Ordering::Less:
antithesis_sdk::assert_sometimes!(
    true,
    "validate_last_write Ordering::Less path exercised (ledger lags data)",
    json!({ "ledger_next": ledger_next, "record_next": record_next })
);

// In seek_to_next_record, after delete_completed_data_file during fast-path:
antithesis_sdk::assert_sometimes!(
    true,
    "seek_to_next_record fast-path: deleted already-acked file during recovery",
    json!({ "file": format!("{:?}", data_file_path) })
);

// After any record delivered to caller (reader.rs next() Ok(Some(record))):
antithesis_sdk::assert_always!(
    // record_id must be >= last delivered record_id (monotonicity)
    record_id >= self.last_reader_record_id,
    "record IDs are strictly monotonic (no wrap-around or garbage ID delivered)",
    json!({ "record_id": record_id, "last_reader_record_id": self.last_reader_record_id })
);
```

---

## Open Questions

**OQ-1: Does `check_archived_root` actually read from the last 8 bytes of the
buffer (F5 torn-tail risk) or does it use a different footer layout?**
`rkyv` v0.7.x (the version in use — check `Cargo.lock`) stores the root
position as a `i32` relative offset in the **last 4 bytes** of the buffer on
32-bit or as a `usize`-sized footer on 64-bit. On x86-64 with a 64-bit `usize`,
the footer is 8 bytes. The exact layout determines how many bytes need to be
torn for F5 to be triggered. Check `rkyv`'s version in
`lib/vector-buffers/Cargo.toml` and the footer layout for that version.

**OQ-2: Is the F5 probability high enough to matter in practice, or is CRC32C
the effective guard?**
For F5 to produce a false-valid (not just `Corrupted`) record, the garbage
bytes at the root-pointer position must both (a) point to a location within the
buffer that passes `CheckBytes` and (b) the payload bytes at that location must
CRC32C-match the checksum field (also potentially garbage). The CRC32C check is
strong (32-bit security margin against random bit flips). However, partial
writes at the torn boundary may leave structured data (zeros, the previous
record's valid bytes) that creates a non-random collision surface. Antithesis's
full coverage of timing windows is the right tool to empirically determine if
F5 is reachable without probability arguments.

**OQ-3: When `should_skip_to_next_file = true` and the writer rolls to the next
file, does the reader still have access to all un-deleted records in the old
file?**
Yes: `mark_for_skip` (`writer.rs:984`) + `reset()` closes the writer's handle
to the old file but does not delete it. The reader reads/deletes data files at
its own pace (`delete_completed_data_file` only after all records acked). The
concern is whether the `increment_writer_file_id` in `validate_last_write`'s
skip path causes the `is_finalized` flag in the reader to flip prematurely,
marking the old file as finalized before all records are written. Trace
`is_finalized = (reader_file_id != writer_file_id) || !self.ready_to_read`
(`reader.rs:1015`): after skip, `writer_file_id` increments → `!=` reader's
file_id → `is_finalized = true`. This correctly signals to `try_next_record`
that the file is done and partial reads at the end are `PartialWrite` errors,
not waits. This is correct behavior.

**OQ-4: Is the monotonicity panic at `reader.rs:482` reachable via the F5
path?**
If F5 produces a garbage `id` that is lower than `self.last_reader_record_id`,
the `add_marker` call at `reader.rs:478` would return `MonotonicityViolation`,
which panics with `"record ID monotonicity violation detected; this is a
serious bug"`. This would be a process crash (not a deadlock), which is more
visible than silent loss but still unrecoverable. However, F5 happens in
`validate_last_write` (writer init) which reads the last record but does not
call `add_marker` — the reader's monotonicity check is on the read path
(`read_record` → `track_record` → `add_marker`). If F5 sets the writer's
`next_record_id` to a wrong value, and the writer then writes new records with
IDs starting from that wrong value, the reader may encounter those records with
IDs that are non-monotonic relative to surviving old records. This is the
indirect path to the monotonicity panic.

**OQ-5: Should `max_data_file_size` be configurable in test mode to a small
value (e.g., 1MB) to trigger frequent rotations?**
Yes. The test configuration should set `max_data_file_size` to a small value
(recommended: 1MB or even 256KB) to make rotations happen every few seconds,
giving Antithesis many rotation-boundary crash opportunities per run. At the
default 128MB, a single test run may not produce enough rotations for full
coverage.
