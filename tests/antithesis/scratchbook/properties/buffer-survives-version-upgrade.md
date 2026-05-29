---
slug: buffer-survives-version-upgrade
type: Safety + Liveness / Sometimes(upgrade_readback_ok) + AlwaysOrUnreachable(compat_flag_rejects_correctly)
sut_path: lib/vector-buffers/src/variants/disk_v2/
commit: 049eec79b737450c4669b7f8aa1dd814551ec466
updated: 2026-06-02
---

# Property: buffer-survives-version-upgrade

## Catalog Entry

**Type:** Safety + Liveness / `Sometimes(upgrade_readback_ok)` (facet A) +
`AlwaysOrUnreachable(compat_flag_handled_correctly)` (facet B)

**Property:** Two distinct but related invariants:

**(A) rkyv layout version safety.** Buffer files written by Vector version N
are read back correctly by the same version N (stability across restart). If
the rkyv-archived layout changes between versions (field addition, removal,
reordering — all banned by the struct-level warning in `record.rs`), the read
attempt at version N+1 produces a clean, detected error (`InvalidStructure`
or `InvalidData` in `DeserializeError`) and is never silently interpreted as a
valid record with garbage content.

**(B) `DiskBufferV1CompatibilityMode` flag correctness.** The
`DiskBufferV1CompatibilityMode` flag (set on every written record by
`get_metadata()`) must be present for `can_decode()` to return `true`. A
record written WITHOUT this flag would be rejected at decode time — a
forward-compat foot-gun if a future encoding scheme stops setting the flag.
Conversely, records written WITH the flag must be accepted. The flag logic
must never silently accept garbage or silently reject valid records.

**Invariant (A):** `Sometimes(buffer_readback_ok)` — after writing N events,
restarting Vector with the same binary (same rkyv layout), and draining the
buffer, all N events are delivered. This establishes a same-version readback
baseline. Under a simulated format change (custom fault: modify the binary or
the data files), any read of the modified data produces a `DeserializeError`,
never a `RecordStatus::Valid{id}` with wrong content.

**Invariant (B):** `AlwaysOrUnreachable` at the `can_decode()` call site: a
record accepted by `get_metadata()` is always accepted by `can_decode()` when
read back. `AlwaysOrUnreachable` because incompatible records are a rare/fault
path — any execution must satisfy the invariant, but never-executed is
acceptable.

**Antithesis Angle (A):** Write events with Vector binary version N; simulate
a format change by modifying `buffer-data-*.dat` files on disk or swapping the
binary (a custom Antithesis fault); restart; assert all reads produce
`DeserializeError` (clean detection), never garbage. Validates that
`try_as_archive` / `check_archived_root` / manual `CheckBytes` properly rejects
changed layouts.

**Antithesis Angle (B):** Confirm by inspection that every record written with
the current binary has `DiskBufferV1CompatibilityMode` set (since
`get_metadata()` always sets it). Inject a synthetic record without the flag
into a data file; assert `can_decode()` returns `false` and the record is
skipped/rejected cleanly. Assert the inverse is never triggered for valid
records.

**Why It Matters:** The `Record` struct carries an explicit warning:

> Do not add/remove/change/reorder fields. Doing so will change the serialized
> representation. This will break things.

This warning is the only guard against a breaking layout change. There is no
version field in the on-disk record that would allow runtime detection of a
layout mismatch. A layout change would silently produce garbage via rkyv's
zero-copy `archived_root` (reading wrong bytes as field values), potentially
passing the manual `CheckBytes` (which validates field types, not semantic
values) and the CRC32C check (computed over the new layout's bytes, now
matching the new checksum). The only signal would be an implausible record ID
or payload — not a detected error.

The `DiskBufferV1CompatibilityMode` flag is a forward-compat foot-gun: if a
future encoding drops this flag, every existing buffer file becomes
unreadable. This property ensures the flag logic is tested end-to-end.

---

## Code Verification

### `Record` struct immutability warning (record.rs:36-45)

```rust
// lib/vector-buffers/src/variants/disk_v2/record.rs:36-45
/// # Warning
///
/// - Do not add fields to this struct.
/// - Do not remove fields from this struct.
/// - Do not change the type of fields in this struct.
/// - Do not change the order of fields this struct.
///
/// Doing so will change the serialized representation.  This will break things.
#[derive(Archive, Serialize, Debug)]
```

This warning is the only guard. There is no `#[rkyv(version)]` attribute, no
format-version field, and no migration path.

### rkyv host-endian, version-sensitive layout

