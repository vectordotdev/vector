# Evidence: record-id-monotonicity-holds

## Property Identification

**Slug:** record-id-monotonicity-holds
**Type:** Safety / Unreachable
**Assertion macro:** `assert_unreachable!("record_id_monotonicity_violation", "Record ID monotonicity violation: reader received a record ID <= last seen ID", ...)`

This property was identified by locating the existing `panic!` in `BufferReader::track_read` at reader.rs:481-483. The panic text reads:

```
"record ID monotonicity violation detected; this is a serious bug"
```

This is a guardrail that the authors placed to catch a class of state corruption that they considered serious enough to warrant immediate process termination. For Antithesis purposes, the right framing is `Unreachable`: this code path must never be reached under any fault scenario. If it is reached, Antithesis has found a genuine bug.

Unlike `AlwaysOrUnreachable` (which applies to paths that are legitimately unreachable in the absence of faults), `Unreachable` is appropriate here because record ID monotonicity is an invariant that must hold even in the presence of crash faults. The design specifically addresses how crashes interact with record IDs (`validate_last_write` + ledger fast-forward), and a violation means that interaction is broken.

## Code Chain Leading to the Property

### The Guardrail: `track_read` (reader.rs:448-485)

```rust
fn track_read(&mut self, record_id: u64, record_bytes: u64, event_count: NonZeroU64) {
    self.last_reader_record_id = record_id.wrapping_add(event_count.get() - 1);
    // ...
    if let Err(me) =
        self.record_acks
            .add_marker(record_id, Some(event_count.get()), Some(record_bytes))
    {
        match me {
            MarkerError::MonotonicityViolation => {
                panic!("record ID monotonicity violation detected; this is a serious bug")
            }
        }
    }
}
```

`track_read` is called immediately after `reader.read_record(token)` succeeds (reader.rs:1115), before emitting the record. The panic is in the acknowledgement tracking machinery: `OrderedAcknowledgements::add_marker` rejects a marker whose ID is less than or equal to the last acknowledged ID (i.e., out of order or duplicate). The only error variant is `MarkerError::MonotonicityViolation`, and it is handled by panic.

### What Establishes Monotonicity

Record IDs are produced by the writer. Each record occupies IDs `[next_record_id, next_record_id + event_count - 1]` (writer.rs:755-757: `get_next_record_id` returns `self.next_record_id.wrapping_add(self.unflushed_events)`). After a successful flush, `increment_next_writer_record_id(flushed_events)` advances the ledger's `writer_next_record` atomic (writer.rs:779-781). The writer never reuses an ID for a new record within a single process lifetime.

On restart, `validate_last_write` (writer.rs:838-991) reads the last record from the current data file and compares its `record_next` (last ID + event_count) against `ledger_next`:

- `Ordering::Equal`: ledger matches data; proceed.
- `Ordering::Less` (data ahead of ledger): fast-forward ledger via `increment_next_writer_record_id(ledger_record_delta)` (writer.rs:940-941). This prevents the writer from reusing IDs that were already on disk.
- `Ordering::Greater` (ledger ahead of data): roll to next file; mark "events have likely been lost" (writer.rs:913-920). The writer starts fresh from the ledger value.

The reader establishes its baseline from `ledger.state().get_last_reader_record_id()` (reader.rs:423) and initializes `record_acks` with `from_acked(next_expected_record_id)` (reader.rs:435), where `next_expected_record_id = ledger_last_reader_record_id.wrapping_add(1)`. This tells `OrderedAcknowledgements` to expect IDs starting from one past the last acknowledged point.

### Interaction with `seek_to_next_record` (reader.rs:810-947)

