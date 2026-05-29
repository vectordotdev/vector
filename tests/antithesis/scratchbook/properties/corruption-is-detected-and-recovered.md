# Evidence: corruption-is-detected-and-recovered

## Property Identification

**Slug:** corruption-is-detected-and-recovered
**Type:** Reachability / Sometimes(`corruption_detected_and_recovered`)
**Assertion macro:** `assert_sometimes!("corruption_detected_and_recovered", "Corruption was detected by is_bad_read and the reader rolled to the next data file", ...)`

This property is the fault-injection confirmation counterpart to `no-corrupted-record-delivered`. It is not enough to know that corruption never leaks through; we must also confirm that when Antithesis actually injects a fault (bit-flip, partial write, torn file), the detection and recovery path actually executes. Without this reachability assertion, a test run that injects faults but never reaches the recovery branch gives zero confidence in the guard.

The property was identified by reading the error handling in `BufferReader::next` (reader.rs:1009-1040) and the `is_bad_read` predicate (reader.rs:132-139), and noting that the entire recovery branch (`roll_to_next_data_file`) could be dead code if the fault injection strategy never produces bytes that fail checksum or deserialization in a live read.

## Code Chain Leading to the Property

### Detection: `is_bad_read` (reader.rs:132-139)

```rust
fn is_bad_read(&self) -> bool {
    matches!(
        self,
        ReaderError::Checksum { .. }
            | ReaderError::Deserialization { .. }
            | ReaderError::PartialWrite
    )
}
```

This predicate collects the three error variants that indicate "the data file is untrustworthy from this point on." `ReaderError::Io` and `ReaderError::EmptyRecord` are deliberately excluded: I/O errors might be transient, and empty records are a writer-side logic error, not corruption.

### Recovery: `roll_to_next_data_file` (reader.rs:694-741)

Called from `BufferReader::next` at reader.rs:1036 when `e.is_bad_read()` is true:

```rust
// reader.rs:1031-1039
Err(e) => {
    if e.is_bad_read() {
        self.roll_to_next_data_file();
    }
    return Err(e);
}
```

`roll_to_next_data_file` (reader.rs:694-741):

- Captures `data_file_start_record_id`, `last_reader_record_id`, `data_file_record_count`, `bytes_read`, and the current data file path from the ledger.
- Adds a data-file deletion marker to `self.data_file_acks` (reader.rs:729-735), which eventually drives `delete_completed_data_file` once all records up to that point are acknowledged.
- Calls `self.reset()` (reader.rs:738), which sets `self.reader = None`, zeroes `bytes_read`, and clears `data_file_start_record_id`.
- Calls `self.ledger.increment_unacked_reader_file_id()` (reader.rs:739), advancing the reader to the next data file.

After `roll_to_next_data_file`, the reader's next call to `ensure_ready_for_read` will open the next file and continue. The buffer does not halt; it absorbs the loss of the rest of that file's records.

Also called at reader.rs:1075 in the `Ok(None)` path (end-of-file when `reader_file_id != writer_file_id`):

```rust
self.roll_to_next_data_file();
force_check_pending_data_files = true;
continue;
```

And at reader.rs:912 in `seek_to_next_record` (during initialization bad-read handling), though that path uses a slightly different flow (checking `reader_file_id > writer_file_id` to avoid deadlock).

### What `roll_to_next_data_file` Does NOT Do

It does not immediately delete the corrupted file. Deletion is deferred to `delete_completed_data_file` via the `data_file_acks` queue, which requires the record-level acknowledgements to drain first. A corrupted file that was partially read (some records read and acked before the corruption was hit) will be deleted only after those earlier records are acked. A file that was corrupted on the very first read (`data_file_start_record_id` is `None`) uses `last_reader_record_id` as a zero-length marker (reader.rs:700-704).

### Key Sub-concern: "Reader Skips Rest of File After First Bad Record"

