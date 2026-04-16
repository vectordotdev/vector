use bytes::{Buf, BufMut};
use enumflags2::{BitFlags, FromBitsError, bitflags};
use prost::Message;
use snafu::Snafu;
use vector_buffers::encoding::{AsMetadata, Encodable};
use vrl::value::Value;

use super::{Event, EventArray, proto};

/// Maximum nesting depth for event data values (Log.fields, Trace.fields).
///
/// Prost enforces a decode recursion limit of 100 (no limit on encode). Each Value nesting
/// level consumes 3 prost recursion entries (Value + ValueMap + map_entry). The event data
/// path (`EventArray → *Array → Event → fields map_entry → Value`) has 3 proto wrapper
/// messages before the Value tree, leaving room for 33 depth levels: 3 + (33 × 3) + 1 = 100.
///
/// Verified by `per_path_boundaries`: depth 33 succeeds, depth 34 fails prost decode.
pub const MAX_NESTING_DEPTH: usize = 33;

/// Maximum nesting depth for event metadata values (via `metadata_full`).
///
/// The metadata path (`EventArray → *Array → Event → Metadata → Value`) has 4 proto wrapper
/// messages — one more than the event data path due to the `Metadata` message. This leaves
/// room for 32 depth levels: 4 + (32 × 3) + 1 = 100, exactly at prost's recursion limit.
///
/// Verified by `per_path_boundaries`: depth 32 succeeds, depth 33 fails prost decode.
pub const MAX_METADATA_NESTING_DEPTH: usize = 32;

/// Check the nesting depth of a `Value`, returning `Err(actual_depth)` if it exceeds `max_depth`.
///
/// This performs an early-exit traversal: it returns as soon as any branch exceeds the limit,
/// avoiding unnecessary work on well-formed events.
///
/// # Errors
///
/// Returns `Err(actual_depth)` if any branch of the value tree exceeds `max_depth`.
pub(crate) fn check_value_depth(
    value: &Value,
    current_depth: usize,
    max_depth: usize,
) -> Result<(), usize> {
    if current_depth > max_depth {
        return Err(current_depth);
    }
    match value {
        Value::Object(map) => {
            for v in map.values() {
                check_value_depth(v, current_depth + 1, max_depth)?;
            }
        }
        Value::Array(arr) => {
            for v in arr {
                check_value_depth(v, current_depth + 1, max_depth)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Checks whether an event's nesting depth exceeds the safe limits for protobuf encoding.
///
/// Returns `Some(depth)` with the violating depth if the event exceeds the limit,
/// or `None` if the event is within bounds.
///
/// Event data values (Log.fields, Trace.fields) are checked against [`MAX_NESTING_DEPTH`],
/// while metadata values are checked against the stricter [`MAX_METADATA_NESTING_DEPTH`]
/// because the `Metadata` proto message adds an extra wrapper layer.
///
/// For metrics, only metadata is checked since metric values have a fixed structure.
pub fn event_exceeds_max_nesting_depth(event: &Event) -> Option<usize> {
    match event {
        Event::Log(log) => check_value_depth(log.value(), 0, MAX_NESTING_DEPTH)
            .and_then(|()| check_value_depth(log.metadata().value(), 0, MAX_METADATA_NESTING_DEPTH))
            .err(),
        Event::Trace(trace) => check_value_depth(trace.value(), 0, MAX_NESTING_DEPTH)
            .and_then(|()| {
                check_value_depth(trace.metadata().value(), 0, MAX_METADATA_NESTING_DEPTH)
            })
            .err(),
        Event::Metric(metric) => {
            check_value_depth(metric.metadata().value(), 0, MAX_METADATA_NESTING_DEPTH).err()
        }
    }
}

/// Checks all events in an `EventArray` for nesting depth violations.
///
/// Event data is checked against [`MAX_NESTING_DEPTH`] and metadata against
/// [`MAX_METADATA_NESTING_DEPTH`]. For metrics, only metadata is checked since
/// metric values have a fixed structure.
fn check_event_array_nesting_depth(events: &EventArray) -> Result<(), EncodeError> {
    let check = |value: &Value, max_depth: usize| {
        check_value_depth(value, 0, max_depth)
            .map_err(|depth| EncodeError::NestingTooDeep { depth, max_depth })
    };
    match events {
        EventArray::Logs(logs) => {
            for log in logs {
                check(log.value(), MAX_NESTING_DEPTH)?;
                check(log.metadata().value(), MAX_METADATA_NESTING_DEPTH)?;
            }
        }
        EventArray::Traces(traces) => {
            for trace in traces {
                check(trace.value(), MAX_NESTING_DEPTH)?;
                check(trace.metadata().value(), MAX_METADATA_NESTING_DEPTH)?;
            }
        }
        EventArray::Metrics(metrics) => {
            for metric in metrics {
                check(metric.metadata().value(), MAX_METADATA_NESTING_DEPTH)?;
            }
        }
    }
    Ok(())
}

#[derive(Debug, Snafu)]
pub enum EncodeError {
    #[snafu(display("the provided buffer was too small to fully encode this item"))]
    BufferTooSmall,
    #[snafu(display("event nesting depth {depth} exceeds maximum of {max_depth}"))]
    NestingTooDeep { depth: usize, max_depth: usize },
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
        // Check nesting depth before encoding. Deeply nested events encode
        // successfully but fail to decode due to prost's recursion limit,
        // which would corrupt the disk buffer.
        check_event_array_nesting_depth(&self)?;

        proto::EventArray::from(self)
            .encode(buffer)
            .map_err(|_| EncodeError::BufferTooSmall)
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
