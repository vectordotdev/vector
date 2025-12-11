use std::{
    fmt,
    num::NonZeroUsize,
    sync::Arc,
    time::{Duration, Instant},
};

use chrono::Utc;
use futures::{Stream, StreamExt as _};
use metrics::Histogram;
use tracing::Span;
use vector_buffers::{
    config::MemoryBufferSize,
    topology::channel::{self, ChannelMetricMetadata, LimitedReceiver, LimitedSender},
};
use vector_common::{
    byte_size_of::ByteSizeOf,
    internal_event::{
        self, ComponentEventsDropped, ComponentEventsTimedOut, Count, CountByteSize, EventsSent,
        InternalEventHandle as _, RegisterInternalEvent as _, Registered, UNINTENTIONAL,
    },
};
use vrl::value::Value;

use super::{CHUNK_SIZE, SendError, SourceSenderItem};
use crate::{
    EstimatedJsonEncodedSizeOf,
    config::{OutputId, log_schema},
    event::{Event, EventArray, EventContainer as _, EventRef, array},
    schema::Definition,
};

const UTILIZATION_METRIC_PREFIX: &str = "source_buffer";

/// UnsentEvents tracks the number of events yet to be sent in the buffer. This is used to
/// increment the appropriate counters when a future is not polled to completion. Particularly,
/// this is known to happen in a Warp server when a client sends a new HTTP request on a TCP
/// connection that already has a pending request.
///
/// If its internal count is greater than 0 when dropped, the appropriate [ComponentEventsDropped]
/// event is emitted.
pub(super) struct UnsentEventCount {
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

    const fn decr(&mut self, count: usize) {
        self.count = self.count.saturating_sub(count);
    }

    const fn discard(&mut self) {
        self.count = 0;
    }

    fn timed_out(&mut self) {
        ComponentEventsTimedOut {
            reason: "Source send timed out.",
        }
        .register()
        .emit(Count(self.count));
        self.count = 0;
    }
}

impl Drop for UnsentEventCount {
    fn drop(&mut self) {
        if self.count > 0 {
            let _enter = self.span.enter();
            internal_event::emit(ComponentEventsDropped::<UNINTENTIONAL> {
                count: self.count,
                reason: "Source send cancelled.",
            });
        }
    }
}

#[derive(Clone)]
pub(super) struct Output {
    sender: LimitedSender<SourceSenderItem>,
    lag_time: Option<Histogram>,
    events_sent: Registered<EventsSent>,
    /// The schema definition that will be attached to Log events sent through here
    log_definition: Option<Arc<Definition>>,
    /// The OutputId related to this source sender. This is set as the `upstream_id` in
    /// `EventMetadata` for all event sent through here.
    id: Arc<OutputId>,
    timeout: Option<Duration>,
}

#[expect(clippy::missing_fields_in_debug)]
impl fmt::Debug for Output {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Output")
            .field("sender", &self.sender)
            .field("output_id", &self.id)
            .field("timeout", &self.timeout)
            // `metrics::Histogram` is missing `impl Debug`
            .finish()
    }
}

impl Output {
    pub(super) fn new_with_buffer(
        n: usize,
        output: String,
        lag_time: Option<Histogram>,
        log_definition: Option<Arc<Definition>>,
        output_id: OutputId,
        timeout: Option<Duration>,
    ) -> (Self, LimitedReceiver<SourceSenderItem>) {
        let limit = MemoryBufferSize::MaxEvents(NonZeroUsize::new(n).unwrap());
        let metrics = ChannelMetricMetadata::new(UTILIZATION_METRIC_PREFIX, Some(output.clone()));
        let (tx, rx) = channel::limited(limit, Some(metrics));
        (
            Self {
                sender: tx,
                lag_time,
                events_sent: internal_event::register(EventsSent::from(internal_event::Output(
                    Some(output.into()),
                ))),
                log_definition,
                id: Arc::new(output_id),
                timeout,
            },
            rx,
        )
    }

    pub(super) async fn send(
        &mut self,
        mut events: EventArray,
        unsent_event_count: &mut UnsentEventCount,
    ) -> Result<(), SendError> {
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
            event.metadata_mut().set_upstream_id(Arc::clone(&self.id));
        });

        let byte_size = events.estimated_json_encoded_size_of();
        let count = events.len();
        self.send_with_timeout(events, send_reference).await?;
        self.events_sent.emit(CountByteSize(count, byte_size));
        unsent_event_count.decr(count);
        Ok(())
    }

    async fn send_with_timeout(
        &mut self,
        events: EventArray,
        send_reference: Instant,
    ) -> Result<(), SendError> {
        let item = SourceSenderItem {
            events,
            send_reference,
        };
        if let Some(timeout) = self.timeout {
            match tokio::time::timeout(timeout, self.sender.send(item)).await {
                Ok(Ok(())) => Ok(()),
                Ok(Err(error)) => Err(error.into()),
                Err(_elapsed) => Err(SendError::Timeout),
            }
        } else {
            self.sender.send(item).await.map_err(Into::into)
        }
    }

    pub(super) async fn send_event(
        &mut self,
        event: impl Into<EventArray>,
    ) -> Result<(), SendError> {
        let event: EventArray = event.into();
        // It's possible that the caller stops polling this future while it is blocked waiting
        // on `self.send()`. When that happens, we use `UnsentEventCount` to correctly emit
        // `ComponentEventsDropped` events.
        let mut unsent_event_count = UnsentEventCount::new(event.len());
        self.send(event, &mut unsent_event_count)
            .await
            .inspect_err(|error| {
                if let SendError::Timeout = error {
                    unsent_event_count.timed_out();
                }
            })
    }

    pub(super) async fn send_event_stream<S, E>(&mut self, events: S) -> Result<(), SendError>
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

    pub(super) async fn send_batch<I, E>(&mut self, events: I) -> Result<(), SendError>
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
            self.send(events, &mut unsent_event_count)
                .await
                .inspect_err(|error| match error {
                    SendError::Timeout => {
                        unsent_event_count.timed_out();
                    }
                    SendError::Closed => {
                        // The unsent event count is discarded here because the callee emits the
                        // `StreamClosedError`.
                        unsent_event_count.discard();
                    }
                })?;
        }
        Ok(())
    }

    /// Calculate the difference between the reference time and the
    /// timestamp stored in the given event reference, and emit the
    /// different, as expressed in milliseconds, as a histogram.
    pub(super) fn emit_lag_time(&self, event: EventRef<'_>, reference: i64) {
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
                #[expect(clippy::cast_precision_loss)]
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
