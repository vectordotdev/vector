use bytes::{Buf, BufMut};
use enumflags2::{BitFlags, FromBitsError, bitflags};
use prost::Message;
use snafu::Snafu;
use vector_buffers::encoding::{AsMetadata, Encodable};
use vector_common::internal_event::{self, ComponentEventsDropped, UNINTENTIONAL};
use vrl::value::Value;

use super::{Event, EventArray, EventStatus, proto};

/// Per-level prost recursion frame cost of an [`Value::Object`].
///
/// Decoding an object level walks `Value → ValueMap → map_entry (synthetic) → Value`,
/// adding three message-decode frames before reaching the child Value.
pub(crate) const OBJECT_FRAME_COST: usize = 3;

/// Per-level prost recursion frame cost of an [`Value::Array`].
///
/// Decoding an array level walks `Value → ValueArray → Value`, adding two message-decode
/// frames before reaching the child Value.
pub(crate) const ARRAY_FRAME_COST: usize = 2;

/// Maximum prost recursion frame cost for event data values (`Log.fields`, `Trace.fields`).
///
/// Prost enforces a decode recursion limit of 100 (no limit on encode). Each nesting level
/// consumes 3 frames for [`Value::Object`] or 2 for [`Value::Array`], plus a fixed overhead
/// for the proto wrappers outside the Value tree. The event data path (`EventArray` →
/// `*Array` → Event → fields) has fewer wrappers than the metadata path, allowing a higher
/// frame budget.
///
/// Object-only depth 33 (cost 99) roundtrips; depth 34 (cost 102) fails decode. Array-only
/// nesting is correspondingly looser: depth 49 (cost 98) is the highest that fits.
pub const MAX_VALUE_NESTING_FRAMES: usize = 99;

/// Maximum prost recursion frame cost for event metadata values (via `metadata_full`).
///
/// The metadata path (`EventArray` → `*Array` → Event → `Metadata` → Value) has one more
/// proto wrapper message than the event data path due to the `Metadata` message, reducing
/// the safe budget by 3 frames.
///
/// Object-only depth 32 (cost 96) roundtrips; depth 33 (cost 99) fails decode.
pub const MAX_METADATA_VALUE_NESTING_FRAMES: usize = 96;

