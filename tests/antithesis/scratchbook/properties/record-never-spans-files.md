# Evidence: record-never-spans-files

## Property Identification

**Slug:** record-never-spans-files
**Type:** Safety / AlwaysOrUnreachable
**Assertion macro:** `assert_always_or_unreachable!("record_never_spans_files", "Record is fully contained within a single data file", ...)`

This property was identified by reading `RecordWriter::can_write` (writer.rs:433-437) and `BufferWriter::can_write` (writer.rs:789-791), which together form a two-level gate that prevents writing a record that would not fit in the current data file. The design doc (mod.rs) and `_external-references-digest.md` both state this as an explicit invariant: "records written sequentially/contiguously; a record never spans two data files."

The property is `AlwaysOrUnreachable` because: on a correct run, records always fit within a single file (the gate prevents spanning); the "record spans files" branch is unreachable. Under faults (corrupted file size metadata, overflowed size counter), the gate could be fooled into allowing a record that would overflow the file — which is the violation. The assertion fires only in the violation case, hence "always or unreachable."

## Code Chain Leading to the Property

### Write-Side Gate: `RecordWriter::can_write` (writer.rs:433-437)

```rust
fn can_write(&self, amount: usize) -> bool {
    let amount = u64::try_from(amount).expect("`amount` should need ever 2^64 bytes.");
    self.current_data_file_size + amount <= self.max_data_file_size
}
```

Called in `archive_record` at writer.rs:527 after the record has been fully serialized into `self.ser_buf`, so `amount = serialized_len` is the exact wire size including the 8-byte length delimiter:

```rust
// writer.rs:527-548
if !self.can_write(serialized_len) {
    // Decode the record back out to return it.
    let record = T::decode(T::get_metadata(), &self.encode_buf[..]).map_err(|_| {
        WriterError::InconsistentState { ... }
    })?;
    return Err(WriterError::DataFileFull {
        record,
        serialized_len,
    });
}
```

If the record would not fit, `archive_record` returns `Err(WriterError::DataFileFull { record, serialized_len })` — the record is recovered from the encode buffer and returned to the caller. The record is NOT written to disk.

### Upper-Level Gate: `BufferWriter::can_write` (writer.rs:789-791)

```rust
fn can_write(&self) -> bool {
    !self.data_file_full && self.data_file_size < self.config.max_data_file_size
}
```

This is checked in `ensure_ready_for_write` (writer.rs:1029) and `try_write_record_inner` (writer.rs:1176). It operates on `self.data_file_size`, which is the `BufferWriter`-level tracking of how many bytes have been written to the current data file. When `can_write()` is false, the writer triggers file rotation (writer.rs:1040-1044) before attempting to write.

The two-level design is:

1. `BufferWriter::can_write()` — coarse gate using `data_file_size` to decide when to rotate proactively.
2. `RecordWriter::can_write(amount)` — precise gate using `current_data_file_size + serialized_len <= max_data_file_size` after the record is serialized.

### File Rotation on `DataFileFull` (writer.rs:1204-1223)

When `archive_record` returns `Err(WriterError::DataFileFull { record, serialized_len })`, `try_write_record_inner` captures it:

```rust
// writer.rs:1204-1223
WriterError::DataFileFull {
    record: old_record,
    serialized_len,
} => {
    self.mark_data_file_full();
    record = old_record;
    // ...loop continues, calling ensure_ready_for_write which triggers rotation
}
```

`mark_data_file_full()` sets `self.data_file_full = true` (writer.rs:801-804), which causes `can_write()` to return false and forces `ensure_ready_for_write` to open the next file. The record is then re-attempted on the new file.

### The Soft Overshoot: `max_record_size` vs. `max_data_file_size`

The design deliberately allows a single record to exceed `max_data_file_size` if the file is currently empty (writer.rs comment preceding `can_write` at writer.rs:429-432):
> "If no bytes have written at all to a data file, then `amount` is allowed to exceed the limit, otherwise a record would never be able to be written."

Wait — checking `can_write`:

```rust
fn can_write(&self, amount: usize) -> bool {
    let amount = u64::try_from(amount).expect("...");
    self.current_data_file_size + amount <= self.max_data_file_size
}
```

When `self.current_data_file_size == 0`, the check reduces to `amount <= max_data_file_size`. If `amount > max_data_file_size`, this returns false even on an empty file. But the code has a `debug_assert` in `RecordWriter::new` (writer.rs:396-399):

```rust
debug_assert!(
    max_data_file_size >= max_record_size_converted,
    "must always be able to fit at least one record into a data file"
);
```

The default `max_data_file_size = 128MB`, `max_record_size = 128MB` (from `DEFAULT_MAX_DATA_FILE_SIZE`, `DEFAULT_MAX_RECORD_SIZE` in `common.rs`). With `max_data_file_size == max_record_size`, the assertion holds but with zero margin — a record of exactly `max_record_size` bytes would land at the limit. A record larger than `max_record_size` is rejected by the `RecordTooLarge` error in `archive_record` (writer.rs:477-480) before `can_write` is even called.

