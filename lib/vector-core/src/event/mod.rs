use std::{
    collections::{BTreeMap, HashMap},
    convert::{TryFrom, TryInto},
    fmt::Debug,
    sync::Arc,
};

use bytes::Bytes;
use vector_buffers::EventCount;
use vector_common::EventDataEq;

use crate::ByteSizeOf;
pub use ::value::Value;
pub use array::{into_event_stream, EventArray, EventContainer, LogArray, MetricArray, TraceArray};
pub use finalization::{
    BatchNotifier, BatchStatus, BatchStatusReceiver, EventFinalizer, EventFinalizers, EventStatus,
    Finalizable,
};
pub use log_event::LogEvent;
pub use metadata::{EventMetadata, WithMetadata};
pub use metric::{Metric, MetricKind, MetricValue, StatisticKind};
pub use r#ref::{EventMutRef, EventRef};
pub use trace::TraceEvent;
#[cfg(feature = "vrl")]
pub use vrl_target::VrlTarget;

pub mod array;
pub mod discriminant;
pub mod error;
mod finalization;
mod log_event;
#[cfg(feature = "lua")]
pub mod lua;
pub mod merge_state;
mod metadata;
pub mod metric;
pub mod proto;
mod r#ref;
mod ser;
#[cfg(test)]
mod test;
mod trace;
pub mod util;
#[cfg(feature = "vrl")]
mod vrl_target;

pub const PARTIAL: &str = "_partial";

#[derive(PartialEq, PartialOrd, Debug, Clone)]
pub enum Event {
    Log(LogEvent),
    Metric(Metric),
    Trace(TraceEvent),
}

impl ByteSizeOf for Event {
    fn allocated_bytes(&self) -> usize {
        match self {
            Event::Log(log_event) => log_event.allocated_bytes(),
            Event::Metric(metric_event) => metric_event.allocated_bytes(),
            Event::Trace(trace_event) => trace_event.allocated_bytes(),
        }
    }
}

impl EventCount for Event {
    fn event_count(&self) -> usize {
        1
    }
}

impl Finalizable for Event {
    fn take_finalizers(&mut self) -> EventFinalizers {
        match self {
            Event::Log(log_event) => log_event.take_finalizers(),
            Event::Metric(metric) => metric.take_finalizers(),
            Event::Trace(trace_event) => trace_event.take_finalizers(),
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

    /// Return self as a `TraceEvent`
    ///
    /// # Panics
    ///
    /// This function panics if self is anything other than an `Event::Trace`.
    pub fn as_trace(&self) -> &TraceEvent {
        match self {
            Event::Trace(trace) => trace,
            _ => panic!("Failed type coercion, {:?} is not a trace event", self),
        }
    }

    /// Return self as a mutable `TraceEvent`
    ///
    /// # Panics
    ///
    /// This function panics if self is anything other than an `Event::Trace`.
    pub fn as_mut_trace(&mut self) -> &mut TraceEvent {
        match self {
            Event::Trace(trace) => trace,
            _ => panic!("Failed type coercion, {:?} is not a trace event", self),
        }
    }

    /// Coerces self into a `TraceEvent`
    ///
    /// # Panics
    ///
    /// This function panics if self is anything other than an `Event::Trace`.
    pub fn into_trace(self) -> TraceEvent {
        match self {
            Event::Trace(trace) => trace,
            _ => panic!("Failed type coercion, {:?} is not a trace event", self),
        }
    }

    /// Fallibly coerces self into a `TraceEvent`
    ///
    /// If the event is a `TraceEvent`, then `Some(trace)` is returned, otherwise `None`.
    pub fn try_into_trace(self) -> Option<TraceEvent> {
        match self {
            Event::Trace(trace) => Some(trace),
            _ => None,
        }
    }

    pub fn metadata(&self) -> &EventMetadata {
        match self {
            Self::Log(log) => log.metadata(),
            Self::Metric(metric) => metric.metadata(),
            Self::Trace(trace) => trace.metadata(),
        }
    }

    pub fn metadata_mut(&mut self) -> &mut EventMetadata {
        match self {
            Self::Log(log) => log.metadata_mut(),
            Self::Metric(metric) => metric.metadata_mut(),
            Self::Trace(trace) => trace.metadata_mut(),
        }
    }

    /// Destroy the event and return the metadata.
    pub fn into_metadata(self) -> EventMetadata {
        match self {
            Self::Log(log) => log.into_parts().1,
            Self::Metric(metric) => metric.into_parts().2,
            Self::Trace(trace) => trace.into_parts().1,
        }
    }

    pub fn add_batch_notifier(&mut self, batch: Arc<BatchNotifier>) {
        let finalizer = EventFinalizer::new(batch);
        match self {
            Self::Log(log) => log.add_finalizer(finalizer),
            Self::Metric(metric) => metric.add_finalizer(finalizer),
            Self::Trace(trace) => trace.add_finalizer(finalizer),
        }
    }

    #[must_use]
    pub fn with_batch_notifier(self, batch: &Arc<BatchNotifier>) -> Self {
        match self {
            Self::Log(log) => log.with_batch_notifier(batch).into(),
            Self::Metric(metric) => metric.with_batch_notifier(batch).into(),
            Self::Trace(trace) => trace.with_batch_notifier(batch).into(),
        }
    }

    #[must_use]
    pub fn with_batch_notifier_option(self, batch: &Option<Arc<BatchNotifier>>) -> Self {
        match self {
            Self::Log(log) => log.with_batch_notifier_option(batch).into(),
            Self::Metric(metric) => metric.with_batch_notifier_option(batch).into(),
            Self::Trace(trace) => trace.with_batch_notifier_option(batch).into(),
        }
    }
}

impl EventDataEq for Event {
    fn event_data_eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Log(a), Self::Log(b)) => a.event_data_eq(b),
            (Self::Metric(a), Self::Metric(b)) => a.event_data_eq(b),
            (Self::Trace(a), Self::Trace(b)) => a.event_data_eq(b),
            _ => false,
        }
    }
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
            Event::Trace(fields) => serde_json::to_value(fields),
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

impl From<TraceEvent> for Event {
    fn from(trace: TraceEvent) -> Self {
        Event::Trace(trace)
    }
}

pub trait MaybeAsLogMut {
    fn maybe_as_log_mut(&mut self) -> Option<&mut LogEvent>;
}

impl MaybeAsLogMut for Event {
    fn maybe_as_log_mut(&mut self) -> Option<&mut LogEvent> {
        match self {
            Event::Log(log) => Some(log),
            _ => None,
        }
    }
}