When `roll_to_next_data_file` is called, the reader **unconditionally abandons the entire remainder of the current data file**. Any valid records that happen to follow the first bad record in the same file are silently lost.

For example: data file has records [A, B, CORRUPT, D, E]. Reader reads A, B, hits CORRUPT, calls `roll_to_next_data_file`. Records D and E are abandoned without being read, acknowledged, or counted. They are charged to `decrement_total_buffer_size` via `delete_completed_data_file`'s `size_delta` calculation (reader.rs:521-535), but they are never delivered.

This is intentional per the code comment at reader.rs:1018-1025:
> "we're not sure the rest of the data file is even valid, so roll to the next file... There's a possibility that the length delimiter we got is valid, and all the data was written for the record, but the data was invalid..."

But it is a significant data-loss surface: a single corrupted record in a 128MB data file can cause the loss of many subsequent valid records in that file. The Antithesis test should measure how many valid records are abandoned per corruption event.

### Error Propagation to Caller

`BufferReader::next` returns `Err(e)` after rolling. The topology adapter (`receiver.rs`) treats this as an unrecoverable error and panics. This means the entire Vector process restarts, not just the buffer reader. On restart, `seek_to_next_record` picks up where the reader file ID was left (now pointing at the next file after the corrupted one), and the pipeline continues. The reachability assertion therefore fires in the read path during the run that encounters the corruption, not on the restart.

## What Goes Wrong if the Property is Not Exercised

If Antithesis injects bit-flips or partial writes but the `corruption_detected_and_recovered` assertion is never triggered, the run provides no evidence that:

1. Fault injection is actually reaching the data files read by the live reader (as opposed to files not yet opened, already deleted, or only read by the mmap fast-path in `seek_to_next_record`).
2. The `is_bad_read` predicate correctly classifies all injected fault signatures (for example, a fault that corrupts the length delimiter in a way that happens to be numerically valid would not cause `PartialWrite` or `Checksum` errors, but might cause `Io` errors, which `is_bad_read` rejects).
3. `roll_to_next_data_file` actually produces a functioning buffer (i.e., the reader successfully opens and reads the next file after rolling).

Without `assert_sometimes`, a test run with zero `is_bad_read` hits is indistinguishable from a run that intentionally never exercises the corruption path.

## Timing / Fault Conditions for Antithesis

- **Bit-flip on payload bytes**: Direct corruption of the CRC-covered region. CRC32C detects this with probability ~(1 - 1/2^32). `Corrupted` error is returned, `is_bad_read()` is true.
- **Bit-flip on checksum field itself**: The stored checksum is wrong, but payload is intact. CRC recomputation produces the correct value; comparison fails. `Corrupted` error.
- **Bit-flip on the rkyv root offset (last 8 bytes of archived record)**: `try_as_archive` reads an incorrect offset, likely accessing out-of-bounds memory → `FailedDeserialization`. `is_bad_read()` is true.
- **Bit-flip on the length delimiter (first 8 bytes of a record)**: `read_length_delimiter` reads a wrong `record_len`. If `record_len` is larger than available data and `is_finalized=true`, returns `PartialWrite`. If `record_len` is valid but points past EOF, returns `PartialWrite`. If `record_len` is small enough that we read the wrong bytes and they fail CRC, returns `Corrupted`.
- **Truncation of the data file mid-record**: With `is_finalized=true`, `try_next_record` detects insufficient bytes and returns `PartialWrite` (reader.rs:263-265, 328-330). `is_bad_read()` is true.
- **File closed/truncated before the reader opens it**: `ensure_ready_for_read` hits an I/O error (`ReaderError::Io`), which `is_bad_read()` does NOT catch. This fault must not be confused with the corruption recovery path.

## SUT-Side Instrumentation Suggestions (ALL MISSING)

**Primary assertion** — in `BufferReader::next`, in the `is_bad_read()` branch just before `roll_to_next_data_file()` is called (reader.rs:1035-1036):