So the soft overshoot up to `max_record_size` is actually not a spanning-files scenario — it means a single large record can occupy an entire file. The file never contains more than `max_data_file_size + max_record_size` bytes in the degenerate case (the first record is large, up to `max_record_size`; subsequent records are blocked from the file but accumulate on the next). The bound documented in sut-analysis.md §5/INV-3 as "~2×" refers to this: in the worst case, one very large record is written, then a second record is written that barely doesn't fit, triggering rotation. The file on disk holds up to `max_data_file_size` (first record is small) + potentially slightly over for the `current_data_file_size + amount <= max_data_file_size` bound.

### Read-Side Constraint

The reader's `try_next_record` (reader.rs:306-351) reads a length delimiter then reads exactly `record_len` bytes from the same file. There is no logic in the reader to span a file boundary: `BufReader<FS::File>` reads from a single open file handle, and reaching EOF causes `try_next_record` to return `Ok(None)` (or `Err(PartialWrite)` if `is_finalized`). The reader never attempts to continue reading a record from the next file. The invariant at read time is: if a record's length delimiter claims N bytes follow, those N bytes must all be in the current file. If they are not, `is_finalized=true` gives `PartialWrite`; if `is_finalized=false` (writer still writing), the reader waits for more data from the writer, but since the writer will never append the missing bytes (it has closed/rotated this file), the reader will eventually see EOF with `is_finalized=true` and detect `PartialWrite`.

The read-side thus enforces the invariant defensively, but only catches a violation after the fact (as a `PartialWrite` error). The property assertion should live on the write side, where the violation is prevented.

### `current_data_file_size` Tracking (writer.rs:629-631)

```rust
// In flush_record (writer.rs:629-631):
self.current_data_file_size += u64::try_from(serialized_len)
    .expect("Serialized length of record should never exceed 2^64 bytes.");
```

This is a plain `u64` addition with no saturation check. If `serialized_len` is somehow larger than `u64::MAX - current_data_file_size` (astronomically unlikely in practice), this would overflow. Not a realistic fault path; the `try_from` would panic first since `serialized_len` is a `usize` checked against `max_record_size < 128MB`.

More concerning: `current_data_file_size` is initialized from the on-disk file size at writer open (writer.rs:1094-1101, passing `file_len` to `RecordWriter::new`). If the file metadata is corrupted (e.g., via external truncation or Antithesis filesystem fault), `file_len` could be wrong. An underreported `file_len` would cause `can_write` to allow writing past the intended limit; an overreported `file_len` would cause premature rotation.

Similarly, `BufferWriter::data_file_size` is initialized from the same `file_len` via `self.data_file_size = data_file_size` (writer.rs:1133). So both gates are seeded from on-disk metadata at open.

## What Goes Wrong if the Property is Violated

If a record's bytes span a file boundary, the reader would read the length delimiter correctly, then read `record_len` bytes — which runs off the end of the file. In practice on Linux, reading past EOF on a regular file returns 0 bytes (EOF); with `is_finalized=true`, this triggers `ReaderError::PartialWrite`. The reader would roll to the next file and the spanning record would be lost. This is a safe degradation, but:

1. **Data loss**: the spanning record is permanently lost, not recoverable on restart.
2. **`total_buffer_size` drift**: the bytes of the spanning record were accounted for on the write side but the incomplete record's bytes are corrected (partially) by the `size_delta` logic in `delete_completed_data_file` (reader.rs:521-535). Whether this is correctly computed depends on how many bytes were actually written to the first file vs. how many the reader observed.
3. **Record ID gap**: the spanning record has a valid ID assigned; if it is lost, the `OrderedAcknowledgements` inserts a gap marker, which causes `events_skipped` to increment and `track_dropped_events` to fire.

## Timing / Fault Conditions for Antithesis

- **Corrupted `file_len` from `metadata().await`**: If Antithesis can corrupt the filesystem metadata call result to return a lower value than the actual file size, `RecordWriter::new` initializes `current_data_file_size` too low, and `can_write` allows writing past the true limit. The record bytes are appended after what the writer thinks is the file boundary.
- **Concurrent external write to the data file**: Another process (or Vector instance due to lock-bypass) appends bytes to the data file. The writer's `current_data_file_size` is now stale (low), causing it to think there is more room than there is.
- **`data_file_size` underflow via overflow-then-wrap**: Not realistic at normal sizes, but in a fuzz scenario with `max_data_file_size` set to a small value (near `u64::MIN`), a record that is allowed could overflow the accumulator.
- **Race between writer file rotation and reader file ID increment**: Not directly a spanning violation, but if the writer increments the file ID (`increment_writer_file_id` at writer.rs:1138) while a record is still being written, the reader might believe the file is finalized while bytes are still being appended. This is not a spanning violation but a timing hazard in the `is_finalized` flag that affects the partial-write detection path.

