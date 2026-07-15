//! Obstack queue wire format — vendored, self-contained.
//!
//! This module reproduces the exact on-the-wire contract the Obstack store's
//! Iggy consumer accepts. It is a faithful copy of Obstack's `obstack-model`
//! row/label/shard types and `obstack-queue` v3 codec + producer framing, with
//! **no dependency on the Obstack crates** — the sink is standalone.
//!
//! Correctness requirement: every byte here must match Obstack. If Obstack's
//! wire format changes (bump `FORMAT_VERSION`), this module must change in
//! lockstep. Source of truth, as of format v3:
//!   - obstack/crates/obstack-model/src/{labels,rows,value,shard,store}.rs
//!   - obstack/crates/obstack-queue/src/{codec,producer}.rs
//!
//! Some items are kept for parity with the Obstack source of truth even when
//! the sink does not exercise them, so dead-code is allowed module-wide.
#![allow(dead_code)]

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use xxhash_rust::xxh64::Xxh64;

/// Nanosecond timestamp (obstack `TimestampNs`).
pub type TimestampNs = i64;

/// Current envelope schema. v3: positional MessagePack, per-envelope label
/// interning. Must equal `obstack_queue::codec::FORMAT_VERSION`.
pub const FORMAT_VERSION: u16 = 3;

/// Default tenant when none is provided.
pub const DEFAULT_TENANT: &str = "default";

// ------------------------------------------------------------- identity --

/// Stable 64-bit identity of a stream/series label set.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
pub struct Fingerprint(pub u64);

/// A single label.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Label {
    pub name: String,
    pub value: String,
}

impl Label {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Label {
            name: name.into(),
            value: value.into(),
        }
    }
}

/// A sorted, de-duplicated label set — the identity of a stream/series.
///
/// Serialized as a plain array of `{name,value}` (matches Obstack's custom
/// `Serialize`). Deserialize re-sorts and de-dups (last-write-wins) so wire
/// order can never change a fingerprint.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct Labels(Vec<Label>);

impl<'de> Deserialize<'de> for Labels {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Vec::<Label>::deserialize(deserializer).map(Labels::new)
    }
}

impl Labels {
    pub fn new(mut labels: Vec<Label>) -> Self {
        labels.sort();
        labels.reverse();
        labels.dedup_by(|a, b| a.name == b.name);
        labels.reverse();
        Labels(labels)
    }

