use std::{collections::HashMap, fmt};

use chrono::Utc;
use futures::{Stream, StreamExt};
use metrics::{register_histogram, Histogram};
use value::Value;
use vector_buffers::topology::channel::{self, LimitedReceiver, LimitedSender};
#[cfg(test)]
use vector_core::event::{into_event_stream, EventStatus};
use vector_core::{
    config::{log_schema, Output},
    event::{array, Event, EventArray, EventContainer, EventRef},
    internal_event::{
        self, CountByteSize, EventsSent, InternalEventHandle as _, Registered, DEFAULT_OUTPUT,
    },
    ByteSizeOf, EstimatedJsonEncodedSizeOf,
};

mod errors;

pub use errors::{ClosedError, StreamSendError};

pub(crate) const CHUNK_SIZE: usize = 1000;

#[cfg(test)]
const TEST_BUFFER_SIZE: usize = 100;

const LAG_TIME_NAME: &str = "source_lag_time_seconds";

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

    pub fn add_output(&mut self, output: Output) -> LimitedReceiver<EventArray> {
        let lag_time = self.lag_time.clone();
        match output.port {
            None => {
                let (inner, rx) =
                    Inner::new_with_buffer(self.buf_size, DEFAULT_OUTPUT.to_owned(), lag_time);
                self.inner = Some(inner);
                rx
            }
            Some(name) => {
                let (inner, rx) = Inner::new_with_buffer(self.buf_size, name.clone(), lag_time);
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
            lag_time: Some(register_histogram!(LAG_TIME_NAME)),
        }
    }

    pub fn new_with_buffer(n: usize) -> (Self, LimitedReceiver<EventArray>) {
        let lag_time = Some(register_histogram!(LAG_TIME_NAME));
        let (inner, rx) = Inner::new_with_buffer(n, DEFAULT_OUTPUT.to_owned(), lag_time);
        (
            Self {
                inner: Some(inner),
                named_inners: Default::default(),
            },
            rx,
        )
    }

    #[cfg(test)]
    pub fn new_test() -> (Self, impl Stream<Item = Event> + Unpin) {
        let (pipe, recv) = Self::new_with_buffer(TEST_BUFFER_SIZE);
        let recv = recv.into_stream().flat_map(into_event_stream);
        (pipe, recv)
    }

    #[cfg(test)]
    pub fn new_test_finalize(status: EventStatus) -> (Self, impl Stream<Item = Event> + Unpin) {
        let (pipe, recv) = Self::new_with_buffer(TEST_BUFFER_SIZE);
        // In a source test pipeline, there is no sink to acknowledge
        // events, so we have to add a map to the receiver to handle the
        // finalization.
        let recv = recv.into_stream().flat_map(move |mut events| {
            events.iter_events_mut().for_each(|mut event| {
                let metadata = event.metadata_mut();
                metadata.update_status(status);
                metadata.update_sources();
            });
            into_event_stream(events)
        });
        (pipe, recv)
    }

    #[cfg(test)]
    pub fn new_test_errors(
        error_at: impl Fn(usize) -> bool,
    ) -> (Self, impl Stream<Item = Event> + Unpin) {
        let (pipe, recv) = Self::new_with_buffer(TEST_BUFFER_SIZE);
        // In a source test pipeline, there is no sink to acknowledge
        // events, so we have to add a map to the receiver to handle the
        // finalization.
        let mut count: usize = 0;
        let recv = recv.into_stream().flat_map(move |mut events| {
            let status = if error_at(count) {
                EventStatus::Errored
            } else {
                EventStatus::Delivered
            };
            count += 1;
            events.iter_events_mut().for_each(|mut event| {
                let metadata = event.metadata_mut();
                metadata.update_status(status);
                metadata.update_sources();
            });
            into_event_stream(events)
        });
        (pipe, recv)
    }

    #[cfg(test)]
    pub fn add_outputs(
        &mut self,
        status: EventStatus,
        name: String,
    ) -> impl Stream<Item = EventArray> + Unpin {
        // The lag_time parameter here will need to be filled in if this function is ever used for
        // non-test situations.
        let (inner, recv) = Inner::new_with_buffer(100, name.clone(), None);
        let recv = recv.into_stream().map(move |mut events| {
            events.iter_events_mut().for_each(|mut event| {
                let metadata = event.metadata_mut();
                metadata.update_status(status);
                metadata.update_sources();
            });
            events
        });
        self.named_inners.insert(name, inner);
        recv
    }

    pub async fn send_event(&mut self, event: impl Into<EventArray>) -> Result<(), ClosedError> {
        self.inner
            .as_mut()
            .expect("no default output")
            .send_event(event)
            .await
    }

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

    pub async fn send_batch<I, E>(&mut self, events: I) -> Result<(), ClosedError>
    where
        E: Into<Event> + ByteSizeOf,
        I: IntoIterator<Item = E>,
    {
        self.inner
            .as_mut()
            .expect("no default output")
            .send_batch(events)
            .await
    }

    pub async fn send_batch_named<I, E>(&mut self, name: &str, events: I) -> Result<(), ClosedError>
    where
        E: Into<Event> + ByteSizeOf,
        I: IntoIterator<Item = E>,
    {
        self.named_inners
            .get_mut(name)
            .expect("unknown output")
            .send_batch(events)
            .await
    }
}