During initialization, `seek_to_next_record` calls `self.next()` in a loop (reader.rs:904-938) until `self.last_reader_record_id >= ledger_last`. Each call to `next()` calls `track_read`, which calls `add_marker`. If during seek a record with a lower ID than expected is encountered (e.g., because `validate_last_write` fast-forwarded the writer ledger to ID 50, but the reader's ledger shows `reader_last_record = 30`, and there is a legitimate record on disk at ID 35), `add_marker` should accept it. But if the ledger fast-forward overshot, or if the reader's `OrderedAcknowledgements` was initialized with the wrong `from_acked` value, a record at a lower-than-expected ID would trigger the panic.

### Interaction with File-ID Rollover

File IDs are `u16`, wrapping at 65,536 (reader.rs:932: `reader_file_id > writer_file_id` comparison, which is raw `u16 >` — not wrapping-aware). In production this is unlikely (131,072 file IDs written before a file is reused). In tests with `MAX_FILE_ID=6`, this is reachable. If the file-ID rollover causes the reader to open a file from a previous generation, that file's record IDs could be far lower than the current `record_acks` watermark, triggering the monotonicity panic.

### Torn-Tail Mis-Recovery (F5 from sut-analysis.md §10)

`validate_last_write` calls `validate_record_archive(data_file_mmap.as_ref(), ...)` which uses `archived_root` to locate the last record. `archived_root` reads the root offset from the **last 8 bytes** of the mmap. A crash that leaves trailing garbage bytes could cause `archived_root` to interpret a plausible but incorrect offset as valid, yielding a `RecordStatus::Valid` record with a wrong `id` field. If that `id` is lower than the correct last-written ID, `validate_last_write` then calls `increment_next_writer_record_id` to fast-forward the writer to `record_next = wrong_id + event_count` — which might be behind the correct current position. The writer then produces a record with a lower ID than already on disk, and the reader's `add_marker` panics.

This torn-tail scenario is the most credible crash-fault path to a monotonicity violation.

## What Goes Wrong if the Property is Violated (the Panic Fires)

The `panic!` terminates the Vector process immediately. In a production deployment this triggers a process restart. On restart, `validate_last_write` and `seek_to_next_record` run again. If the underlying state that caused the violation persists (i.e., the ledger and data files are in a state that will always produce out-of-order IDs), the process enters an infinite restart loop — a permanent pipeline stall. This is operationally equivalent to the writer deadlock bug (#21683) in severity: no data flows, no error is clearly surfaced to the user (just repeated crash logs), and the buffer can only be recovered by manual intervention.

Additionally, the panic discards any in-flight data in `TrackingBufWriter`'s 256KB buffer that has not yet been written to the data file (`BufferWriter::Drop` calls `close()` but not `flush()`), compounding the data loss.

## Timing / Fault Conditions for Antithesis

- **Node kill during `validate_last_write` fast-forward**: If the process is killed after `increment_next_writer_record_id` updates the ledger (writer.rs:940) but before the data file is updated, the ledger is ahead of the data. On the next restart, the `Ordering::Greater` branch fires and rolls to a new file — potentially skipping records. This is the documented "events have likely been lost" path and does not itself cause a monotonicity violation, but it produces a gap in record IDs that the reader must handle via gap markers in `OrderedAcknowledgements`.

- **Node kill during `seek_to_next_record`**: If the process is killed while the reader is replaying records from a file, the next restart starts `seek_to_next_record` again. The ledger `reader_last_record` is updated lazily (only on explicit `ledger.flush()`). If the ledger persisted a partially-advanced `reader_last_record`, the reader might seek past a valid record, leaving its ID below the `record_acks` watermark.

- **File-ID rollover during test** (small `MAX_FILE_ID`): Causes the reader to open a file from a previous file-ID generation. The raw `u16 >` comparison at reader.rs:932 does not handle wrap-around, so this can cause the reader to believe it is ahead of the writer when actually it has wrapped. The reader opens an old file, finds records with much lower IDs than `record_acks` expects, and triggers the panic.

- **External file placement**: Placing a valid-looking `buffer-data-N.dat` from a different run (with lower record IDs) into the buffer directory. The reader would open it as part of the sequence, read records with old IDs, and panic.

## SUT-Side Instrumentation Suggestions (ALL MISSING)

The existing `panic!` is a strong signal but not an Antithesis assertion. Adding the SDK assertion before the panic converts this into a structured finding:

**Primary assertion** — replace the panic in `track_read` (reader.rs:481-483) with an assertion that fires before panicking:

```rust
MarkerError::MonotonicityViolation => {
    antithesis_sdk::assert_unreachable!(
        "record_id_monotonicity_violation",
        "Record ID monotonicity violation: this is a serious bug",
        &serde_json::json!({
            "record_id": record_id,
            "last_reader_record_id": self.last_reader_record_id,
            "data_file_id": self.ledger.get_current_reader_file_id(),
            "writer_file_id": self.ledger.get_current_writer_file_id(),
            "ledger_last_reader_record_id": self.ledger.state().get_last_reader_record_id(),
            "ledger_next_writer_record_id": self.ledger.state().get_next_writer_record_id(),
        })
    );
    panic!("record ID monotonicity violation detected; this is a serious bug");
}
```

The `assert_unreachable!` fires before the panic, giving Antithesis a structured report with state context (IDs, file IDs, ledger values) that can be correlated with the fault that caused the violation.

**Supporting instrumentation** — in `validate_last_write` (writer.rs:838-991), log the `Ordering::Less` fast-forward case with the ledger delta:

```rust
// writer.rs:922-944 (Ordering::Less branch)
let ledger_record_delta = record_next - ledger_next;
// Before increment_next_writer_record_id:
antithesis_sdk::assert_sometimes!(
    "writer_ledger_fast_forwarded",
    "Writer ledger fast-forwarded after crash: data ahead of ledger",
    &serde_json::json!({
        "ledger_next": ledger_next,
        "data_next": record_next,
        "delta": ledger_record_delta,
        "last_record_id_on_disk": last_record_id,
    })
);
```

This makes the crash-recovery fast-forward path reachable in Antithesis testing, and also lets the test author correlate fast-forward events with subsequent monotonicity violations.

## Open Questions

- **Does `OrderedAcknowledgements::add_marker` use wrapping-aware comparison?** If record IDs wrap around `u64::MAX` (theoretically possible after 2^64 writes, not practically reachable but logically relevant at the zero-initialized state), a wrapping `record_id` of 0 would appear to violate monotonicity relative to a high watermark. This matters for how the reader handles the first record on a fresh buffer (where `next_expected_record_id = 0 + 1 = 1` but the first record might have ID 0). Checking the `OrderedAcknowledgements` implementation would clarify whether this is handled correctly.

- **Is the `reader_file_id > writer_file_id` comparison at reader.rs:932 wrapping-safe?** The SUT analysis flags this as a known ordering bug with `MAX_FILE_ID=6`. If file-ID rollover causes this comparison to yield the wrong result, the reader exits the seek loop too early and then reads from the wrong file position — which could produce out-of-order record IDs and trigger the panic. This needs a dedicated test or direct code fix before the Antithesis property can be considered "sound."

- **Can `validate_last_write` ever produce `record_next` lower than `ledger_next` due to torn-tail mis-read?** If yes, and if the `Ordering::Greater` branch simply rolls to the next file (writer.rs:910-920) without updating `next_record_id`, the writer's next record ID might be lower than what the reader's `record_acks` expects. Specifically: `validate_last_write` only updates `self.next_record_id` in the `Ordering::Less` branch (writer.rs:941); in the `Ordering::Greater` branch it just sets `should_skip_to_next_file = true`. Does `self.next_record_id` remain at `ledger.state().get_next_writer_record_id()` (the pre-crash persisted value), which might be higher than the torn-tail record's ID? This determines whether `Ordering::Greater` is safe from a monotonicity perspective.

- **What is the exact behavior of `OrderedAcknowledgements::from_acked` at the `u64` boundary?** If `ledger_last_reader_record_id` is `u64::MAX`, then `wrapping_add(1)` produces `next_expected_record_id = 0`. The reader's `record_acks` would accept only a record with ID >= 0, i.e., any record. This is correct behavior for the wrapping case, but only if `add_marker` also uses wrapping arithmetic. Confirming this would determine whether the panic path is reachable at wrapping boundaries.