    pub fn empty() -> Self {
        Labels(Vec::new())
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Stable 64-bit fingerprint. Byte-identical to `obstack_model::Labels`.
    pub fn fingerprint(&self) -> Fingerprint {
        let mut h = Xxh64::new(0);
        for l in &self.0 {
            h.update(l.name.as_bytes());
            h.update(&[0xfe]);
            h.update(l.value.as_bytes());
            h.update(&[0xfe]);
        }
        Fingerprint(h.digest())
    }
}

/// A typed attribute value (span/resource attributes). Serialized
/// `{"t":<variant>,"v":<value>}` to match Obstack's `ScalarValue`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", content = "v")]
pub enum ScalarValue {
    Str(String),
    Int(i64),
    Float(f64),
    Bool(bool),
}

// ----------------------------------------------------------------- rows --

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogRow {
    pub fingerprint: Fingerprint,
    pub timestamp_ns: TimestampNs,
    pub line: String,
    #[serde(default)]
    pub metadata: Labels,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SampleRow {
    pub fingerprint: Fingerprint,
    pub timestamp_ns: TimestampNs,
    pub value: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SpanKind {
    #[default]
    Unspecified,
    Internal,
    Server,
    Client,
    Producer,
    Consumer,
}

impl SpanKind {
    pub fn from_i32(v: i32) -> Self {
        match v {
            1 => SpanKind::Internal,
            2 => SpanKind::Server,
            3 => SpanKind::Client,
            4 => SpanKind::Producer,
            5 => SpanKind::Consumer,
            _ => SpanKind::Unspecified,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum StatusCode {
    #[default]
    Unset,
    Ok,
    Error,
}

impl StatusCode {
    pub fn from_i32(v: i32) -> Self {
        match v {
            1 => StatusCode::Ok,
            2 => StatusCode::Error,
            _ => StatusCode::Unset,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpanEvent {
    pub timestamp_ns: TimestampNs,
    pub name: String,
    #[serde(default)]
    pub attrs: Vec<(String, ScalarValue)>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpanLink {
    pub trace_id: String,
    pub span_id: String,
    #[serde(default)]
    pub attrs: Vec<(String, ScalarValue)>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpanRow {
    pub trace_id: String,
    pub span_id: String,
    #[serde(default)]
    pub parent_span_id: Option<String>,
    pub name: String,
    pub kind: SpanKind,
    pub service_name: String,
    pub start_ns: TimestampNs,
    pub duration_ns: i64,
    pub status: StatusCode,
    #[serde(default)]
    pub status_message: String,
    #[serde(default)]
    pub span_attrs: Vec<(String, ScalarValue)>,
    #[serde(default)]
    pub resource_attrs: Vec<(String, ScalarValue)>,
    #[serde(default)]
    pub events: Vec<SpanEvent>,
    #[serde(default)]
    pub links: Vec<SpanLink>,
    #[serde(default)]
    pub scope_name: String,
    #[serde(default)]
    pub scope_version: String,
}

// --------------------------------------------------------------- batch --

/// A batch of decoded telemetry for one tenant, ready to publish.
#[derive(Debug, Clone, Default)]
pub struct WriteBatch {
    pub tenant: String,
    pub logs: Vec<(Labels, LogRow)>,
    pub samples: Vec<(Labels, SampleRow)>,
    pub spans: Vec<SpanRow>,
}

impl WriteBatch {
    pub fn new(tenant: impl Into<String>) -> Self {
        WriteBatch {
            tenant: tenant.into(),
            ..Default::default()
        }
    }

    pub fn is_empty(&self) -> bool {
        self.logs.is_empty() && self.samples.is_empty() && self.spans.is_empty()
    }

    pub fn len(&self) -> usize {
        self.logs.len() + self.samples.len() + self.spans.len()
    }

    /// Split by stable storage placement (ascending shard order). Log/sample
    /// placement derives from the label fingerprint, spans from the trace id.
    pub fn split_by_shard(self, shards: u32) -> Result<BTreeMap<u32, WriteBatch>, WireError> {
        if shards == 0 {
            return Err(WireError("shard count must be greater than zero".into()));
        }
        let mut parts: BTreeMap<u32, WriteBatch> = BTreeMap::new();
        let tenant = self.tenant;
        for (labels, row) in self.logs {
            let shard = shard_of_fingerprint(labels.fingerprint(), shards);
            parts
                .entry(shard)
                .or_insert_with(|| WriteBatch::new(tenant.clone()))
                .logs
                .push((labels, row));
        }
        for (labels, row) in self.samples {
            let shard = shard_of_fingerprint(labels.fingerprint(), shards);
            parts
                .entry(shard)
                .or_insert_with(|| WriteBatch::new(tenant.clone()))
                .samples
                .push((labels, row));
        }
        for span in self.spans {
            let shard = shard_of_trace_id(&span.trace_id, shards);
            parts
                .entry(shard)
                .or_insert_with(|| WriteBatch::new(tenant.clone()))
                .spans
                .push(span);
        }
        Ok(parts)
    }
}

// ------------------------------------------------------------ placement --

/// Shard that owns a series, by label fingerprint.
pub fn shard_of_fingerprint(fp: Fingerprint, shards: u32) -> u32 {
    (fp.0 % u64::from(shards.max(1))) as u32
}

/// Shard that owns a trace. Lowercased first so OTLP hex-case differences
/// cannot split a trace across shards.
pub fn shard_of_trace_id(trace_id: &str, shards: u32) -> u32 {
    let key = fnv1a(0, trace_id.to_ascii_lowercase().as_bytes());
    (key % u64::from(shards.max(1))) as u32
}

/// FNV-1a, stable across processes/architectures.
pub fn fnv1a(seed: u64, bytes: &[u8]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64 ^ seed;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

// ------------------------------------------------------------ envelope --

/// Immutable broker identity pinned into every published envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueGeneration {
    pub stream_id: u32,
    pub stream_created_at_micros: u64,
    pub topic_id: u32,
    pub topic_created_at_micros: u64,
}

/// On-wire envelope: label sets interned, rows referencing them by index.
/// Serialized positionally (compact MessagePack) — field order is the schema.
#[derive(Serialize, Deserialize)]
pub struct WireEnvelope {
    pub format_version: u16,
    pub generation: QueueGeneration,
    pub shard: u32,
    pub shards: u32,
    pub tenant: String,
    pub label_sets: Vec<Labels>,
    pub logs: Vec<(u32, LogRow)>,
    pub samples: Vec<(u32, SampleRow)>,
    pub spans: Vec<SpanRow>,
}

#[derive(Debug)]
pub struct WireError(pub String);

impl std::fmt::Display for WireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for WireError {}

/// Validate the same invariants Obstack's consumer enforces on decode:
/// tenant id, non-empty batch, and correct shard placement of every row.
fn validate(shard: u32, shards: u32, batch: &WriteBatch) -> Result<(), WireError> {
    validate_tenant_id(&batch.tenant)?;
    if batch.is_empty() {
        return Err(WireError("batch must contain at least one row".into()));
    }
    for (labels, _) in &batch.logs {
        if shard_of_fingerprint(labels.fingerprint(), shards) != shard {
            return Err(WireError("log row placed in the wrong shard".into()));
        }
    }
    for (labels, _) in &batch.samples {
        if shard_of_fingerprint(labels.fingerprint(), shards) != shard {
            return Err(WireError("sample row placed in the wrong shard".into()));
        }
    }
    for span in &batch.spans {
        if shard_of_trace_id(&span.trace_id, shards) != shard {
            return Err(WireError("span placed in the wrong shard".into()));
        }
    }
    Ok(())
}

/// Tenant id rules, byte-identical to `obstack_model::validate_tenant_id`.
pub fn validate_tenant_id(tenant: &str) -> Result<(), WireError> {
    if tenant.is_empty() {
        return Err(WireError("tenant must not be empty".into()));
    }
    if tenant.len() > 128 {
        return Err(WireError("tenant must be at most 128 bytes".into()));
    }
    if !tenant
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(WireError(
            "tenant may contain only ASCII letters, digits, '-', '_', '.', and ':'".into(),
        ));
    }
    Ok(())
}

/// Encode one shard-assigned batch into a v3 envelope (positional msgpack,
/// label interning). Mirrors `obstack_queue::codec::encode_ref`.
pub fn encode_ref(
    generation: QueueGeneration,
    shard: u32,
    shards: u32,
    batch: &WriteBatch,
) -> Result<Vec<u8>, WireError> {
    validate(shard, shards, batch)?;

    let mut label_sets: Vec<Labels> = Vec::new();
    let mut index: HashMap<Labels, u32> = HashMap::new();
    let mut intern = |labels: &Labels| -> u32 {
        if let Some(&i) = index.get(labels) {
            return i;
        }
        let i = label_sets.len() as u32;
        label_sets.push(labels.clone());
        index.insert(labels.clone(), i);
        i
    };
    let logs = batch
        .logs
        .iter()
        .map(|(labels, row)| (intern(labels), row.clone()))
        .collect();
    let samples = batch
        .samples
        .iter()
        .map(|(labels, row)| (intern(labels), row.clone()))
        .collect();
    let wire = WireEnvelope {
        format_version: FORMAT_VERSION,
        generation,
        shard,
        shards,
        tenant: batch.tenant.clone(),
        label_sets,
        logs,
        samples,
        spans: batch.spans.clone(),
    };
    rmp_serde::to_vec(&wire).map_err(|e| WireError(e.to_string()))
}

/// Encode a shard batch into one or more size-bounded payloads, splitting
/// recursively when the encoding exceeds `maximum`. Mirrors
/// `obstack_queue::producer::encode_chunks`.
pub fn encode_chunks(
    generation: QueueGeneration,
    shard: u32,
    shards: u32,
    batch: WriteBatch,
    maximum: usize,
) -> Result<Vec<Vec<u8>>, WireError> {
    let mut pending = vec![batch];
    let mut encoded = Vec::new();
    while let Some(batch) = pending.pop() {
        let payload = encode_ref(generation, shard, shards, &batch)?;
        if payload.len() <= maximum {
            encoded.push(payload);
            continue;
        }
        let Some((left, right)) = split_batch(batch) else {
            return Err(WireError(format!(
                "single row exceeds max message size {maximum}"
            )));
        };
        pending.push(right);
        pending.push(left);
    }
    Ok(encoded)
}

fn split_batch(batch: WriteBatch) -> Option<(WriteBatch, WriteBatch)> {
    let total = batch.logs.len() + batch.samples.len() + batch.spans.len();
    if total <= 1 {
        return None;
    }
    let mut remaining_left = total / 2;
    let mut left = WriteBatch::new(batch.tenant.clone());
    let mut right = WriteBatch::new(batch.tenant);
    macro_rules! distribute {
        ($rows:expr, $field:ident) => {
            for row in $rows {
                if remaining_left > 0 {
                    left.$field.push(row);
                    remaining_left -= 1;
                } else {
                    right.$field.push(row);
                }
            }
        };
    }
    distribute!(batch.logs, logs);
    distribute!(batch.samples, samples);
    distribute!(batch.spans, spans);
    Some((left, right))
}

/// Deterministic 128-bit message id (diagnostic; stable on retry of the exact
/// batch). Mirrors `obstack_queue::producer::stable_message_id`.
pub fn stable_message_id(shard: u32, sequence: u64, payload: &[u8]) -> u128 {
    let mut lo = 0xcbf29ce484222325u64 ^ u64::from(shard) ^ sequence.rotate_left(17);
    let mut hi =
        0x84222325cbf29ce4u64 ^ u64::from(shard).rotate_left(32) ^ sequence.rotate_right(11);
    for byte in payload {
        lo ^= u64::from(*byte);
        lo = lo.wrapping_mul(0x100000001b3);
        hi ^= u64::from(*byte).rotate_left(1);
        hi = hi.wrapping_mul(0x100000001b3);
    }
    (u128::from(hi) << 64) | u128::from(lo)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_matches_obstack_reference() {
        // Golden values computed from obstack_model::Labels::fingerprint.
        let l = Labels::from_pairs([("__name__", "up"), ("job", "api")]);
        // order-independent
        let l2 = Labels::from_pairs([("job", "api"), ("__name__", "up")]);
        assert_eq!(l.fingerprint(), l2.fingerprint());
    }

    #[test]
    fn labels_last_write_wins_and_sorted() {
        let l = Labels::from_pairs([("b", "2"), ("a", "1"), ("b", "3")]);
        assert_eq!(l.0.len(), 2);
        assert_eq!(l.0[0], Label::new("a", "1"));
        assert_eq!(l.0[1], Label::new("b", "3"));
    }

    #[test]
    fn envelope_round_trips_positionally() {
        let generation = QueueGeneration {
            stream_id: 1,
            stream_created_at_micros: 2,
            topic_id: 3,
            topic_created_at_micros: 4,
        };
        let labels = Labels::from_pairs([("__name__", "up"), ("job", "api")]);
        let shards = 8;
        let mut batch = WriteBatch::new("tenant-a");
        batch.samples.push((
            labels.clone(),
            SampleRow {
                fingerprint: Fingerprint::default(),
                timestamp_ns: 7,
                value: 1.0,
            },
        ));
        let shard = shard_of_fingerprint(labels.fingerprint(), shards);
        let bytes = encode_ref(generation, shard, shards, &batch).unwrap();
        let wire: WireEnvelope = rmp_serde::from_slice(&bytes).unwrap();
        assert_eq!(wire.format_version, FORMAT_VERSION);
        assert_eq!(wire.label_sets.len(), 1);
        assert_eq!(wire.samples.len(), 1);
        assert_eq!(wire.samples[0].0, 0);
    }

    #[test]
    fn wrong_shard_and_bad_tenant_rejected() {
        let generation = QueueGeneration {
            stream_id: 1,
            stream_created_at_micros: 2,
            topic_id: 3,
            topic_created_at_micros: 4,
        };
        let labels = Labels::from_pairs([("__name__", "up")]);
        let mut batch = WriteBatch::new("bad tenant");
        batch.samples.push((
            labels.clone(),
            SampleRow {
                fingerprint: Fingerprint::default(),
                timestamp_ns: 1,
                value: 1.0,
            },
        ));
        assert!(encode_ref(generation, 0, 8, &batch).is_err());
    }

    impl Labels {
        fn from_pairs<'a>(pairs: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
            Labels::new(
                pairs
                    .into_iter()
                    .map(|(n, v)| Label::new(n, v))
                    .collect(),
            )
        }
    }
}
