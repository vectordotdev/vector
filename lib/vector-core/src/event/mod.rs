use std::{
    collections::{BTreeMap, HashMap},
    convert::{TryFrom, TryInto},
    fmt::Debug,
    sync::Arc,
};

use bytes::{Buf, BufMut, Bytes};
use chrono::{DateTime, SecondsFormat, Utc};
use enumflags2::{bitflags, BitFlags, FromBitsError};
use prost::Message;
use shared::EventDataEq;
use snafu::Snafu;
use vector_buffers::encoding::{AsMetadata, Encodable};

use crate::ByteSizeOf;
pub use array::{EventArray, EventContainer, LogArray, MetricArray};
pub use finalization::{
    BatchNotifier, BatchStatus, BatchStatusReceiver, EventFinalizer, EventFinalizers, EventStatus,
    Finalizable,
};
pub use legacy_lookup::Lookup;
pub use log_event::LogEvent;
pub use metadata::{EventMetadata, WithMetadata};
pub use metric::{Metric, MetricKind, MetricValue, StatisticKind};
pub use util::log::{PathComponent, PathIter};
pub use value::Value;
#[cfg(feature = "vrl")]
pub use vrl_target::VrlTarget;

pub mod array;
pub mod discriminant;
pub mod error;
mod finalization;
mod legacy_lookup;
mod log_event;
#[cfg(feature = "lua")]
pub mod lua;
pub mod merge_state;
mod metadata;
pub mod metric;
pub mod proto;
#[cfg(test)]
mod test;
pub mod util;
mod value;
#[cfg(feature = "vrl")]
mod vrl_target;

pub const PARTIAL: &str = "_partial";

#[derive(PartialEq, PartialOrd, Debug, Clone)]
pub enum Event {
    Log(LogEvent),
    Metric(Metric),
}

impl ByteSizeOf for Event {
    fn allocated_bytes(&self) -> usize {
        match self {
            Event::Log(log_event) => log_event.allocated_bytes(),
            Event::Metric(metric_event) => metric_event.allocated_bytes(),
        }
    }
}

impl Finalizable for Event {
    fn take_finalizers(&mut self) -> EventFinalizers {
        match self {
            Event::Log(log) => log.take_finalizers(),
            Event::Metric(metric) => metric.take_finalizers(),
        }
    }
}

impl Event {
    #[must_use]
    pub fn new_empty_log() -> Self {
        Event::Log(LogEvent::default())
    }

    /// Return self as a `LogEvent`
    ///
    /// # Panics
    ///
    /// This function panics if self is anything other than an `Event::Log`.
    pub fn as_log(&self) -> &LogEvent {
        match self {
            Event::Log(log) => log,
            _ => panic!("Failed type coercion, {:?} is not a log event", self),
        }
    }

    /// Return self as a mutable `LogEvent`
    ///
    /// # Panics
    ///
    /// This function panics if self is anything other than an `Event::Log`.
    pub fn as_mut_log(&mut self) -> &mut LogEvent {
        match self {
            Event::Log(log) => log,
            _ => panic!("Failed type coercion, {:?} is not a log event", self),
        }
    }

    /// Coerces self into a `LogEvent`
    ///
    /// # Panics
    ///
    /// This function panics if self is anything other than an `Event::Log`.
    pub fn into_log(self) -> LogEvent {
        match self {
            Event::Log(log) => log,
            _ => panic!("Failed type coercion, {:?} is not a log event", self),
        }
    }

    /// Fallibly coerces self into a `LogEvent`
    ///
    /// If the event is a `LogEvent`, then `Some(log_event)` is returned, otherwise `None`.
    pub fn try_into_log(self) -> Option<LogEvent> {
        match self {
            Event::Log(log) => Some(log),
            _ => None,
        }
    }

    /// Return self as a `Metric`
    ///
    /// # Panics
    ///
    /// This function panics if self is anything other than an `Event::Metric`.
    pub fn as_metric(&self) -> &Metric {
        match self {
            Event::Metric(metric) => metric,
            _ => panic!("Failed type coercion, {:?} is not a metric", self),
        }
    }

    /// Return self as a mutable `Metric`
    ///
    /// # Panics
    ///
    /// This function panics if self is anything other than an `Event::Metric`.
    pub fn as_mut_metric(&mut self) -> &mut Metric {
        match self {
            Event::Metric(metric) => metric,
            _ => panic!("Failed type coercion, {:?} is not a metric", self),
        }
    }

    /// Coerces self into `Metric`
    ///
    /// # Panics
    ///
    /// This function panics if self is anything other than an `Event::Metric`.
    pub fn into_metric(self) -> Metric {
        match self {
            Event::Metric(metric) => metric,
            _ => panic!("Failed type coercion, {:?} is not a metric", self),
        }
    }

