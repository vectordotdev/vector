# Why `InvalidProtobufPayload` Can Only Come From prost's Recursion Limit

## The Short Version

When Vector writes an event to its disk buffer, prost encodes it to protobuf bytes with **no
depth limit**. When Vector reads it back, prost decodes those same bytes but enforces a **hard
depth limit of 100**. If an event has maps nested 33+ levels deep, the write succeeds and the
read fails. The bytes on disk are perfectly valid — prost just refuses to read back what it
wrote.

This document proves that deeply nested maps are the **only** way to trigger
`InvalidProtobufPayload` on the disk buffer read path, by showing that every other possible
cause is ruled out by the system's integrity guarantees.

## Three Things We Assume

| # | We assume... | Why it's reasonable |
|---|-------------|---------------------|
| A1 | If the CRC32 checksum matches, the bytes weren't corrupted | CRC32 has a ~1-in-4-billion false positive rate per record. Good enough for non-adversarial disk I/O. |
| A2 | rkyv gives back the same bytes it stored | It uses `CopyOptimize + RefAsBox` for byte slices, which is essentially memcpy. Validated by `check_archived_root` on read. |
| A3 | prost's encoder produces well-formed protobuf | Field tags come from compile-time `.proto` definitions, lengths are computed from actual content, and Rust's `String` type guarantees UTF-8. |

---

## Where Does the Error Come From?

There is exactly **one place** in Vector's codebase that produces `InvalidProtobufPayload`.