```rust
if e.is_bad_read() {
    antithesis_sdk::assert_sometimes!(
        "corruption_detected_and_recovered",
        "Corruption detected by is_bad_read; rolling to next data file",
        &serde_json::json!({
            "error_code": e.as_error_code(),
            "reader_file_id": self.ledger.get_current_reader_file_id(),
            "writer_file_id": self.ledger.get_current_writer_file_id(),
            "bytes_read_before_corruption": self.bytes_read,
            "records_read_before_corruption": self.data_file_record_count,
        })
    );
    self.roll_to_next_data_file();
}
```

The `as_error_code()` method (reader.rs:141-152) already distinguishes the three bad-read variants (`"checksum_mismatch"`, `"deser_failed"`, `"partial_write"`). Antithesis can break down by error code to confirm all fault types are being detected.

**Secondary assertion** — in `roll_to_next_data_file`, to confirm the rolling logic itself completes (reader.rs:738-740):

```rust
// After increment_unacked_reader_file_id():
antithesis_sdk::assert_sometimes!(
    "reader_rolled_to_next_file_after_corruption",
    "Reader successfully incremented to next data file after corruption",
    &serde_json::json!({
        "new_reader_file_id": self.ledger.get_current_reader_file_id(),
    })
);
```

**Tertiary instrumentation** — count valid records abandoned per roll. In `roll_to_next_data_file`, log the delta between the data file size and `self.bytes_read`. This is already computed implicitly in `delete_completed_data_file` via `size_delta` (reader.rs:521-535). An `assert_sometimes` there with `size_delta > 0` would confirm the "records abandoned after corruption" path is exercised.

## Open Questions

- **Does the `seek_to_next_record` corruption path (reader.rs:912-934) trigger `roll_to_next_data_file`?** The code at reader.rs:912 calls `self.next()` on a bad-read error during initialization but does NOT call `roll_to_next_data_file` directly; it relies on `next()` to do so. If `next()` does roll on bad reads during the seek loop, the same assertion fires. If not (e.g., if initialization-mode `next()` suppresses the roll), the reachability assertion placement inside `next()` would miss corruption during startup. This matters because the most likely time to encounter a corrupted last record is immediately after a crash, during `seek_to_next_record`.

- **Does `roll_to_next_data_file` succeed if the next data file does not yet exist?** If the writer has not yet created file N+1 when the reader rolls to it, `ensure_ready_for_read` will block waiting for the writer (reader.rs:774-775). The buffer would not stall permanently (writer will eventually create the file), but the pipeline is paused. Antithesis should verify the pipeline recovers within a reasonable timeout after corruption-triggered roll.

- **How many valid records are silently abandoned per corruption event?** The decision to roll the entire file on the first bad record is conservative. In a 128MB file with one corruption at byte 1000, nearly the entire file is abandoned. Antithesis should quantify this loss (via the `size_delta` metric) to determine if the policy matches user expectations for a buffer marketed as "at-least-once." (The answer may be that within a process lifetime, corruption = data loss for that file, and users must rely on e2e acks + crash-restart to get the pre-crash unsynced window replayed.)

- **What happens to `pending_acks` for records that were already read and emitted before the corruption was hit?** If records A and B were emitted from the corrupted file before CORRUPT was found, their `BatchNotifier`s are in-flight. When the finalizer drains them, it calls `pending_acks` increment, which eventually causes `record_acks` acknowledgement processing. But the file was rolled: the deletion marker was added with `data_file_record_count` including only A and B, not all records. Does the `data_file_acks` drain correctly when only 2 out of a potential 10 records are marked? Specifically: `OrderedAcknowledgements::add_marker` is called with `Some(2)` records expected (reader.rs:729). If 2 acks come in, the data file is eligible for deletion. This seems correct but the interaction with gap markers in `record_acks` (for the abandoned records D, E) should be verified.