#[derive(Clone)]
struct Inner {
    inner: LimitedSender<EventArray>,
    output: String,
    lag_time: Option<Histogram>,
    events_sent: Registered<EventsSent>,
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
    ) -> (Self, LimitedReceiver<EventArray>) {
        let (tx, rx) = channel::limited(n);
        (
            Self {
                inner: tx,
                output: output.clone(),
                lag_time,
                events_sent: register!(EventsSent::from(internal_event::Output(Some(
                    output.into()
                )))),
            },
            rx,
        )
    }

    async fn send(&mut self, events: EventArray) -> Result<(), ClosedError> {
        let reference = Utc::now().timestamp_millis();
        events
            .iter_events()
            .for_each(|event| self.emit_lag_time(event, reference));
        let byte_size = events.estimated_json_encoded_size_of();
        let count = events.len();
        self.inner.send(events).await.map_err(|_| ClosedError)?;
        self.events_sent.emit(CountByteSize(count, byte_size));
        Ok(())
    }

    async fn send_event(&mut self, event: impl Into<EventArray>) -> Result<(), ClosedError> {
        self.send(event.into()).await
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
    {
        let reference = Utc::now().timestamp_millis();
        let events = events.into_iter().map(Into::into);
        for events in array::events_into_arrays(events, Some(CHUNK_SIZE)) {
            events
                .iter_events()
                .for_each(|event| self.emit_lag_time(event, reference));
            let cbs = CountByteSize(events.len(), events.estimated_json_encoded_size_of());
            match self.inner.send(events).await {
                Ok(()) => {
                    self.events_sent.emit(cbs);
                }
                Err(error) => {
                    return Err(error.into());
                }
            }
        }

        Ok(())
    }

    /// Calculate the difference between the reference time and the
    /// timestamp stored in the given event reference, and emit the
    /// different, as expressed in milliseconds, as a histogram.
    fn emit_lag_time(&self, event: EventRef<'_>, reference: i64) {
        if let Some(lag_time_metric) = &self.lag_time {
            let timestamp = match event {
                EventRef::Log(log) => log
                    .get(log_schema().timestamp_key())
                    .and_then(get_timestamp_millis),
                EventRef::Metric(metric) => metric
                    .timestamp()
                    .map(|timestamp| timestamp.timestamp_millis()),
                EventRef::Trace(trace) => trace
                    .get(log_schema().timestamp_key())
                    .and_then(get_timestamp_millis),
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

fn get_timestamp_millis(value: &Value) -> Option<i64> {
    match value {
        Value::Timestamp(timestamp) => Some(timestamp.timestamp_millis()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Duration};
    use rand::{thread_rng, Rng};
    use vector_core::event::{LogEvent, Metric, MetricKind, MetricValue, TraceEvent};

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
            trace.insert("timestamp", timestamp);
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
}
