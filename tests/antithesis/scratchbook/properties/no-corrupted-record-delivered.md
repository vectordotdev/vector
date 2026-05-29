# Evidence: no-corrupted-record-delivered

## Property Identification

**Slug:** no-corrupted-record-delivered
**Type:** Safety / AlwaysOrUnreachable
**Assertion macro:** `assert_always_or_unreachable!("no_corrupted_record_delivered", "Record emitted to sink passed CRC32C checksum and rkyv CheckBytes validation", ...)`

This property was identified during review of the read path in `lib/vector-buffers/src/variants/disk_v2/reader.rs`. The buffer's entire durability story rests on the claim that a record failing integrity validation is never delivered to a downstream sink. The property is "always or unreachable" because corruption is expected to be rare (it requires a real fault), but when it does occur the guard must hold unconditionally.

## Code Chain Leading to the Property

### The Validation Gate: `try_next_record` (reader.rs:306-351)

`RecordReader::try_next_record` is the sole entry point for reading a record from a data file. After accumulating `record_len` bytes into `self.aligned_buf`, it calls:

```rust
// reader.rs:338-350
match validate_record_archive(buf, &self.checksummer) {
    RecordStatus::FailedDeserialization(de) => Err(ReaderError::Deserialization { ... }),
    RecordStatus::Corrupted { calculated, actual } => Err(ReaderError::Checksum { ... }),
    RecordStatus::Valid { id, .. } => {
        self.current_record_id = id;
        Ok(Some(ReadToken::new(id, 8 + buf.len())))
    }
}
```

A `ReadToken` is only produced on the `Valid` arm. The two failure arms return `Err`, never a token.

### `validate_record_archive` (record.rs:177-182)

```rust
pub fn validate_record_archive(buf: &[u8], checksummer: &Hasher) -> RecordStatus {
    match try_as_record_archive(buf) {
        Ok(archive) => archive.verify_checksum(checksummer),
        Err(e) => RecordStatus::FailedDeserialization(e),
    }
}
```

Two independent checks are applied in sequence:

1. **rkyv `CheckBytes` / deserialization** via `try_as_record_archive` → `try_as_archive::<Record<'_>>`. This calls the hand-written `CheckBytes` implementation at `record.rs:79-117`.
2. **CRC32C verification** via `ArchivedRecord::verify_checksum` (record.rs:144-155), which recomputes `CRC32C(BE(id) || BE(metadata) || payload)` and compares against the stored `checksum` field.

### The Hand-Written `CheckBytes` (record.rs:75-117)