## SUT-Side Instrumentation Suggestions (ALL MISSING)

**Primary assertion (write side)** — in `RecordWriter::flush_record` (writer.rs:609-633), after `current_data_file_size` is updated, assert the invariant:

```rust
// writer.rs, after line 631:
antithesis_sdk::assert_always_or_unreachable!(
    "record_never_spans_files",
    "After flush_record, data file size does not exceed max_data_file_size",
    &serde_json::json!({
        "current_data_file_size": self.current_data_file_size,
        "max_data_file_size": self.max_data_file_size,
        "serialized_len": serialized_len,
    })
);
// Specifically: current_data_file_size <= max_data_file_size (allowing first-record overshoot is a design choice; the invariant is that we never write a partial record across a boundary)
```

Note: the first record on an empty file is allowed to fill up to `max_data_file_size` (since `can_write` checks `<= max_data_file_size`, not `< max_data_file_size`). A stronger assertion would check that `current_data_file_size` after flush equals `current_data_file_size_before + serialized_len`, confirming no corruption of the size counter.

**Secondary assertion (read side)** — in `RecordReader::try_next_record` (reader.rs:306-351), when `PartialWrite` is returned, check whether the partial write is at a position consistent with a spanning violation vs. a genuine incomplete write:

```rust
// reader.rs, in the PartialWrite return path:
antithesis_sdk::assert_always_or_unreachable!(
    "partial_write_not_due_to_spanning",
    "PartialWrite detected; this should only occur due to crash-interrupted writes, not record spanning",
    &serde_json::json!({
        "bytes_accumulated": self.aligned_buf.len(),
        "record_len_claimed": record_len,
        "reader_file_id": ...,
    })
);
```

This makes the partial-write detection path a reachability check and also documents the expected cause.

**Gate audit assertion** — in `RecordWriter::can_write` (writer.rs:433-437), assert that we never call `can_write` with a `current_data_file_size` that already exceeds `max_data_file_size` (which would indicate the size counter drifted):

```rust
fn can_write(&self, amount: usize) -> bool {
    let amount = u64::try_from(amount).expect("...");
    antithesis_sdk::assert_always_or_unreachable!(
        "data_file_size_counter_not_drifted_above_max",
        "current_data_file_size should not exceed max_data_file_size before can_write is checked",
        &serde_json::json!({
            "current_data_file_size": self.current_data_file_size,
            "max_data_file_size": self.max_data_file_size,
        })
    );
    self.current_data_file_size + amount <= self.max_data_file_size
}
```

## Open Questions

- **Is there a window between `can_write` returning true and `flush_record` completing where a concurrent size update could invalidate the gate?** The writer is single-threaded (behind a topology `Mutex`), so no interleaving is possible between `archive_record`'s `can_write` check and `flush_record`'s size update within a single tokio task. However, if the `Mutex` is somehow released between these calls (which it should not be), a race would be possible. Confirming that `archive_record` and `flush_record` are always called within the same `Mutex` lock scope is needed. If they are (they appear to be — both are called from `try_write_record_inner` which holds the lock end-to-end), the gate is sound against concurrency.

- **What is the actual on-disk file size upper bound for a single data file?** The documented limit is `max_data_file_size = 128MB`. The code's `can_write` check is `current_data_file_size + amount <= max_data_file_size`. If a record of exactly `max_data_file_size` bytes is written to an empty file, `current_data_file_size` after the write equals `max_data_file_size`. The next `can_write` check for any subsequent record will fail (since `max_data_file_size + any_positive_amount > max_data_file_size`). So the file can reach exactly 128MB but not exceed it for the initial record, and subsequent records trigger rotation. The "~2×" bound from the SUT analysis appears to be incorrect, or refers to an older behavior. Confirming the actual maximum file size by testing with `max_record_size = max_data_file_size` would resolve this.

- **Does the `debug_assert` at writer.rs:396-399 (`max_data_file_size >= max_record_size`) fire in release builds?** `debug_assert!` is compiled out in release mode. If a user configures `max_record_size > max_data_file_size` (which would require exposing `max_record_size` to users, currently not done), the assert would not catch it. The invariant that "at least one record always fits in a data file" would then be violated at the `can_write` level: a single record would return `DataFileFull` even on an empty file, and the writer would loop forever trying to rotate to a new file that is also too small. This is a separate stall bug, but it interacts with the file-spanning property.

- **Is `current_data_file_size` reset to 0 when the writer opens a new data file?** Yes: `self.reset()` (writer.rs:806-811) sets `self.data_file_size = 0`, and `RecordWriter::new` is called with `data_file_size` from on-disk metadata (writer.rs:1094-1101). For a freshly created file, `metadata.len() == 0`, so `current_data_file_size = 0`. For an existing file (resumed after crash), `metadata.len()` is the true on-disk size, correctly seeding the gate. This seems correct, but if Antithesis corrupts `metadata().len()` to return a lower value than the true on-disk size, `current_data_file_size` is initialized too low, creating the spanning risk.