    /// Fallibly coerces self into a `Metric`
    ///
    /// If the event is a `Metric`, then `Some(metric)` is returned, otherwise `None`.
    pub fn try_into_metric(self) -> Option<Metric> {
        match self {
            Event::Metric(metric) => Some(metric),
            _ => None,
        }
    }

    pub fn metadata(&self) -> &EventMetadata {
        match self {
            Self::Log(log) => log.metadata(),
            Self::Metric(metric) => metric.metadata(),
        }
    }

    pub fn metadata_mut(&mut self) -> &mut EventMetadata {
        match self {
            Self::Log(log) => log.metadata_mut(),
            Self::Metric(metric) => metric.metadata_mut(),
        }
    }

    /// Destroy the event and return the metadata.
    pub fn into_metadata(self) -> EventMetadata {
        match self {
            Self::Log(log) => log.into_parts().1,
            Self::Metric(metric) => metric.into_parts().2,
        }
    }

    pub fn add_batch_notifier(&mut self, batch: Arc<BatchNotifier>) {
        let finalizer = EventFinalizer::new(batch);
        match self {
            Self::Log(log) => log.add_finalizer(finalizer),
            Self::Metric(metric) => metric.add_finalizer(finalizer),
        }
    }

    pub fn with_batch_notifier(self, batch: &Arc<BatchNotifier>) -> Self {
        match self {
            Self::Log(log) => log.with_batch_notifier(batch).into(),
            Self::Metric(metric) => metric.with_batch_notifier(batch).into(),
        }
    }

    pub fn with_batch_notifier_option(self, batch: &Option<Arc<BatchNotifier>>) -> Self {
        match self {
            Self::Log(log) => log.with_batch_notifier_option(batch).into(),
            Self::Metric(metric) => metric.with_batch_notifier_option(batch).into(),
        }
    }
}

impl EventDataEq for Event {
    fn event_data_eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Log(a), Self::Log(b)) => a.event_data_eq(b),
            (Self::Metric(a), Self::Metric(b)) => a.event_data_eq(b),
            _ => false,
        }
    }
}

fn timestamp_to_string(timestamp: &DateTime<Utc>) -> String {
    timestamp.to_rfc3339_opts(SecondsFormat::AutoSi, true)
}

impl From<BTreeMap<String, Value>> for Event {
    fn from(map: BTreeMap<String, Value>) -> Self {
        Self::Log(LogEvent::from(map))
    }
}

impl From<HashMap<String, Value>> for Event {
    fn from(map: HashMap<String, Value>) -> Self {
        Self::Log(LogEvent::from(map))
    }
}

impl TryFrom<serde_json::Value> for Event {
    type Error = crate::Error;

    fn try_from(map: serde_json::Value) -> Result<Self, Self::Error> {
        match map {
            serde_json::Value::Object(fields) => Ok(Event::from(
                fields
                    .into_iter()
                    .map(|(k, v)| (k, v.into()))
                    .collect::<BTreeMap<_, _>>(),
            )),
            _ => Err(crate::Error::from(
                "Attempted to convert non-Object JSON into an Event.",
            )),
        }
    }
}

impl TryInto<serde_json::Value> for Event {
    type Error = serde_json::Error;

    fn try_into(self) -> Result<serde_json::Value, Self::Error> {
        match self {
            Event::Log(fields) => serde_json::to_value(fields),
            Event::Metric(metric) => serde_json::to_value(metric),
        }
    }
}

impl From<proto::StatisticKind> for StatisticKind {
    fn from(kind: proto::StatisticKind) -> Self {
        match kind {
            proto::StatisticKind::Histogram => StatisticKind::Histogram,
            proto::StatisticKind::Summary => StatisticKind::Summary,
        }
    }
}

impl From<metric::Sample> for proto::DistributionSample {
    fn from(sample: metric::Sample) -> Self {
        Self {
            value: sample.value,
            rate: sample.rate,
        }
    }
}

impl From<proto::DistributionSample> for metric::Sample {
    fn from(sample: proto::DistributionSample) -> Self {
        Self {
            value: sample.value,
            rate: sample.rate,
        }
    }
}

impl From<metric::Bucket> for proto::HistogramBucket {
    fn from(bucket: metric::Bucket) -> Self {
        Self {
            upper_limit: bucket.upper_limit,
            count: bucket.count,
        }
    }
}

impl From<proto::HistogramBucket> for metric::Bucket {
    fn from(bucket: proto::HistogramBucket) -> Self {
        Self {
            upper_limit: bucket.upper_limit,
            count: bucket.count,
        }
    }
}