```rust
// record.rs:46-73 (ArchivedRecord layout)
pub struct Record<'a> {
    pub(super) checksum: u32,  // [u32 native-endian]
    id: u64,                   // [u64 native-endian]
    pub(super) metadata: u32,  // [u32 native-endian]
    #[with(CopyOptimize, RefAsBox)]
    payload: &'a [u8],         // [ArchivedBox<[u8]>: ptr+len, native-endian]
}
```

Fields are native-endian (little-endian on x86-64). The `CopyOptimize,
RefAsBox` combination serializes the payload as an `ArchivedBox<[u8]>`, which
stores a relative pointer and length in the rkyv buffer. Any change to the
struct layout — including adding a field, changing `RefAsBox` to another
`With` adapter, or reordering fields — changes the byte positions of all
subsequent fields, making existing files unreadable without a detected error.

### `try_as_archive` — the deserialization gate (ser.rs:88-95)

```rust
// lib/vector-buffers/src/variants/disk_v2/ser.rs:88-95
pub fn try_as_archive<'a, T>(buf: &'a [u8]) -> Result<&'a T::Archived, DeserializeError>
where
    T: Archive,
    T::Archived: for<'b> CheckBytes<DefaultValidator<'b>>,
{
    debug_assert!(!buf.is_empty());
    check_archived_root::<T>(buf).map_err(Into::into)
}
```

`check_archived_root` reads the root offset from the last 8 bytes of the
buffer, then validates the archived value using `CheckBytes`. If the buffer
layout changed (new field at offset 0 shifts everything), `check_archived_root`
may:

- Return `CheckArchiveError::CheckBytesError` → `DeserializeError::InvalidData`
  (detected, clean error).
- OR interpret the bytes as a valid archived value at a wrong offset — this is
  the "silent garbage" path if the raw bytes happen to pass `CheckBytes`
  validation.

The CRC32C check in `verify_checksum` (record.rs:144-155) would catch
most garbage payloads IF the checksum field itself was not shifted into a
position that holds a value that happens to match the CRC of the new layout's
payload. This is unlikely but not impossible for small payloads.

### Manual `CheckBytes` (record.rs:79-117)

```rust
// record.rs:79-117
impl<'a, C: ?Sized> CheckBytes<C> for ArchivedRecord<'a> { ... }
```

This is a manual `unsafe` implementation (due to an upstream rkyv ICE, see
the comment). It validates that `checksum`, `id`, `metadata` are valid `u32`
and `u64` values, and that `ArchivedBox<[u8]>` is valid. It does NOT validate
semantic constraints (e.g., that `id` is monotonic, or that `checksum`
actually matches the payload). A layout-changed record may pass this check if
the bytes at the expected offsets are valid primitive values.

### `DiskBufferV1CompatibilityMode` flag (vector-core/event/ser.rs:86-91)

```rust
// lib/vector-core/src/event/ser.rs:86-91
fn get_metadata() -> Self::Metadata {
    EventEncodableMetadataFlags::DiskBufferV1CompatibilityMode.into()
}

fn can_decode(metadata: Self::Metadata) -> bool {
    metadata.contains(EventEncodableMetadataFlags::DiskBufferV1CompatibilityMode)
}
```

`get_metadata()` always returns the `DiskBufferV1CompatibilityMode` flag.
`can_decode()` requires this flag to be present. Every record written at this
commit will have the flag set; `can_decode()` will return `true` for them.

The foot-gun: if a future version of Vector introduces a new
`EventEncodableMetadataFlags` variant and changes `get_metadata()` to return
only the new flag (not `DiskBufferV1CompatibilityMode`), then all existing
buffer files (which have only `DiskBufferV1CompatibilityMode` set) would fail
`can_decode()` and be rejected. This is a format-incompatibility scenario,
not a bug in the current code — but it is uncovered by any test.

### `can_decode` call site in reader (reader.rs — the decode gate)

The `can_decode` result gates whether the payload bytes are passed to
`Encodable::decode`. A `false` result leads to a `RecordStatus::Valid` with a
metadata rejection path, not a clean `FailedDeserialization` — the exact error
path needs confirmation.

---

## Fault Conditions

| Fault | Effect |
|---|---|
| Restart with same binary | Normal readback — baseline; must always succeed. |
| Binary swap (version upgrade) with rkyv layout change | Data files become unreadable; must be a clean `DeserializeError`, not garbage. |
| Synthetic record with wrong metadata flag injected into `.dat` file | `can_decode()` returns `false`; record rejected; reader rolls forward. |
| Synthetic record with correct metadata flag but wrong rkyv layout | `CheckBytes` may or may not detect it; CRC32C is the backstop. |
| `buffer.db` ledger migrated between versions (no format versioning) | Ledger is a raw memory-mapped struct (`LedgerState`); field-order changes break it silently. |