/// Walks a [`Value`] tree accumulating prost recursion frame cost, returning
/// `Err(over_budget_cost)` as soon as any branch exceeds `budget`.
///
/// Object levels weigh [`OBJECT_FRAME_COST`] frames each, array levels weigh
/// [`ARRAY_FRAME_COST`]; scalar leaves are free. Performs an early-exit traversal so
/// well-formed events incur a single descent of the deepest branch only.
///
/// # Errors
///
/// Returns `Err(actual_cost)` if any branch's cumulative frame cost exceeds `budget`.
pub(crate) fn check_value_nesting_cost(
    value: &Value,
    accumulated: usize,
    budget: usize,
) -> Result<(), usize> {
    let level_cost = match value {
        Value::Object(_) => OBJECT_FRAME_COST,
        Value::Array(_) => ARRAY_FRAME_COST,
        _ => 0,
    };
    let next = accumulated + level_cost;
    if next > budget {
        return Err(next);
    }
    match value {
        Value::Object(map) => {
            for v in map.values() {
                check_value_nesting_cost(v, next, budget)?;
            }
        }
        Value::Array(arr) => {
            for v in arr {
                check_value_nesting_cost(v, next, budget)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Checks whether an event's nesting frame cost exceeds the safe limits for protobuf encoding.
///
/// Returns `Some((cost, budget))` identifying the path that violated its budget, or `None`
/// if the event is within bounds.
///
/// Event data values (Log.fields, Trace.fields) are checked against
/// [`MAX_VALUE_NESTING_FRAMES`], while metadata values are checked against the stricter
/// [`MAX_METADATA_VALUE_NESTING_FRAMES`] because the `Metadata` proto message adds an
/// extra wrapper layer.
///
/// For metrics, only metadata is checked since metric values have a fixed structure.
pub fn event_exceeds_max_nesting_cost(event: &Event) -> Option<(usize, usize)> {
    match event {
        Event::Log(log) => check_value_nesting_cost(log.value(), 0, MAX_VALUE_NESTING_FRAMES)
            .map_err(|cost| (cost, MAX_VALUE_NESTING_FRAMES))
            .and_then(|()| {
                check_value_nesting_cost(
                    log.metadata().value(),
                    0,
                    MAX_METADATA_VALUE_NESTING_FRAMES,
                )
                .map_err(|cost| (cost, MAX_METADATA_VALUE_NESTING_FRAMES))
            })
            .err(),
        Event::Trace(trace) => check_value_nesting_cost(trace.value(), 0, MAX_VALUE_NESTING_FRAMES)
            .map_err(|cost| (cost, MAX_VALUE_NESTING_FRAMES))
            .and_then(|()| {
                check_value_nesting_cost(
                    trace.metadata().value(),
                    0,
                    MAX_METADATA_VALUE_NESTING_FRAMES,
                )
                .map_err(|cost| (cost, MAX_METADATA_VALUE_NESTING_FRAMES))
            })
            .err(),
        Event::Metric(metric) => check_value_nesting_cost(
            metric.metadata().value(),
            0,
            MAX_METADATA_VALUE_NESTING_FRAMES,
        )
        .map_err(|cost| (cost, MAX_METADATA_VALUE_NESTING_FRAMES))
        .err(),
    }
}

/// Checks all events in an `EventArray` for nesting cost violations.
///
/// Event data is checked against [`MAX_VALUE_NESTING_FRAMES`] and metadata against
/// [`MAX_METADATA_VALUE_NESTING_FRAMES`]. For metrics, only metadata is checked since
/// metric values have a fixed structure.
fn check_event_array_nesting_cost(events: &EventArray) -> Result<(), EncodeError> {
    let check = |value: &Value, budget: usize| {
        check_value_nesting_cost(value, 0, budget)
            .map_err(|cost| EncodeError::NestingTooDeep { cost, budget })
    };
    match events {
        EventArray::Logs(logs) => {
            for log in logs {
                check(log.value(), MAX_VALUE_NESTING_FRAMES)?;
                check(log.metadata().value(), MAX_METADATA_VALUE_NESTING_FRAMES)?;
            }
        }
        EventArray::Traces(traces) => {
            for trace in traces {
                check(trace.value(), MAX_VALUE_NESTING_FRAMES)?;
                check(trace.metadata().value(), MAX_METADATA_VALUE_NESTING_FRAMES)?;
            }
        }
        EventArray::Metrics(metrics) => {
            for metric in metrics {
                check(metric.metadata().value(), MAX_METADATA_VALUE_NESTING_FRAMES)?;
            }
        }
    }
    Ok(())
}

#[derive(Debug, Snafu)]
pub enum EncodeError {
    #[snafu(display("the provided buffer was too small to fully encode this item"))]
    BufferTooSmall,
    #[snafu(display("event nesting cost {cost} exceeds protobuf budget of {budget}"))]
    NestingTooDeep { cost: usize, budget: usize },
}

#[derive(Debug, Snafu)]
pub enum DecodeError {
    #[snafu(display(
        "the provided buffer could not be decoded as a valid Protocol Buffers payload"
    ))]
    InvalidProtobufPayload,
    #[snafu(display("unsupported encoding metadata for this context"))]
    UnsupportedEncodingMetadata,
}
/// Flags for describing the encoding scheme used by our primary event types that flow through buffers.
///
/// # Stability
///
/// This enumeration should never have any flags removed, only added.  This ensures that previously
/// used flags cannot have their meaning changed/repurposed after-the-fact.
#[bitflags]
#[repr(u32)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum EventEncodableMetadataFlags {
    /// Chained encoding scheme that first tries to decode as `EventArray` and then as `Event`, as a
    /// way to support gracefully migrating existing v1-based disk buffers to the new
    /// `EventArray`-based architecture.
    ///
    /// All encoding uses the `EventArray` variant, however.
    DiskBufferV1CompatibilityMode = 0b1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EventEncodableMetadata(BitFlags<EventEncodableMetadataFlags>);

impl EventEncodableMetadata {
    fn contains(self, flag: EventEncodableMetadataFlags) -> bool {
        self.0.contains(flag)
    }
}

impl From<EventEncodableMetadataFlags> for EventEncodableMetadata {
    fn from(flag: EventEncodableMetadataFlags) -> Self {
        Self(BitFlags::from(flag))
    }
}

impl From<BitFlags<EventEncodableMetadataFlags>> for EventEncodableMetadata {
    fn from(flags: BitFlags<EventEncodableMetadataFlags>) -> Self {
        Self(flags)
    }
}

impl TryFrom<u32> for EventEncodableMetadata {
    type Error = FromBitsError<EventEncodableMetadataFlags>;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        BitFlags::try_from(value).map(Self)
    }
}