impl From<metric::Quantile> for proto::SummaryQuantile {
    fn from(quantile: metric::Quantile) -> Self {
        Self {
            quantile: quantile.quantile,
            value: quantile.value,
        }
    }
}

impl From<proto::SummaryQuantile> for metric::Quantile {
    fn from(quantile: proto::SummaryQuantile) -> Self {
        Self {
            quantile: quantile.quantile,
            value: quantile.value,
        }
    }
}

impl From<Bytes> for Event {
    fn from(message: Bytes) -> Self {
        Event::Log(LogEvent::from(message))
    }
}

impl From<&str> for Event {
    fn from(line: &str) -> Self {
        LogEvent::from(line).into()
    }
}

impl From<String> for Event {
    fn from(line: String) -> Self {
        LogEvent::from(line).into()
    }
}

impl From<LogEvent> for Event {
    fn from(log: LogEvent) -> Self {
        Event::Log(log)
    }
}

impl From<Metric> for Event {
    fn from(metric: Metric) -> Self {
        Event::Metric(metric)
    }
}

pub trait MaybeAsLogMut {
    fn maybe_as_log_mut(&mut self) -> Option<&mut LogEvent>;
}

impl MaybeAsLogMut for Event {
    fn maybe_as_log_mut(&mut self) -> Option<&mut LogEvent> {
        match self {
            Event::Log(log) => Some(log),
            Event::Metric(_) => None,
        }
    }
}

/// A wrapper for references to inner event types, where reconstituting
/// a full `Event` from a `LogEvent` or `Metric` might be inconvenient.
#[derive(Clone, Copy, Debug)]
pub enum EventRef<'a> {
    Log(&'a LogEvent),
    Metric(&'a Metric),
}

impl<'a> From<&'a Event> for EventRef<'a> {
    fn from(event: &'a Event) -> Self {
        match event {
            Event::Log(log) => log.into(),
            Event::Metric(metric) => metric.into(),
        }
    }
}

impl<'a> From<&'a LogEvent> for EventRef<'a> {
    fn from(log: &'a LogEvent) -> Self {
        Self::Log(log)
    }
}

impl<'a> From<&'a Metric> for EventRef<'a> {
    fn from(metric: &'a Metric) -> Self {
        Self::Metric(metric)
    }
}

#[derive(Debug, Snafu)]
pub enum EncodeError {
    #[snafu(display("the provided buffer was too small to fully encode this item"))]
    BufferTooSmall,
}

#[derive(Debug, Snafu)]
pub enum DecodeError {
    #[snafu(display(
        "the provided buffer could not be decoded as a valid Protocol Buffers payload"
    ))]
    InvalidProtobufPayload,
}

#[bitflags]
#[repr(u32)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum EventEncodableMetadataFlags {
    // Protocol Buffers-based encoding based on the `Event`
    // definition used for Vector gRPC communication.
    ProtocolBuffers = 0b1,
}

#[derive(Clone, Copy, Debug, PartialEq)]
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

impl Encodable for Event {
    type Metadata = EventEncodableMetadata;
    type EncodeError = EncodeError;
    type DecodeError = DecodeError;

    fn get_metadata() -> Self::Metadata {
        EventEncodableMetadataFlags::ProtocolBuffers.into()
    }

    fn can_decode(metadata: Self::Metadata) -> bool {
        metadata.contains(EventEncodableMetadataFlags::ProtocolBuffers)
    }

    fn encode<B>(self, buffer: &mut B) -> Result<(), Self::EncodeError>
    where
        B: BufMut,
    {
        proto::EventWrapper::from(self)
            .encode(buffer)
            .map_err(|_| EncodeError::BufferTooSmall)
    }

    fn decode<B>(_metadata: Self::Metadata, buffer: B) -> Result<Event, Self::DecodeError>
    where
        B: Buf,
    {
        proto::EventWrapper::decode(buffer)
            .map(Into::into)
            .map_err(|_| DecodeError::InvalidProtobufPayload)
    }
}

/// Gets the only valid metadata value that should be given by the `Encodable` implementation of `Event`.
///
/// This is specifically used within a unit test to enforce that if we changed the `Encodable`
/// implementation prior to the LevelDB-based disk buffer being removed entirely, unit tests would
/// fail indicating that a PR/change was making a breaking change that it shouldn't be making.
///
/// REVIEWERS: Be aware, if this is being removed or changed, the only acceptable context is
/// LevelDB-based disk buffers being removed, or some other extenuating circumstance that must be explained.
#[allow(dead_code)]
pub(crate) fn allowed_event_encodable_metadata() -> EventEncodableMetadata {
    EventEncodableMetadataFlags::ProtocolBuffers.into()
}