> **Verify**: search for [`InvalidProtobufPayload` in vectordotdev/vector](https://github.com/vectordotdev/vector/search?q=InvalidProtobufPayload&type=code) — 2 results: the enum definition and this single production site.

[`lib/vector-core/src/event/ser.rs:107-114`](lib/vector-core/src/event/ser.rs):

```rust
proto::EventArray::decode(buffer.clone())          // attempt 1 — line 108
    .map(Into::into)
    .or_else(|_| {
        proto::EventWrapper::decode(buffer)        // attempt 2 — line 111
            .map(|pe| EventArray::from(Event::from(pe)))
            .map_err(|_| DecodeError::InvalidProtobufPayload)  // line 113
    })
```

`InvalidProtobufPayload` is produced when prost fails to decode the payload bytes. (The two
attempts are a backwards-compatibility fallback — `EventArray` is the new format,
`EventWrapper` the legacy format. Both contain the same `Value` nesting chain, so if one fails
on deep nesting, the other does too.)

So the question becomes: **what can make prost's decoder fail?**

---

## What Can Make prost's Decoder Fail?

prost 0.12.6 ([`Cargo.toml:184`](Cargo.toml), [`Cargo.lock:8404`](Cargo.lock)) can fail in
exactly **11 ways**. We know the list is exhaustive because:

[`prost-0.12.6/src/error.rs:17-43`](https://github.com/tokio-rs/prost/blob/v0.12.6/src/error.rs#L17):

```rust
pub struct DecodeError {
    inner: Box<Inner>,     // private field
}

struct Inner {             // private struct
    description: Cow<'static, str>,
    stack: Vec<(&'static str, &'static str)>,
}

impl DecodeError {
    pub fn new(description: impl Into<Cow<'static, str>>) -> DecodeError { ... }
}
```

`DecodeError` has private fields, no `From<X>` impls that construct it, and a single
constructor: `DecodeError::new()`. A grep for `DecodeError::new(` across the entire prost
0.12.6 source returns **20 call sites** that produce **11 distinct error messages**:

Since `DecodeError::new()` is the only constructor (private fields, no `From` impls — see
[`prost/src/error.rs`](https://github.com/tokio-rs/prost/blob/v0.12.6/prost/src/error.rs)), a grep
for `DecodeError::new(` across the entire prost source tree is exhaustive. All 20 call sites
land in two files:
[`prost/src/encoding.rs`](https://github.com/tokio-rs/prost/blob/v0.12.6/prost/src/encoding.rs) and
[`prost/src/lib.rs`](https://github.com/tokio-rs/prost/blob/v0.12.6/prost/src/lib.rs). The recursion
limit constant is at
[`prost/src/lib.rs:31`](https://github.com/tokio-rs/prost/blob/v0.12.6/prost/src/lib.rs#L31):
`const RECURSION_LIMIT: u32 = 100`.

Exhaustive grep of prost 0.12.6 at [`d42c85e`](https://github.com/tokio-rs/prost/commit/d42c85e7):

```
$ grep -rn 'DecodeError::new(' prost/src/

prost/src/encoding.rs:50:        return Err(DecodeError::new("invalid varint"));
prost/src/encoding.rs:155:    Err(DecodeError::new("invalid varint"))
prost/src/encoding.rs:178:                return Err(DecodeError::new("invalid varint"));
prost/src/encoding.rs:185:    Err(DecodeError::new("invalid varint"))
prost/src/encoding.rs:244:            Err(DecodeError::new("recursion limit reached"))
prost/src/encoding.rs:293:            _ => Err(DecodeError::new(format!("invalid wire type value: {}", value
prost/src/encoding.rs:322:        return Err(DecodeError::new(format!("invalid key value: {}", key)));
prost/src/encoding.rs:328:        return Err(DecodeError::new("invalid tag value: 0"));
prost/src/encoding.rs:346:        return Err(DecodeError::new(format!("invalid wire type: {:?} (expected {:?})", actual, expected
prost/src/encoding.rs:369:        return Err(DecodeError::new("buffer underflow"));
prost/src/encoding.rs:378:        return Err(DecodeError::new("delimited length exceeded"));
prost/src/encoding.rs:403:                        return Err(DecodeError::new("unexpected end group tag"));
prost/src/encoding.rs:410:        WireType::EndGroup => return Err(DecodeError::new("unexpected end group tag")),
prost/src/encoding.rs:414:        return Err(DecodeError::new("buffer underflow"));
prost/src/encoding.rs:631:                    return Err(DecodeError::new("buffer underflow"));
prost/src/encoding.rs:844:                Err(_) => Err(DecodeError::new("invalid string value: data is not UTF-8 encoded",
prost/src/encoding.rs:972:            return Err(DecodeError::new("buffer underflow"));
prost/src/encoding.rs:1005:            return Err(DecodeError::new("buffer underflow"));
prost/src/encoding.rs:1172:                    return Err(DecodeError::new("unexpected end group tag"));
prost/src/lib.rs:78:        return Err(DecodeError::new("length delimiter exceeds maximum usize value",
```

20 call sites, 11 distinct messages:

| # | Error message | Call sites | What triggers it |
|---|--------------|------------|------------------|
| **1** | **`recursion limit reached`** | **encoding.rs:244** | **Message nesting depth exceeds 100** |
| 2 | `buffer underflow` | encoding.rs:369, 414, 631, 972, 1005 | A length prefix claims more bytes than remain |
| 3 | `invalid tag value: 0` | encoding.rs:328 | A field tag has field number 0 |
| 4 | `invalid wire type value: {N}` | encoding.rs:293 | A field tag has wire type 6+ |
| 5 | `invalid wire type: {A} (expected {B})` | encoding.rs:346 | A field's wire type doesn't match its schema |
| 6 | `invalid key value: {K}` | encoding.rs:322 | A decoded key exceeds u32 |
| 7 | `invalid varint` | encoding.rs:50, 155, 178, 185 | A varint is >10 bytes or malformed |
| 8 | `invalid string value: not UTF-8` | encoding.rs:844 | A string field contains non-UTF-8 bytes |
| 9 | `delimited length exceeded` | encoding.rs:378 | A sub-message overruns its declared length |
| 10 | `unexpected end group tag` | encoding.rs:403, 410, 1172 | A group end tag doesn't match its start tag |
| 11 | `length delimiter exceeds max usize` | lib.rs:78 | A length varint exceeds usize::MAX |

---

## Why Can We Rule Out 10 of the 11?

Errors 2–11 all require the bytes to be **structurally broken** — bad tags, bad lengths, bad
varints, bad UTF-8, etc. But the bytes reaching the decoder aren't arbitrary. They passed two
integrity checks first.

### The bytes on disk are the same bytes that were encoded

**On write**, prost encodes the event into bytes P, and a CRC32 checksum is computed over them:

[`lib/vector-core/src/event/ser.rs:94-100`](lib/vector-core/src/event/ser.rs) — encode:

```rust
proto::EventArray::from(self)
    .encode(buffer)       // prost Message::encode — no recursion limit
```

[`lib/vector-buffers/src/variants/disk_v2/writer.rs:483-485`](lib/vector-buffers/src/variants/disk_v2/writer.rs) — wrap with checksum:

```rust
Record::with_checksum(id, metadata, &self.encode_buf, &self.checksummer)
```

[`lib/vector-buffers/src/variants/disk_v2/record.rs:157-164`](lib/vector-buffers/src/variants/disk_v2/record.rs) — checksum covers the payload:

```rust
fn generate_checksum(checksummer: &Hasher, id: u64, metadata: u32, payload: &[u8]) -> u32 {
    let mut checksummer = checksummer.clone();
    checksummer.reset();
    checksummer.update(&id.to_be_bytes()[..]);
    checksummer.update(&metadata.to_be_bytes()[..]);
    checksummer.update(payload);    // payload = bytes P at write time
    checksummer.finalize()
}
```

**On read**, the same checksum is recomputed over the bytes pulled from disk, and compared:

[`lib/vector-buffers/src/variants/disk_v2/record.rs:144-154`](lib/vector-buffers/src/variants/disk_v2/record.rs) — verify:

```rust
pub fn verify_checksum(&self, checksummer: &Hasher) -> RecordStatus {
    let calculated = generate_checksum(checksummer, self.id, self.metadata, &self.payload);
    if self.checksum == calculated {
        RecordStatus::Valid { id: self.id }
    } else {
        RecordStatus::Corrupted { calculated, actual: self.checksum }
    }
}
```

[`lib/vector-buffers/src/variants/disk_v2/reader.rs:1153-1154`](lib/vector-buffers/src/variants/disk_v2/reader.rs) — then the payload is decoded:

```rust
T::decode(metadata, record.payload())    // payload = bytes B at read time
```

If the checksums match (A1) and rkyv preserved the bytes faithfully (A2), then the bytes being
decoded (**B**) are identical to the bytes prost originally encoded (**P**).

### prost's encoder can't produce broken bytes

Under A3, the encoded bytes P are well-formed protobuf. Since B == P, the bytes B are also
well-formed. That rules out every error that requires broken structure:

| Error | Needs broken... | But the encoder guarantees... | Possible? |
|-------|----------------|------------------------------|-----------|
| `buffer underflow` | Length prefixes | Lengths are computed from actual sub-message sizes | **No** |
| `invalid tag value: 0` | Field tags | Tags come from `.proto` definitions (field numbers start at 1) | **No** |
| `invalid wire type value` | Wire type bits | Wire types come from `.proto` field types (compile-time) | **No** |
| `invalid wire type (expected)` | Wire type match | Same as above — types are fixed per field | **No** |
| `invalid key value` | Key size | Keys are `(field_number << 3 \| wire_type)`, always fits u32 | **No** |
| `invalid varint` | Varint encoding | Varint encoder always produces well-formed varints | **No** |
| `invalid string: not UTF-8` | UTF-8 validity | Protobuf `string` maps to Rust `String`, which is always UTF-8 | **No** |
| `delimited length exceeded` | Length delimiters | Lengths are computed from actual content | **No** |
| `unexpected end group tag` | Group tags | Vector's `.proto` schema has no group fields | **No** |
| `length delimiter > usize` | Length size | Encoder writes lengths that fit in memory | **No** |

Every one of these is a structural check that the encoder satisfies by construction. **None of
them can fire on bytes that the encoder produced.**

### That leaves only one possibility

Error 1 — `recursion limit reached` — is the **only** prost decode error that doesn't require
broken bytes. It fires on perfectly valid protobuf that happens to be nested more than 100
message levels deep. And crucially, the encoder has **no corresponding depth limit** — it will
happily encode any depth.

> **Verify**: There is no recursion limit, depth counter, or nesting check anywhere in prost's
> encode path. The `RECURSION_LIMIT` constant is only referenced in `DecodeContext`.

---

## Why 33 Nested Maps Hits the Limit

[`lib/vector-core/proto/event.proto`](lib/vector-core/proto/event.proto) defines how events
are structured:

```protobuf
message Log       { Value value = 2; }

message Value     { oneof kind { ValueMap map = 7; } }

message ValueMap  { map<string, Value> fields = 1; }
```

A nested map in a log event goes through this cycle: to get from one level of map nesting to
the next, prost must enter 3 message boundaries — the `ValueMap`, the implicit map entry
message (protobuf encodes `map<K,V>` as repeated `{key, value}` message pairs), and the inner
`Value`. There's also a small overhead to get from the top-level wrapper down to the first
`Value` (2 boundaries for `EventWrapper`, 3 for `EventArray`).

prost starts with a budget of 100 and spends 1 per message boundary. At 32 nested maps the
deepest path uses 98–99 of the budget. At 33, it needs 101–102, which exceeds 100:

| Nesting depth | Message boundaries used | Under limit? |
|---------------|------------------------|--------------|
| 32 | 98–99 | Yes |
| 33 | 101–102 | **No — decode fails** |
| 34 | 104–105 | No |

### Confirmed empirically

- [`depth32_nesting.json`](depth32_nesting.json): 32 nested maps — **passes** decode
- [`depth33_nesting.json`](depth33_nesting.json): 33 nested maps — **fails** with `InvalidProtobufPayload`

---

## Putting It All Together

1. `InvalidProtobufPayload` is produced at exactly one place in Vector, when prost fails to
   decode the payload bytes (ser.rs:113)
2. prost can fail in exactly 11 ways (private struct + single constructor + exhaustive grep)
3. The bytes being decoded are the same bytes prost encoded (CRC32 + rkyv integrity)
4. 10 of those 11 failures require structurally broken bytes, which the encoder can't produce
5. The remaining failure — `recursion limit reached` — requires only deep nesting, which the
   encoder has no limit on

**The encoder accepts any depth. The decoder rejects depth over 100. This asymmetry is the only
way `InvalidProtobufPayload` can occur on the disk buffer read path.** Events with 33+ levels
of nested maps are written successfully and then can never be read back.