impl AsMetadata for EventEncodableMetadata {
    fn into_u32(self) -> u32 {
        self.0.bits()
    }

    fn from_u32(value: u32) -> Option<Self> {
        EventEncodableMetadata::try_from(value).ok()
    }
}

impl Encodable for EventArray {
    type Metadata = EventEncodableMetadata;
    type EncodeError = EncodeError;
    type DecodeError = DecodeError;

    fn get_metadata() -> Self::Metadata {
        EventEncodableMetadataFlags::DiskBufferV1CompatibilityMode.into()
    }

    fn can_decode(metadata: Self::Metadata) -> bool {
        metadata.contains(EventEncodableMetadataFlags::DiskBufferV1CompatibilityMode)
    }

    fn encode<B>(self, buffer: &mut B) -> Result<(), Self::EncodeError>
    where
        B: BufMut,
    {
        // Defense-in-depth: well-behaved callers run `pre_encode_drop_unencodable`
        // first, but if any deeply-nested event reaches encode it would corrupt the
        // disk buffer by encoding successfully and then failing prost's recursion
        // limit on decode.
        check_event_array_nesting_cost(&self)?;

        proto::EventArray::from(self)
            .encode(buffer)
            .map_err(|_| EncodeError::BufferTooSmall)
    }

    fn pre_encode_drop_unencodable(&mut self) -> usize {
        let exceeds =
            |value: &Value, budget: usize| check_value_nesting_cost(value, 0, budget).is_err();
        let mut dropped = 0;
        match self {
            EventArray::Logs(logs) => logs.retain(|log| {
                let too_deep = exceeds(log.value(), MAX_VALUE_NESTING_FRAMES)
                    || exceeds(log.metadata().value(), MAX_METADATA_VALUE_NESTING_FRAMES);
                if too_deep {
                    log.metadata().update_status(EventStatus::Rejected);
                    dropped += 1;
                }
                !too_deep
            }),
            EventArray::Traces(traces) => traces.retain(|trace| {
                let too_deep = exceeds(trace.value(), MAX_VALUE_NESTING_FRAMES)
                    || exceeds(trace.metadata().value(), MAX_METADATA_VALUE_NESTING_FRAMES);
                if too_deep {
                    trace.metadata().update_status(EventStatus::Rejected);
                    dropped += 1;
                }
                !too_deep
            }),
            EventArray::Metrics(metrics) => metrics.retain(|metric| {
                let too_deep =
                    exceeds(metric.metadata().value(), MAX_METADATA_VALUE_NESTING_FRAMES);
                if too_deep {
                    metric.metadata().update_status(EventStatus::Rejected);
                    dropped += 1;
                }
                !too_deep
            }),
        }
        if dropped > 0 {
            internal_event::emit(ComponentEventsDropped::<UNINTENTIONAL> {
                count: dropped,
                reason: "Event nesting cost exceeds maximum for protobuf encoding.",
            });
        }
        dropped
    }

    fn decode<B>(metadata: Self::Metadata, buffer: B) -> Result<Self, Self::DecodeError>
    where
        B: Buf + Clone,
    {
        if metadata.contains(EventEncodableMetadataFlags::DiskBufferV1CompatibilityMode) {
            proto::EventArray::decode(buffer.clone())
                .map(Into::into)
                .or_else(|_| {
                    proto::EventWrapper::decode(buffer)
                        .map(|pe| EventArray::from(Event::from(pe)))
                        .map_err(|_| DecodeError::InvalidProtobufPayload)
                })
        } else {
            Err(DecodeError::UnsupportedEncodingMetadata)
        }
    }
}