Because of an upstream rkyv ICE (`rkyv` issue #221), `ArchivedRecord` uses a manual `unsafe` `CheckBytes` implementation instead of a derived one. It validates each field pointer individually:

- `Archived<u32>::check_bytes` for `checksum` (record.rs:90-95)
- `Archived<u64>::check_bytes` for `id` (record.rs:96-101)
- `Archived<u32>::check_bytes` for `schema_metadata` (record.rs:102-107)
- `ArchivedBox<[u8]>::check_bytes` for `payload` (record.rs:108-113)

This is a **manual unsafe validation surface**. Correctness depends entirely on the author having correctly replicated what the derived implementation would have done. A derived-vs-manual divergence (e.g., missing field, wrong offset) would create a gap between what passes `CheckBytes` and what is structurally sound. This cannot be caught by CRC32C alone if the structural corruption happens to produce a valid archive layout with correct-looking raw bytes.

### Corruption Response: `is_bad_read` + `roll_to_next_data_file` (reader.rs:132-139, 1035-1036)

```rust
// reader.rs:132-139
fn is_bad_read(&self) -> bool {
    matches!(
        self,
        ReaderError::Checksum { .. }
            | ReaderError::Deserialization { .. }
            | ReaderError::PartialWrite
    )
}
```

In `BufferReader::next` (reader.rs:1009-1040):

```rust
Err(e) => {
    if e.is_bad_read() {
        self.roll_to_next_data_file();
    }
    return Err(e);
}
```

On a bad read the error is returned to the caller (`BufferReader::next` returns `Err`), NOT a record. The record is never extracted from `aligned_buf` and never handed to `decode_record_payload`. The `Err` propagates to the topology adapter, which treats reader errors as unrecoverable panics (`receiver.rs`). The corrupted bytes never become a sink-delivered event.

### Emission Point: reader.rs:1106-1131

The only code path that produces `Ok(Some(record))` from `next()` passes through `ReadToken` (only issued on `RecordStatus::Valid`), then `read_record` (which calls `archived_root` under a SAFETY comment citing prior validation), then `decode_record_payload`. No path bypasses `validate_record_archive`.

## What Goes Wrong if the Property is Violated

A corrupted record delivered to the sink would contain:

- Wrong event data (payload bytes from a different record, a partial write, or random noise).
- Potentially wrong event count encoded in the record ID gap, causing `total_buffer_size` accounting to drift.
- Silent data corruption: the sink receives and potentially forwards garbled telemetry data, violating downstream correctness.

The Vector comment in `reader.rs:77-78` is explicit: "corruption may have affected other records in a way that is not easily detectable and could lead to records which deserialize/decode but contain invalid data." This is the motivation for rolling the entire file, not just skipping the individual record.

## Timing / Fault Conditions for Antithesis

- **Bit-flip fault injection**: Antithesis can corrupt bytes in a data file while the reader is mid-read or between reads. This exercises both CRC32C detection (payload bytes changed) and rkyv detection (structural fields corrupted).
- **Partial write fault**: A crash during a `TrackingBufWriter::write` call (writer.rs:321-330) can leave a partial record at the end of a data file. On restart, `is_finalized=true` for that file because the reader and writer file IDs differ; `try_next_record` should return `ReaderError::PartialWrite` (reader.rs:263-265, 328-330), which `is_bad_read()` catches.
- **Torn tail after crash**: rkyv's `archived_root` reads the root offset from the last 8 bytes of the buffer. If a crash leaves trailing bytes that happen to encode a plausible offset, the structure might pass `CheckBytes` but point into the wrong region. CRC32C should catch this if the payload is actually wrong bytes, but a CRC collision (probability ~1/2^32 per check) would bypass it.
- **Foreign `.dat` file injection**: Placing a file written by a different Vector version or by an unrelated process into the buffer directory. The record format is host-endian and version-specific; `CheckBytes` should reject misaligned/invalid archives.

## SUT-Side Instrumentation Suggestions (ALL MISSING)

No Antithesis SDK assertions exist anywhere in the codebase (confirmed by repo-wide scan in `existing-assertions.md`). All suggestions below require adding the Antithesis Rust SDK as a new `Cargo.toml` dependency.

**Primary assertion** — in `BufferReader::next` (reader.rs), at the point `Ok(Some(record))` is returned (reader.rs:1131), assert that the record was validated:

```rust
// Immediately after `reader.read_record(token)?` succeeds (reader.rs:1106)
// and before returning Ok(Some(record)):
antithesis_sdk::assert_always_or_unreachable!(
    "no_corrupted_record_delivered",
    "Record emitted to sink passed CRC32C and CheckBytes validation",
    &serde_json::json!({
        "record_id": record_id,
        "record_bytes": record_bytes,
        "data_file_id": self.ledger.get_current_reader_file_id(),
    })
);
```

The assertion is `AlwaysOrUnreachable` because: on a clean run with no faults injected, no corrupted record should ever be delivered (always passes, but the "corrupted record delivered" branch is unreachable). When Antithesis injects faults, if a corrupted record somehow bypasses validation and reaches this point, the assertion fires.

**Secondary instrumentation** — in `validate_record_archive` (record.rs:177), log when each failure mode fires so Antithesis can correlate fault injection with detection:

```rust
// At RecordStatus::Corrupted return:
antithesis_sdk::assert_sometimes!(
    "corruption_detected_by_crc",
    "CRC32C mismatch detected during record validation",
    &serde_json::json!({ "calculated": calculated, "actual": actual })
);
// At RecordStatus::FailedDeserialization return:
antithesis_sdk::assert_sometimes!(
    "corruption_detected_by_checkbytes",
    "rkyv CheckBytes failure detected during record validation",
    ...
);
```

These support the companion property `corruption-is-detected-and-recovered`.

## Residual Risk: CRC Collision

CRC32C is 32 bits. The probability of a random bit-flip producing the correct CRC is approximately 1/2^32 (~2.3 × 10^-10). For any reasonable number of records this is negligible in practice, but it is not zero. A CRC collision would allow a structurally valid but content-incorrect record to pass all checks and be delivered. This is a known, documented limitation of 32-bit checksums and is not a code bug, but it means the property has a residual probabilistic violation rate even with correct implementation.

The hand-written `CheckBytes` (record.rs:75-117) provides a second layer, but it only validates structural soundness (field types and alignment), not semantic correctness of the payload content. A flipped bit in the payload bytes would pass `CheckBytes` and only be caught by CRC32C.

## Open Questions

- **What does the topology adapter do with a `ReaderError` from `next()`?** The SUT analysis notes that `receiver.rs` panics on reader I/O errors. Does it also panic on deserialization/checksum errors, or does it swallow them silently? If it swallows them, the test harness needs a separate counter to confirm that corruption was detected, not ignored. This matters because a swallowed error could look like a successful read to any external observer.

- **Does `seek_to_next_record` (reader.rs:810-947) have the same guard?** During initialization, the reader calls `validate_record_archive` directly at reader.rs:850, not through `try_next_record`. On a `FailedDeserialization` or `Corrupted` result it falls back to the slow path (`break`, reader.rs:896) rather than rolling the file immediately. If the slow path then calls `next()` which itself hits `is_bad_read` + `roll_to_next_data_file`, the protection is preserved; but if the code path does not eventually go through `next()`, the assertion placement above would not catch a corruption during `seek_to_next_record`. This matters for whether the assertion needs to be placed in `seek_to_next_record` as well.

- **Is the `unsafe archived_root` call in `read_record` (reader.rs:375) sound if `aligned_buf` was modified between `try_next_record` and `read_record`?** The code assumes the buffer is not touched between calls. The SAFETY comment cites prior validation. If Antithesis can introduce a data race here (unlikely given single-reader design, but worth confirming with the tokio executor model), the assertion may not catch a post-validation corruption.

- **Does the rkyv `archived_root` torn-tail scenario actually produce a `RecordStatus::Valid` that then fails CRC32C?** Specifically: if crash-left trailing bytes form a plausible rkyv root offset pointing to a valid-looking struct in memory, does `verify_checksum` save us? This determines whether the CRC32C is a sufficient backstop for torn-tail mis-reads or whether a structural check on the offset itself is needed.
