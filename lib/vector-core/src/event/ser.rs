use bytes::{Buf, BufMut};
use enumflags2::{BitFlags, FromBitsError, bitflags};
use prost::Message;
use snafu::Snafu;
use vector_buffers::{
    Bufferable, EventCount,
    encoding::{AsMetadata, Encodable},
};
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

/// Per-leaf prost recursion frame cost of a [`Value::Timestamp`].
///
/// Unlike other scalar variants, `Value::Timestamp` is encoded as a nested
/// `google.protobuf.Timestamp` message, so decoding it consumes one additional frame
/// beyond the enclosing `Value`. Without this cost, a timestamp leaf under 32 object
/// levels would sneak past the gate at cost 96 and trip prost's recursion limit on
/// decode at cost 97.
pub(crate) const TIMESTAMP_FRAME_COST: usize = 1;

/// Maximum prost recursion frame cost accepted for any arbitrary [`Value`].
///
/// Prost enforces a decode recursion limit of 100 (no limit on encode). Each nesting level
/// consumes 3 frames for [`Value::Object`], 2 for [`Value::Array`], or 1 for a
/// [`Value::Timestamp`] leaf, plus a fixed overhead for the proto wrappers outside the
/// Value tree.
///
/// Some protobuf paths (`Log.fields` and `Trace.fields`) can carry 99 frames, but the
/// `Log.value` and metadata paths are only safe through 96. We use that highest common
/// safe limit for every value so validation does not depend on its event type, root type,
/// or destination protobuf field.
pub const MAX_VALUE_NESTING_FRAMES: usize = 96;

/// Walks a [`Value`] tree accumulating prost recursion frame cost, returning
/// `Err(over_budget_cost)` as soon as any branch exceeds `budget`.
///
/// Object levels weigh [`OBJECT_FRAME_COST`] frames each, array levels weigh
/// [`ARRAY_FRAME_COST`], and timestamp leaves weigh [`TIMESTAMP_FRAME_COST`] (because
/// they decode into a nested `google.protobuf.Timestamp` message); other scalar leaves
/// are free. Performs an early-exit traversal so well-formed events incur a single
/// descent of the deepest branch only.
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
        Value::Timestamp(_) => TIMESTAMP_FRAME_COST,
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
/// Every arbitrary value is checked against [`MAX_VALUE_NESTING_FRAMES`].
///
/// For metrics, only metadata is checked since metric values have a fixed structure.
pub fn event_exceeds_max_nesting_cost(event: &Event) -> Option<(usize, usize)> {
    let check = |value: &Value| {
        check_value_nesting_cost(value, 0, MAX_VALUE_NESTING_FRAMES)
            .map_err(|cost| (cost, MAX_VALUE_NESTING_FRAMES))
    };
    match event {
        Event::Log(log) => check(log.value())
            .and_then(|()| check(log.metadata().value()))
            .err(),
        Event::Trace(trace) => check(trace.value())
            .and_then(|()| check(trace.metadata().value()))
            .err(),
        Event::Metric(metric) => check(metric.metadata().value()).err(),
    }
}

/// Checks all events in an `EventArray` for nesting cost violations.
///
/// Every arbitrary value is checked against [`MAX_VALUE_NESTING_FRAMES`]. For metrics,
/// only metadata is checked since metric values have a fixed structure.
fn check_event_array_nesting_cost(events: &EventArray) -> Result<(), EncodeError> {
    let check = |value: &Value| {
        check_value_nesting_cost(value, 0, MAX_VALUE_NESTING_FRAMES).map_err(|cost| {
            EncodeError::NestingTooDeep {
                cost,
                budget: MAX_VALUE_NESTING_FRAMES,
            }
        })
    };
    match events {
        EventArray::Logs(logs) => {
            for log in logs {
                check(log.value())?;
                check(log.metadata().value())?;
            }
        }
        EventArray::Traces(traces) => {
            for trace in traces {
                check(trace.value())?;
                check(trace.metadata().value())?;
            }
        }
        EventArray::Metrics(metrics) => {
            for metric in metrics {
                check(metric.metadata().value())?;
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

    /// # Errors
    ///
    /// Returns `EncodeError::NestingTooDeep` if any contained event's value or metadata
    /// exceeds [`MAX_VALUE_NESTING_FRAMES`]. This is **all-or-nothing**: a single
    /// over-budget event fails the entire batch, because a partially-encoded
    /// `EventArray` reaching disk would trip prost's recursion limit on decode and
    /// corrupt the buffer.
    ///
    /// Callers that want graceful per-item drop with telemetry and
    /// `EventStatus::Rejected` must run [`Bufferable::filter_unencodable`] first.
    /// `SenderAdapter::send`/`try_send` already does this on the disk-v2 path, so the
    /// `NestingTooDeep` arm is unreachable from any current production call site — it
    /// is defense-in-depth for a future caller that bypasses `SenderAdapter`.
    ///
    /// Returns `EncodeError::BufferTooSmall` if the buffer cannot hold the encoded
    /// output.
    fn encode<B>(self, buffer: &mut B) -> Result<(), Self::EncodeError>
    where
        B: BufMut,
    {
        check_event_array_nesting_cost(&self)?;

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

impl Bufferable for EventArray {
    /// Reuses the same budget walk as the encode-time gate, so the routing decision and
    /// the eventual encode can never disagree about what is persistable.
    fn is_fully_encodable(&self) -> bool {
        check_event_array_nesting_cost(self).is_ok()
    }

    fn filter_unencodable(self) -> Option<Self> {
        let exceeds =
            |value: &Value| check_value_nesting_cost(value, 0, MAX_VALUE_NESTING_FRAMES).is_err();
        let mut dropped = 0;
        let filtered = match self {
            EventArray::Logs(mut logs) => {
                logs.retain(|log| {
                    let too_deep = exceeds(log.value()) || exceeds(log.metadata().value());
                    if too_deep {
                        log.metadata().update_status(EventStatus::Rejected);
                        dropped += 1;
                    }
                    !too_deep
                });
                EventArray::Logs(logs)
            }
            EventArray::Traces(mut traces) => {
                traces.retain(|trace| {
                    let too_deep = exceeds(trace.value()) || exceeds(trace.metadata().value());
                    if too_deep {
                        trace.metadata().update_status(EventStatus::Rejected);
                        dropped += 1;
                    }
                    !too_deep
                });
                EventArray::Traces(traces)
            }
            EventArray::Metrics(mut metrics) => {
                metrics.retain(|metric| {
                    let too_deep = exceeds(metric.metadata().value());
                    if too_deep {
                        metric.metadata().update_status(EventStatus::Rejected);
                        dropped += 1;
                    }
                    !too_deep
                });
                EventArray::Metrics(metrics)
            }
        };
        if dropped > 0 {
            internal_event::emit(ComponentEventsDropped::<UNINTENTIONAL> {
                count: dropped,
                reason: "Event nesting cost exceeds maximum for protobuf encoding.",
            });
        }
        (filtered.event_count() > 0).then_some(filtered)
    }
}
