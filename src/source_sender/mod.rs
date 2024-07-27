#![allow(missing_docs)]
use std::{collections::HashMap, fmt, sync::Arc, time::Instant};

use chrono::Utc;
use futures::{Stream, StreamExt};
use metrics::{histogram, Histogram};
use tracing::Span;
use vector_lib::buffers::topology::channel::{self, LimitedReceiver, LimitedSender};
use vector_lib::buffers::EventCount;
use vector_lib::event::array::EventArrayIntoIter;
#[cfg(any(test, feature = "test-utils"))]
use vector_lib::event::{into_event_stream, EventStatus};
use vector_lib::finalization::{AddBatchNotifier, BatchNotifier};
use vector_lib::internal_event::{ComponentEventsDropped, UNINTENTIONAL};
use vector_lib::json_size::JsonSize;
use vector_lib::{
    config::{log_schema, SourceOutput},
    event::{array, Event, EventArray, EventContainer, EventRef},
    internal_event::{
        self, CountByteSize, EventsSent, InternalEventHandle as _, Registered, DEFAULT_OUTPUT,
    },
    ByteSizeOf, EstimatedJsonEncodedSizeOf,
};
use vrl::value::Value;

mod errors;

use crate::config::{ComponentKey, OutputId};
use crate::schema::Definition;
pub use errors::{ClosedError, StreamSendError};

pub(crate) const CHUNK_SIZE: usize = 1000;

#[cfg(any(test, feature = "test-utils"))]
const TEST_BUFFER_SIZE: usize = 100;

const LAG_TIME_NAME: &str = "source_lag_time_seconds";

/// SourceSenderItem is a thin wrapper around [EventArray] used to track the send duration of a batch.
///
/// This is needed because the send duration is calculated as the difference between when the batch
/// is sent from the origin component to when the batch is enqueued on the receiving component's input buffer.
/// For sources in particular, this requires the batch to be enqueued on two channels: the origin component's pump
/// channel and then the receiving component's input buffer.
#[derive(Debug)]
pub struct SourceSenderItem {
    /// The batch of events to send.
    pub events: EventArray,
    /// Reference instant used to calculate send duration.
    pub send_reference: Instant,
}

impl AddBatchNotifier for SourceSenderItem {
    fn add_batch_notifier(&mut self, notifier: BatchNotifier) {
        self.events.add_batch_notifier(notifier)
    }
}

impl ByteSizeOf for SourceSenderItem {
    fn allocated_bytes(&self) -> usize {
        self.events.allocated_bytes()
    }
}

impl EventCount for SourceSenderItem {
    fn event_count(&self) -> usize {
        self.events.event_count()
    }
}

impl EstimatedJsonEncodedSizeOf for SourceSenderItem {
    fn estimated_json_encoded_size_of(&self) -> JsonSize {
        self.events.estimated_json_encoded_size_of()
    }
}

impl EventContainer for SourceSenderItem {
    type IntoIter = EventArrayIntoIter;

    fn len(&self) -> usize {
        self.events.len()
    }

    fn into_events(self) -> Self::IntoIter {
        self.events.into_events()
    }
}

impl From<SourceSenderItem> for EventArray {
    fn from(val: SourceSenderItem) -> Self {
        val.events
    }
}

pub struct Builder {
    buf_size: usize,
    inner: Option<Inner>,
    named_inners: HashMap<String, Inner>,
    lag_time: Option<Histogram>,
}

impl Builder {
    // https://github.com/rust-lang/rust/issues/73255
    #[allow(clippy::missing_const_for_fn)]
    pub fn with_buffer(self, n: usize) -> Self {
        Self {
            buf_size: n,
            inner: self.inner,
            named_inners: self.named_inners,
            lag_time: self.lag_time,
        }
    }

    pub fn add_source_output(
        &mut self,
        output: SourceOutput,
        component_key: ComponentKey,
    ) -> LimitedReceiver<SourceSenderItem> {
        let lag_time = self.lag_time.clone();
        let log_definition = output.schema_definition.clone();
        let output_id = OutputId {
            component: component_key,
            port: output.port.clone(),
        };
        match output.port {
            None => {
                let (inner, rx) = Inner::new_with_buffer(
                    self.buf_size,
                    DEFAULT_OUTPUT.to_owned(),
                    lag_time,
                    log_definition,
                    output_id,
                );
                self.inner = Some(inner);
                rx
            }
            Some(name) => {
                let (inner, rx) = Inner::new_with_buffer(
                    self.buf_size,
                    name.clone(),
                    lag_time,
                    log_definition,
                    output_id,
                );
                self.named_inners.insert(name, inner);
                rx
            }
        }
    }