---

## SUT-Side Instrumentation (not yet committed — the SDK is wired and the three #21683 underflow asserts are present; these are additional)

The Antithesis SDK is a committed dependency under the `antithesis` feature, and
three underflow `assert_always_greater_than_or_equal_to!` detectors exist
(ledger.rs:271/313, reader.rs:529; see existing-assertions.md). None covers
rkyv layout safety or the compatibility-mode flag, so the readback assertions
below remain genuine still-to-add suggestions.

### Assertion 1 — Sometimes: same-version readback succeeds (baseline)

```rust
// reader.rs, after a successful record decode and CRC validation
antithesis_sdk::assert_sometimes!(
    true,  // reachability: this path executes
    "buffer-readback: record successfully decoded after restart",
    &serde_json::json!({
        "record_id": record.id,
        "metadata": record.metadata,
    })
);
```

### Assertion 2 — AlwaysOrUnreachable: incompatible metadata is cleanly rejected

```rust
// reader.rs, at the can_decode() check
let can_decode = T::can_decode(metadata);
antithesis_sdk::assert_always_or_unreachable!(
    can_decode || /* metadata flag is NOT the expected flag */ true,
    "buffer-readback: can_decode returns true for any record written by this binary",
    &serde_json::json!({
        "metadata_value": metadata.into_u32(),
        "can_decode": can_decode,
    })
);
```

### Assertion 3 — Always: DeserializeError path detected, not garbage

At the `RecordStatus::FailedDeserialization` arm, assert the path is taken
(not `RecordStatus::Valid` with a wrong ID) when a known-bad record is
injected:

```rust
// record.rs, inside verify_record_archive when returning FailedDeserialization
antithesis_sdk::assert_reachable!(
    "buffer-readback: FailedDeserialization reached for injected bad record",
    &serde_json::json!({ "error": err.to_string() })
);
```

---

## Why Existing Tests Cannot Catch This

- The model-based proptest uses an in-memory filesystem and does not exercise
  the rkyv deserialization path with externally-modified buffers.
- No test writes events, swaps or modifies the binary/data files, and then
  restarts — this is an upgrade/migration scenario outside the unit-test scope.
- The `DiskBufferV1CompatibilityMode` flag is set on every write in the
  current binary; no test ever synthesizes a record without it.
- The manual `CheckBytes` implementation is only validated for correctness
  under the current struct layout, not under a changed layout.

---

## Requires a Custom Fault

Testing facet (A) requires one of:

1. A custom Antithesis fault that modifies `buffer-data-*.dat` bytes while
   Vector is stopped (between shutdown and restart).
2. Two Vector binaries in the harness image — binary N writes, binary N+1
   (with a simulated layout change or flag change) reads.

Neither is standard in the default Antithesis fault library. The harness must
be explicitly designed for this scenario.

---

## Open Questions

- Is there any runtime mechanism to detect a rkyv layout mismatch short of
  `CheckBytes` and CRC32C? If both checks pass for a layout-changed record
  (possible for small payloads), the garbage is delivered as a valid event.
  This is the "silent corruption" path that this property is designed to expose.

- Does `check_archived_root` return `CheckArchiveError::ContextError` (layout
  mismatch — the bytes don't form a valid archive) or fall through to
  `CheckBytesError` for the layout-changed case? The distinction determines
  whether the error is `InvalidStructure` or `InvalidData`, which affects the
  reader's recovery behavior.

- Is the `LedgerState` mmap'd struct (`buffer.db`) versioned? If a Vector
  upgrade changes `LedgerState` (e.g., adds a field), the mmap'd file is read
  with wrong offsets — silently. This is a separate version-upgrade risk not
  covered by the record-level `CheckBytes`.

- What is the expected behavior when `can_decode()` returns `false`? Does the
  reader treat the record as unreadable (skip + roll) or as a fatal error
  (stop)? The current code path at `reader.rs` needs verification to confirm
  `false` → `RecordStatus` rejection → `roll_to_next_data_file`, not panic.

- Should this property be split into two separate slugs: one for rkyv layout
  version safety and one for the `DiskBufferV1CompatibilityMode` flag? The two
  facets have distinct fault mechanisms (binary swap vs. flag injection) but
  share the same "upgrade readback" narrative.