    // https://github.com/rust-lang/rust/issues/73255
    #[allow(clippy::missing_const_for_fn)]
    pub fn build(self) -> SourceSender {
        SourceSender {
            inner: self.inner,
            named_inners: self.named_inners,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SourceSender {
    inner: Option<Inner>,
    named_inners: HashMap<String, Inner>,
}

impl SourceSender {
    pub fn builder() -> Builder {
        Builder {
            buf_size: CHUNK_SIZE,
            inner: None,
            named_inners: Default::default(),
            lag_time: Some(histogram!(LAG_TIME_NAME)),
        }
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn new_test_sender_with_buffer(n: usize) -> (Self, LimitedReceiver<SourceSenderItem>) {
        let lag_time = Some(histogram!(LAG_TIME_NAME));
        let output_id = OutputId {
            component: "test".to_string().into(),
            port: None,
        };
        let (inner, rx) =
            Inner::new_with_buffer(n, DEFAULT_OUTPUT.to_owned(), lag_time, None, output_id);
        (
            Self {
                inner: Some(inner),
                named_inners: Default::default(),
            },
            rx,
        )
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn new_test() -> (Self, impl Stream<Item = Event> + Unpin) {
        let (pipe, recv) = Self::new_test_sender_with_buffer(TEST_BUFFER_SIZE);
        let recv = recv.into_stream().flat_map(into_event_stream);
        (pipe, recv)
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn new_test_finalize(status: EventStatus) -> (Self, impl Stream<Item = Event> + Unpin) {
        let (pipe, recv) = Self::new_test_sender_with_buffer(TEST_BUFFER_SIZE);
        // In a source test pipeline, there is no sink to acknowledge
        // events, so we have to add a map to the receiver to handle the
        // finalization.
        let recv = recv.into_stream().flat_map(move |mut item| {
            item.events.iter_events_mut().for_each(|mut event| {
                let metadata = event.metadata_mut();
                metadata.update_status(status);
                metadata.update_sources();
            });
            into_event_stream(item)
        });
        (pipe, recv)
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn new_test_errors(
        error_at: impl Fn(usize) -> bool,
    ) -> (Self, impl Stream<Item = Event> + Unpin) {
        let (pipe, recv) = Self::new_test_sender_with_buffer(TEST_BUFFER_SIZE);
        // In a source test pipeline, there is no sink to acknowledge
        // events, so we have to add a map to the receiver to handle the
        // finalization.
        let mut count: usize = 0;
        let recv = recv.into_stream().flat_map(move |mut item| {
            let status = if error_at(count) {
                EventStatus::Errored
            } else {
                EventStatus::Delivered
            };
            count += 1;
            item.events.iter_events_mut().for_each(|mut event| {
                let metadata = event.metadata_mut();
                metadata.update_status(status);
                metadata.update_sources();
            });
            into_event_stream(item)
        });
        (pipe, recv)
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn add_outputs(
        &mut self,
        status: EventStatus,
        name: String,
    ) -> impl Stream<Item = SourceSenderItem> + Unpin {
        // The lag_time parameter here will need to be filled in if this function is ever used for
        // non-test situations.
        let output_id = OutputId {
            component: "test".to_string().into(),
            port: Some(name.clone()),
        };
        let (inner, recv) = Inner::new_with_buffer(100, name.clone(), None, None, output_id);
        let recv = recv.into_stream().map(move |mut item| {
            item.events.iter_events_mut().for_each(|mut event| {
                let metadata = event.metadata_mut();
                metadata.update_status(status);
                metadata.update_sources();
            });
            item
        });
        self.named_inners.insert(name, inner);
        recv
    }

    /// Send an event to the default output.
    ///
    /// This internally handles emitting [EventsSent] and [ComponentEventsDropped] events.
    pub async fn send_event(&mut self, event: impl Into<EventArray>) -> Result<(), ClosedError> {
        self.inner
            .as_mut()
            .expect("no default output")
            .send_event(event)
            .await
    }

    /// Send a stream of events to the default output.
    ///
    /// This internally handles emitting [EventsSent] and [ComponentEventsDropped] events.
    pub async fn send_event_stream<S, E>(&mut self, events: S) -> Result<(), ClosedError>
    where
        S: Stream<Item = E> + Unpin,
        E: Into<Event> + ByteSizeOf,
    {
        self.inner
            .as_mut()
            .expect("no default output")
            .send_event_stream(events)
            .await
    }

    /// Send a batch of events to the default output.
    ///
    /// This internally handles emitting [EventsSent] and [ComponentEventsDropped] events.
    pub async fn send_batch<I, E>(&mut self, events: I) -> Result<(), ClosedError>
    where
        E: Into<Event> + ByteSizeOf,
        I: IntoIterator<Item = E>,
        <I as IntoIterator>::IntoIter: ExactSizeIterator,
    {
        self.inner
            .as_mut()
            .expect("no default output")
            .send_batch(events)
            .await
    }

    /// Send a batch of events event to a named output.
    ///
    /// This internally handles emitting [EventsSent] and [ComponentEventsDropped] events.
    pub async fn send_batch_named<I, E>(&mut self, name: &str, events: I) -> Result<(), ClosedError>
    where
        E: Into<Event> + ByteSizeOf,
        I: IntoIterator<Item = E>,
        <I as IntoIterator>::IntoIter: ExactSizeIterator,
    {
        self.named_inners
            .get_mut(name)
            .expect("unknown output")
            .send_batch(events)
            .await
    }
}

/// UnsentEvents tracks the number of events yet to be sent in the buffer. This is used to
/// increment the appropriate counters when a future is not polled to completion. Particularly,
/// this is known to happen in a Warp server when a client sends a new HTTP request on a TCP
/// connection that already has a pending request.
///
/// If its internal count is greater than 0 when dropped, the appropriate [ComponentEventsDropped]
/// event is emitted.
struct UnsentEventCount {
    count: usize,
    span: Span,
}

impl UnsentEventCount {
    fn new(count: usize) -> Self {
        Self {
            count,
            span: Span::current(),
        }
    }

    fn decr(&mut self, count: usize) {
        self.count = self.count.saturating_sub(count);
    }

    fn discard(&mut self) {
        self.count = 0;
    }
}

impl Drop for UnsentEventCount {
    fn drop(&mut self) {
        if self.count > 0 {
            let _enter = self.span.enter();
            emit!(ComponentEventsDropped::<UNINTENTIONAL> {
                count: self.count,
                reason: "Source send cancelled."
            });
        }
    }
}

#[derive(Clone)]
struct Inner {
    inner: LimitedSender<SourceSenderItem>,
    output: String,
    lag_time: Option<Histogram>,
    events_sent: Registered<EventsSent>,
    /// The schema definition that will be attached to Log events sent through here
    log_definition: Option<Arc<Definition>>,
    /// The OutputId related to this source sender. This is set as the `upstream_id` in
    /// `EventMetadata` for all event sent through here.
    output_id: Arc<OutputId>,
}

impl fmt::Debug for Inner {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Inner")
            .field("inner", &self.inner)
            .field("output", &self.output)
            // `metrics::Histogram` is missing `impl Debug`
            .finish()
    }
}

impl Inner {
    fn new_with_buffer(
        n: usize,
        output: String,
        lag_time: Option<Histogram>,
        log_definition: Option<Arc<Definition>>,
        output_id: OutputId,
    ) -> (Self, LimitedReceiver<SourceSenderItem>) {
        let (tx, rx) = channel::limited(n);
        (
            Self {
                inner: tx,
                output: output.clone(),
                lag_time,
                events_sent: register!(EventsSent::from(internal_event::Output(Some(
                    output.into()
                )))),
                log_definition,
                output_id: Arc::new(output_id),
            },
            rx,
        )
    }

    async fn send(&mut self, mut events: EventArray) -> Result<(), ClosedError> {
        let send_reference = Instant::now();
        let reference = Utc::now().timestamp_millis();
        events
            .iter_events()
            .for_each(|event| self.emit_lag_time(event, reference));

        events.iter_events_mut().for_each(|mut event| {
            // attach runtime schema definitions from the source
            if let Some(log_definition) = &self.log_definition {
                event.metadata_mut().set_schema_definition(log_definition);
            }
            event
                .metadata_mut()
                .set_upstream_id(Arc::clone(&self.output_id));
        });

        let byte_size = events.estimated_json_encoded_size_of();
        let count = events.len();
        self.inner
            .send(SourceSenderItem {
                events,
                send_reference,
            })
            .await
            .map_err(|_| ClosedError)?;
        self.events_sent.emit(CountByteSize(count, byte_size));
        Ok(())
    }

    async fn send_event(&mut self, event: impl Into<EventArray>) -> Result<(), ClosedError> {
        let event: EventArray = event.into();
        // It's possible that the caller stops polling this future while it is blocked waiting
        // on `self.send()`. When that happens, we use `UnsentEventCount` to correctly emit
        // `ComponentEventsDropped` events.
        let count = event.len();
        let mut unsent_event_count = UnsentEventCount::new(count);
        let res = self.send(event).await;
        unsent_event_count.discard();
        res
    }

    async fn send_event_stream<S, E>(&mut self, events: S) -> Result<(), ClosedError>
    where
        S: Stream<Item = E> + Unpin,
        E: Into<Event> + ByteSizeOf,
    {
        let mut stream = events.ready_chunks(CHUNK_SIZE);
        while let Some(events) = stream.next().await {
            self.send_batch(events.into_iter()).await?;
        }
        Ok(())
    }

    async fn send_batch<I, E>(&mut self, events: I) -> Result<(), ClosedError>
    where
        E: Into<Event> + ByteSizeOf,
        I: IntoIterator<Item = E>,
        <I as IntoIterator>::IntoIter: ExactSizeIterator,
    {
        // It's possible that the caller stops polling this future while it is blocked waiting
        // on `self.send()`. When that happens, we use `UnsentEventCount` to correctly emit
        // `ComponentEventsDropped` events.
        let events = events.into_iter().map(Into::into);
        let mut unsent_event_count = UnsentEventCount::new(events.len());
        for events in array::events_into_arrays(events, Some(CHUNK_SIZE)) {
            let count = events.len();
            self.send(events).await.map_err(|err| {
                // The unsent event count is discarded here because the caller emits the
                // `StreamClosedError`.
                unsent_event_count.discard();
                err
            })?;
            unsent_event_count.decr(count);
        }
        Ok(())
    }

    /// Calculate the difference between the reference time and the
    /// timestamp stored in the given event reference, and emit the
    /// different, as expressed in milliseconds, as a histogram.
    fn emit_lag_time(&self, event: EventRef<'_>, reference: i64) {
        if let Some(lag_time_metric) = &self.lag_time {
            let timestamp = match event {
                EventRef::Log(log) => {
                    log_schema()
                        .timestamp_key_target_path()
                        .and_then(|timestamp_key| {
                            log.get(timestamp_key).and_then(get_timestamp_millis)
                        })
                }
                EventRef::Metric(metric) => metric
                    .timestamp()
                    .map(|timestamp| timestamp.timestamp_millis()),
                EventRef::Trace(trace) => {
                    log_schema()
                        .timestamp_key_target_path()
                        .and_then(|timestamp_key| {
                            trace.get(timestamp_key).and_then(get_timestamp_millis)
                        })
                }
            };
            if let Some(timestamp) = timestamp {
                // This will truncate precision for values larger than 2**52, but at that point the user
                // probably has much larger problems than precision.
                let lag_time = (reference - timestamp) as f64 / 1000.0;
                lag_time_metric.record(lag_time);
            }
        }
    }
}

const fn get_timestamp_millis(value: &Value) -> Option<i64> {
    match value {
        Value::Timestamp(timestamp) => Some(timestamp.timestamp_millis()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Duration};
    use rand::{thread_rng, Rng};
    use tokio::time::timeout;
    use vector_lib::event::{LogEvent, Metric, MetricKind, MetricValue, TraceEvent};
    use vrl::event_path;

    use super::*;
    use crate::metrics::{self, Controller};

    #[tokio::test]
    async fn emits_lag_time_for_log() {
        emit_and_test(|timestamp| {
            let mut log = LogEvent::from("Log message");
            log.insert("timestamp", timestamp);
            Event::Log(log)
        })
        .await;
    }

    #[tokio::test]
    async fn emits_lag_time_for_metric() {
        emit_and_test(|timestamp| {
            Event::Metric(
                Metric::new(
                    "name",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 123.4 },
                )
                .with_timestamp(Some(timestamp)),
            )
        })
        .await;
    }

    #[tokio::test]
    async fn emits_lag_time_for_trace() {
        emit_and_test(|timestamp| {
            let mut trace = TraceEvent::default();
            trace.insert(event_path!("timestamp"), timestamp);
            Event::Trace(trace)
        })
        .await;
    }

    async fn emit_and_test(make_event: impl FnOnce(DateTime<Utc>) -> Event) {
        metrics::init_test();
        let (mut sender, _stream) = SourceSender::new_test();
        let millis = thread_rng().gen_range(10..10000);
        let timestamp = Utc::now() - Duration::milliseconds(millis);
        let expected = millis as f64 / 1000.0;

        let event = make_event(timestamp);
        sender
            .send_event(event)
            .await
            .expect("Send should not fail");

        let lag_times = Controller::get()
            .expect("There must be a controller")
            .capture_metrics()
            .into_iter()
            .filter(|metric| metric.name() == "source_lag_time_seconds")
            .collect::<Vec<_>>();
        assert_eq!(lag_times.len(), 1);

        let lag_time = &lag_times[0];
        match lag_time.value() {
            MetricValue::AggregatedHistogram {
                buckets,
                count,
                sum,
            } => {
                let mut done = false;
                for bucket in buckets {
                    if !done && bucket.upper_limit >= expected {
                        assert_eq!(bucket.count, 1);
                        done = true;
                    } else {
                        assert_eq!(bucket.count, 0);
                    }
                }
                assert_eq!(*count, 1);
                assert!(
                    (*sum - expected).abs() <= 0.002,
                    "Histogram sum does not match expected sum: {} vs {}",
                    *sum,
                    expected,
                );
            }
            _ => panic!("source_lag_time_seconds has invalid type"),
        }
    }

    #[tokio::test]
    async fn emits_component_discarded_events_total_for_send_event() {
        metrics::init_test();
        let (mut sender, _recv) = SourceSender::new_test_sender_with_buffer(1);

        let event = Event::Metric(Metric::new(
            "name",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 123.4 },
        ));

        // First send will succeed.
        sender
            .send_event(event.clone())
            .await
            .expect("First send should not fail");

        // Second send will timeout, so the future will not be polled to completion.
        let res = timeout(
            std::time::Duration::from_millis(100),
            sender.send_event(event.clone()),
        )
        .await;
        assert!(res.is_err(), "Send should have timed out.");

        let component_discarded_events_total = Controller::get()
            .expect("There must be a controller")
            .capture_metrics()
            .into_iter()
            .filter(|metric| metric.name() == "component_discarded_events_total")
            .collect::<Vec<_>>();
        assert_eq!(component_discarded_events_total.len(), 1);

        let component_discarded_events_total = &component_discarded_events_total[0];
        let MetricValue::Counter { value } = component_discarded_events_total.value() else {
            panic!("component_discarded_events_total has invalid type")
        };
        assert_eq!(*value, 1.0);
    }

    #[tokio::test]
    async fn emits_component_discarded_events_total_for_send_batch() {
        metrics::init_test();
        let (mut sender, _recv) = SourceSender::new_test_sender_with_buffer(1);

        let expected_drop = 100;
        let events: Vec<Event> = (0..(CHUNK_SIZE + expected_drop))
            .map(|_| {
                Event::Metric(Metric::new(
                    "name",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 123.4 },
                ))
            })
            .collect();

        // `CHUNK_SIZE` events will be sent into buffer but then the future will not be polled to completion.
        let res = timeout(
            std::time::Duration::from_millis(100),
            sender.send_batch(events),
        )
        .await;
        assert!(res.is_err(), "Send should have timed out.");

        let component_discarded_events_total = Controller::get()
            .expect("There must be a controller")
            .capture_metrics()
            .into_iter()
            .filter(|metric| metric.name() == "component_discarded_events_total")
            .collect::<Vec<_>>();
        assert_eq!(component_discarded_events_total.len(), 1);

        let component_discarded_events_total = &component_discarded_events_total[0];
        let MetricValue::Counter { value } = component_discarded_events_total.value() else {
            panic!("component_discarded_events_total has invalid type")
        };
        assert_eq!(*value, expected_drop as f64);
    }
}
