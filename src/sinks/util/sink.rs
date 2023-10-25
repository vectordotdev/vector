//! This module contains all our internal sink utilities
//!
//! All vector "sinks" are built around the `Sink` type which
//! we use to "push" events into. Within the different types of
//! vector "sinks" we need to support three main use cases:
//!
//! - Streaming sinks
//! - Single partition batching
//! - Multiple partition batching
//!
//! For each of these types this module provides one external type
//! that can be used within sinks. The simplest type being the `StreamSink`
//! type should be used when you do not want to batch events but you want
//! to _stream_ them to the downstream service. `BatchSink` and `PartitionBatchSink`
//! are similar in the sense that they both take some `tower::Service`, and `Batch`
//! and will provide full batching, and request dispatching based on
//! the settings passed.
//!
//! For more advanced use cases like HTTP based sinks, one should use the
//! `BatchedHttpSink` type, which is a wrapper for `BatchSink` and `HttpSink`.
//!
//! # Driving to completion
//!
//! Each sink utility provided here strictly follows the patterns described in
//! the `futures::Sink` docs. Each sink utility must be polled from a valid
//! tokio context.
//!
//! For service based sinks like `BatchSink` and `PartitionBatchSink` they also
//! must be polled within a valid tokio executor context. This is due to the fact
//! that they will spawn service requests to allow them to be driven independently
//! from the sink. A oneshot channel is used to tie them back into the sink to allow
//! it to notify the consumer that the request has succeeded.

use std::{
    collections::HashMap,
    fmt,
    hash::Hash,
    marker::PhantomData,
    pin::Pin,
    task::{ready, Context, Poll},
};

use futures::{future::BoxFuture, stream::FuturesUnordered, FutureExt, Sink, Stream, TryFutureExt};
use pin_project::pin_project;
use tokio::{
    sync::oneshot,
    time::{sleep, Duration, Sleep},
};
use tower::{Service, ServiceBuilder};
use tracing::Instrument;
use vector_lib::internal_event::{
    CallError, CountByteSize, EventsSent, InternalEventHandle as _, Output,
};
// === StreamSink<Event> ===
pub use vector_lib::sink::StreamSink;

use super::{
    batch::{Batch, EncodedBatch, FinalizersBatch, PushResult, StatefulBatch},
    buffer::{Partition, PartitionBuffer, PartitionInnerBuffer},
    service::{Map, ServiceBuilderExt},
    EncodedEvent,
};
use crate::event::EventStatus;

// === BatchSink ===

/// A `Sink` interface that wraps a `Service` and a
/// `Batch`.
///
/// Provided a batching scheme, a service and batch settings
/// this type will handle buffering events via the batching scheme
/// and dispatching requests via the service based on either the size
/// of the batch or a batch linger timeout.
///
/// # Acking
///
/// Service based acking will only ack events when all prior request
/// batches have been acked. This means if sequential requests r1, r2,
/// and r3 are dispatched and r2 and r3 complete, all events contained
/// in all requests will not be acked until r1 has completed.
///
/// Note: This has been deprecated, please do not use when creating new Sinks.
#[pin_project]
#[derive(Debug)]
pub struct BatchSink<S, B>
where
    S: Service<B::Output>,
    B: Batch,
{
    #[pin]
    inner: PartitionBatchSink<
        Map<S, PartitionInnerBuffer<B::Output, ()>, B::Output>,
        PartitionBuffer<B, ()>,
        (),
    >,
}

impl<S, B> BatchSink<S, B>
where
    S: Service<B::Output>,
    S::Future: Send + 'static,
    S::Error: Into<crate::Error> + Send + 'static,
    S::Response: Response + Send + 'static,
    B: Batch,
{
    pub fn new(service: S, batch: B, timeout: Duration) -> Self {
        let service = ServiceBuilder::new()
            .map(|req: PartitionInnerBuffer<B::Output, ()>| req.into_parts().0)
            .service(service);
        let batch = PartitionBuffer::new(batch);
        let inner = PartitionBatchSink::new(service, batch, timeout);
        Self { inner }
    }
}

#[cfg(test)]
impl<S, B> BatchSink<S, B>
where
    S: Service<B::Output>,
    B: Batch,
{
    pub const fn get_ref(&self) -> &S {
        &self.inner.service.service.inner
    }
}

impl<S, B> Sink<EncodedEvent<B::Input>> for BatchSink<S, B>
where
    S: Service<B::Output>,
    S::Future: Send + 'static,
    S::Error: Into<crate::Error> + Send + 'static,
    S::Response: Response + Send + 'static,
    B: Batch,
{
    type Error = crate::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_ready(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: EncodedEvent<B::Input>) -> Result<(), Self::Error> {
        self.project()
            .inner
            .start_send(item.map(|item| PartitionInnerBuffer::new(item, ())))
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_close(cx)
    }
}

// === PartitionBatchSink ===

/// A partition based batcher, given some `Service` and `Batch` where the
/// input is partitionable via the `Partition` trait, it will hold many
/// in flight batches.
///
/// This type is similar to `BatchSink` with the added benefit that it has
/// more fine-grained partitioning ability. It will hold many different batches
/// of events and contain linger timeouts for each.
///
/// Note that, unlike `BatchSink`, the `batch` given to this sink is
/// *only* used to create new batches (via `Batch::fresh`) for each new
/// partition.
///
/// # Acking
///
/// Service based acking will only ack events when all prior request
/// batches have been acked. This means if sequential requests r1, r2,
/// and r3 are dispatched and r2 and r3 complete, all events contained
/// in all requests will not be acked until r1 has completed.
///
/// # Ordering
/// Per partition ordering can be achieved by holding onto future of a request
/// until it finishes. Until then all further requests in that partition are
/// delayed.
///
/// Note: This has been deprecated, please do not use when creating new Sinks.
#[pin_project]
pub struct PartitionBatchSink<S, B, K>
where
    B: Batch,
    S: Service<B::Output>,
{
    service: ServiceSink<S, B::Output>,
    buffer: Option<(K, EncodedEvent<B::Input>)>,
    batch: StatefulBatch<FinalizersBatch<B>>,
    partitions: HashMap<K, StatefulBatch<FinalizersBatch<B>>>,
    timeout: Duration,
    lingers: HashMap<K, Pin<Box<Sleep>>>,
    in_flight: Option<HashMap<K, BoxFuture<'static, ()>>>,
    closing: bool,
}

impl<S, B, K> PartitionBatchSink<S, B, K>
where
    B: Batch,
    B::Input: Partition<K>,
    K: Hash + Eq + Clone + Send + 'static,
    S: Service<B::Output>,
    S::Future: Send + 'static,
    S::Error: Into<crate::Error> + Send + 'static,
    S::Response: Response + Send + 'static,
{
    pub fn new(service: S, batch: B, timeout: Duration) -> Self {
        Self {
            service: ServiceSink::new(service),
            buffer: None,
            batch: StatefulBatch::from(FinalizersBatch::from(batch)),
            partitions: HashMap::new(),
            timeout,
            lingers: HashMap::new(),
            in_flight: None,
            closing: false,
        }
    }

    /// Enforces per partition ordering of request.
    pub fn ordered(&mut self) {
        self.in_flight = Some(HashMap::new());
    }
}

impl<S, B, K> Sink<EncodedEvent<B::Input>> for PartitionBatchSink<S, B, K>
where
    B: Batch,
    B::Input: Partition<K>,
    K: Hash + Eq + Clone + Send + 'static,
    S: Service<B::Output>,
    S::Future: Send + 'static,
    S::Error: Into<crate::Error> + Send + 'static,
    S::Response: Response + Send + 'static,
{
    type Error = crate::Error;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        if self.buffer.is_some() {
            match self.as_mut().poll_flush(cx) {
                Poll::Ready(Ok(())) => {}
                Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                Poll::Pending => {
                    if self.buffer.is_some() {
                        return Poll::Pending;
                    }
                }
            }
        }

        Poll::Ready(Ok(()))
    }

    fn start_send(
        mut self: Pin<&mut Self>,
        item: EncodedEvent<B::Input>,
    ) -> Result<(), Self::Error> {
        let partition = item.item.partition();

        let batch = loop {
            if let Some(batch) = self.partitions.get_mut(&partition) {
                break batch;
            }

            let batch = self.batch.fresh();
            self.partitions.insert(partition.clone(), batch);

            let delay = sleep(self.timeout);
            self.lingers.insert(partition.clone(), Box::pin(delay));
        };

        if let PushResult::Overflow(item) = batch.push(item) {
            self.buffer = Some((partition, item));
        }

        Ok(())
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        loop {
            // Poll inner service while not ready, if we don't have buffer or any batch.
            if self.buffer.is_none() && self.partitions.is_empty() {
                ready!(self.service.poll_complete(cx));
                return Poll::Ready(Ok(()));
            }

            // Try send batches.
            let this = self.as_mut().project();
            let mut partitions_ready = vec![];
            for (partition, batch) in this.partitions.iter() {
                if ((*this.closing && !batch.is_empty())
                    || batch.was_full()
                    || matches!(
                        this.lingers
                            .get_mut(partition)
                            .expect("linger should exists for poll_flush")
                            .poll_unpin(cx),
                        Poll::Ready(())
                    ))
                    && this
                        .in_flight
                        .as_mut()
                        .and_then(|map| map.get_mut(partition))
                        .map(|req| matches!(req.poll_unpin(cx), Poll::Ready(())))
                        .unwrap_or(true)
                {
                    partitions_ready.push(partition.clone());
                }
            }
            let mut batch_consumed = false;
            for partition in partitions_ready.iter() {
                let service_ready = match this.service.poll_ready(cx) {
                    Poll::Ready(Ok(())) => true,
                    Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                    Poll::Pending => false,
                };
                if service_ready {
                    trace!("Service ready; Sending batch.");

                    let batch = this.partitions.remove(partition).unwrap();
                    this.lingers.remove(partition);

                    let batch = batch.finish();
                    let future = tokio::spawn(this.service.call(batch));

                    if let Some(map) = this.in_flight.as_mut() {
                        map.insert(partition.clone(), future.map(|_| ()).fuse().boxed());
                    }

                    batch_consumed = true;
                } else {
                    break;
                }
            }
            if batch_consumed {
                continue;
            }

            // Cleanup of in flight futures
            if let Some(in_flight) = this.in_flight.as_mut() {
                if in_flight.len() > this.partitions.len() {
                    // There is at least one in flight future without a partition to check it
                    // so we will do it here.
                    let partitions = this.partitions;
                    in_flight.retain(|partition, req| {
                        partitions.contains_key(partition) || req.poll_unpin(cx).is_pending()
                    });
                }
            }

            // Try move item from buffer to batch.
            if let Some((partition, item)) = self.buffer.take() {
                if self.partitions.contains_key(&partition) {
                    self.buffer = Some((partition, item));
                } else {
                    self.as_mut().start_send(item)?;

                    if self.buffer.is_some() {
                        unreachable!("Empty buffer overflowed.");
                    }

                    continue;
                }
            }

            // Only poll inner service and return `Poll::Pending` anyway.
            ready!(self.service.poll_complete(cx));
            return Poll::Pending;
        }
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        trace!("Closing partition batch sink.");
        self.closing = true;
        self.poll_flush(cx)
    }
}

impl<S, B, K> fmt::Debug for PartitionBatchSink<S, B, K>
where
    S: Service<B::Output> + fmt::Debug,
    B: Batch + fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PartitionBatchSink")
            .field("service", &self.service)
            .field("batch", &self.batch)
            .field("timeout", &self.timeout)
            .finish()
    }
}

// === ServiceSink ===

struct ServiceSink<S, Request> {
    service: S,
    in_flight: FuturesUnordered<oneshot::Receiver<()>>,
    next_request_id: usize,
    _pd: PhantomData<Request>,
}

impl<S, Request> ServiceSink<S, Request>
where
    S: Service<Request>,
    S::Future: Send + 'static,
    S::Error: Into<crate::Error> + Send + 'static,
    S::Response: Response + Send + 'static,
{
    fn new(service: S) -> Self {
        Self {
            service,
            in_flight: FuturesUnordered::new(),
            next_request_id: 0,
            _pd: PhantomData,
        }
    }

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<crate::Result<()>> {
        self.service.poll_ready(cx).map_err(Into::into)
    }

    fn call(&mut self, batch: EncodedBatch<Request>) -> BoxFuture<'static, ()> {
        let EncodedBatch {
            items,
            finalizers,
            count,
            json_byte_size,
            ..
        } = batch;

        let (tx, rx) = oneshot::channel();

        self.in_flight.push(rx);

        let request_id = self.next_request_id;
        self.next_request_id = request_id.wrapping_add(1);

        trace!(
            message = "Submitting service request.",
            in_flight_requests = self.in_flight.len()
        );
        let events_sent = register!(EventsSent::from(Output(None)));
        self.service
            .call(items)
            .err_into()
            .map(move |result| {
                let status = result_status(&result);
                finalizers.update_status(status);
                match status {
                    EventStatus::Delivered => {
                        events_sent.emit(CountByteSize(count, json_byte_size));
                        // TODO: Emit a BytesSent event here too
                    }
                    EventStatus::Rejected => {
                        // Emit the `Error` and `EventsDropped` internal events.
                        // This scenario occurs after retries have been attempted.
                        let error = result.err().unwrap_or_else(|| "Response failed.".into());
                        emit!(CallError {
                            error,
                            request_id,
                            count,
                        });
                    }
                    _ => {} // do nothing
                }

                // If the rx end is dropped we still completed
                // the request so this is a weird case that we can
                // ignore for now.
                _ = tx.send(());
            })
            .instrument(info_span!("request", %request_id).or_current())
            .boxed()
    }

    fn poll_complete(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        while !self.in_flight.is_empty() {
            match ready!(Pin::new(&mut self.in_flight).poll_next(cx)) {
                Some(Ok(())) => {}
                Some(Err(_)) => panic!("ServiceSink service sender dropped."),
                None => break,
            }
        }

        Poll::Ready(())
    }
}

impl<S, Request> fmt::Debug for ServiceSink<S, Request>
where
    S: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ServiceSink")
            .field("service", &self.service)
            .finish()
    }
}

// === Response ===

pub trait ServiceLogic: Clone {
    type Response: Response;
    fn result_status(&self, result: &crate::Result<Self::Response>) -> EventStatus;
}

#[derive(Derivative)]
#[derivative(Clone)]
pub struct StdServiceLogic<R> {
    _pd: PhantomData<R>,
}

impl<R> Default for StdServiceLogic<R> {
    fn default() -> Self {
        Self { _pd: PhantomData }
    }
}

impl<R> ServiceLogic for StdServiceLogic<R>
where
    R: Response + Send,
{
    type Response = R;

    fn result_status(&self, result: &crate::Result<Self::Response>) -> EventStatus {
        result_status(result)
    }
}

fn result_status<R: Response + Send>(result: &crate::Result<R>) -> EventStatus {
    match result {
        Ok(response) => {
            if response.is_successful() {
                trace!(message = "Response successful.", ?response);
                EventStatus::Delivered
            } else if response.is_transient() {
                error!(message = "Response wasn't successful.", ?response);
                EventStatus::Errored
            } else {
                error!(message = "Response failed.", ?response);
                EventStatus::Rejected
            }
        }
        Err(error) => {
            error!(message = "Request failed.", %error);
            EventStatus::Errored
        }
    }
}

// === Response ===

pub trait Response: fmt::Debug {
    fn is_successful(&self) -> bool {
        true
    }

    fn is_transient(&self) -> bool {
        true
    }
}

impl Response for () {}

impl<'a> Response for &'a str {}

#[cfg(test)]
mod tests {
    use std::{
        convert::Infallible,
        sync::{atomic::AtomicUsize, atomic::Ordering::Relaxed, Arc, Mutex},
    };

    use bytes::Bytes;
    use futures::{future, stream, task::noop_waker_ref, SinkExt, StreamExt};
    use tokio::{task::yield_now, time::Instant};
    use vector_lib::{
        finalization::{BatchNotifier, BatchStatus, EventFinalizer, EventFinalizers},
        json_size::JsonSize,
    };

    use super::*;
    use crate::{
        sinks::util::{BatchSettings, EncodedLength, VecBuffer},
        test_util::trace_init,
    };

    const TIMEOUT: Duration = Duration::from_secs(10);

    impl EncodedLength for usize {
        fn encoded_length(&self) -> usize {
            22
        }
    }

    async fn advance_time(duration: Duration) {
        tokio::time::pause();
        tokio::time::advance(duration).await;
        tokio::time::resume();
    }

    type Counter = Arc<AtomicUsize>;

    struct Request(usize, EventFinalizers);

    impl Request {
        fn new(value: usize, counter: &Counter) -> Self {
            let (batch, receiver) = BatchNotifier::new_with_receiver();
            let counter = Arc::clone(counter);
            tokio::spawn(async move {
                if receiver.await == BatchStatus::Delivered {
                    counter.fetch_add(value, Relaxed);
                }
            });
            Self(value, EventFinalizers::new(EventFinalizer::new(batch)))
        }

        fn encoded(value: usize, counter: &Counter) -> EncodedEvent<Self> {
            let mut item = Self::new(value, counter);
            let finalizers = std::mem::take(&mut item.1);
            EncodedEvent {
                item,
                finalizers,
                json_byte_size: JsonSize::zero(),
                byte_size: 0,
            }
        }
    }

    impl EncodedLength for Request {
        fn encoded_length(&self) -> usize {
            22
        }
    }

    #[tokio::test]
    async fn batch_sink_acking_sequential() {
        let ack_counter = Counter::default();

        let svc = tower::service_fn(|_| future::ok::<_, std::io::Error>(()));
        let mut batch_settings = BatchSettings::default();
        batch_settings.size.bytes = 9999;
        batch_settings.size.events = 10;

        let buffered = BatchSink::new(svc, VecBuffer::new(batch_settings.size), TIMEOUT);

        buffered
            .sink_map_err(drop)
            .send_all(
                &mut stream::iter(1..=22).map(|item| Ok(Request::encoded(item, &ack_counter))),
            )
            .await
            .unwrap();

        assert_eq!(ack_counter.load(Relaxed), 22 * 23 / 2);
    }

    #[tokio::test]
    async fn batch_sink_acking_unordered() {
        let ack_counter = Counter::default();

        trace_init();

        // Services future will be spawned and work between `yield_now` calls.
        let svc = tower::service_fn(|req: Vec<Request>| async move {
            let duration = match req[0].0 {
                1..=3 => Duration::from_secs(1),

                // The 4th request will introduce some sort of
                // latency spike to ensure later events don't
                // get acked.
                4 => Duration::from_secs(5),
                5 | 6 => Duration::from_secs(1),
                _ => unreachable!(),
            };

            sleep(duration).await;
            Ok::<(), Infallible>(())
        });

        let mut batch_settings = BatchSettings::default();
        batch_settings.size.bytes = 9999;
        batch_settings.size.events = 1;

        let mut sink = BatchSink::new(svc, VecBuffer::new(batch_settings.size), TIMEOUT);

        let mut cx = Context::from_waker(noop_waker_ref());
        for item in 1..=3 {
            assert!(matches!(
                sink.poll_ready_unpin(&mut cx),
                Poll::Ready(Ok(()))
            ));
            assert!(matches!(
                sink.start_send_unpin(Request::encoded(item, &ack_counter)),
                Ok(())
            ));
        }

        // Clear internal buffer
        assert!(matches!(sink.poll_flush_unpin(&mut cx), Poll::Pending));
        assert_eq!(ack_counter.load(Relaxed), 0);

        yield_now().await;
        advance_time(Duration::from_secs(3)).await;
        yield_now().await;

        for _ in 1..=3 {
            assert!(matches!(
                sink.poll_flush_unpin(&mut cx),
                Poll::Ready(Ok(()))
            ));
        }

        // Events 1,2,3 should have been acked at this point.
        assert_eq!(ack_counter.load(Relaxed), 6);

        for item in 4..=6 {
            assert!(matches!(
                sink.poll_ready_unpin(&mut cx),
                Poll::Ready(Ok(()))
            ));
            assert!(matches!(
                sink.start_send_unpin(Request::encoded(item, &ack_counter)),
                Ok(())
            ));
        }

        // Clear internal buffer
        assert!(matches!(sink.poll_flush_unpin(&mut cx), Poll::Pending));
        assert_eq!(ack_counter.load(Relaxed), 6);

        yield_now().await;
        advance_time(Duration::from_secs(2)).await;
        yield_now().await;

        assert!(matches!(sink.poll_flush_unpin(&mut cx), Poll::Pending));

        // Check that events 1-3,5,6 have been acked
        assert_eq!(ack_counter.load(Relaxed), 17);

        yield_now().await;
        advance_time(Duration::from_secs(5)).await;
        yield_now().await;

        for _ in 4..=6 {
            assert!(matches!(
                sink.poll_flush_unpin(&mut cx),
                Poll::Ready(Ok(()))
            ));
        }

        assert_eq!(ack_counter.load(Relaxed), 21);
    }

    #[tokio::test]
    async fn batch_sink_buffers_messages_until_limit() {
        let sent_requests = Arc::new(Mutex::new(Vec::new()));

        let svc = tower::service_fn(|req| {
            let sent_requests = Arc::clone(&sent_requests);

            sent_requests.lock().unwrap().push(req);

            future::ok::<_, std::io::Error>(())
        });

        let mut batch_settings = BatchSettings::default();
        batch_settings.size.bytes = 9999;
        batch_settings.size.events = 10;
        let buffered = BatchSink::new(svc, VecBuffer::new(batch_settings.size), TIMEOUT);

        buffered
            .sink_map_err(drop)
            .send_all(
                &mut stream::iter(0..22)
                    .map(|item| Ok(EncodedEvent::new(item, 0, JsonSize::zero()))),
            )
            .await
            .unwrap();

        let output = sent_requests.lock().unwrap();
        assert_eq!(
            &*output,
            &vec![
                vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
                vec![10, 11, 12, 13, 14, 15, 16, 17, 18, 19],
                vec![20, 21]
            ]
        );
    }

    #[tokio::test]
    async fn batch_sink_flushes_below_min_on_close() {
        let sent_requests = Arc::new(Mutex::new(Vec::new()));

        let svc = tower::service_fn(|req| {
            let sent_requests = Arc::clone(&sent_requests);
            sent_requests.lock().unwrap().push(req);
            future::ok::<_, std::io::Error>(())
        });

        let mut batch_settings = BatchSettings::default();
        batch_settings.size.bytes = 9999;
        batch_settings.size.events = 10;
        let mut buffered = BatchSink::new(svc, VecBuffer::new(batch_settings.size), TIMEOUT);

        let mut cx = Context::from_waker(noop_waker_ref());
        assert!(matches!(
            buffered.poll_ready_unpin(&mut cx),
            Poll::Ready(Ok(()))
        ));
        assert!(matches!(
            buffered.start_send_unpin(EncodedEvent::new(0, 0, JsonSize::zero())),
            Ok(())
        ));
        assert!(matches!(
            buffered.poll_ready_unpin(&mut cx),
            Poll::Ready(Ok(()))
        ));
        assert!(matches!(
            buffered.start_send_unpin(EncodedEvent::new(1, 0, JsonSize::zero())),
            Ok(())
        ));

        buffered.close().await.unwrap();

        let output = sent_requests.lock().unwrap();
        assert_eq!(&*output, &vec![vec![0, 1]]);
    }

    #[tokio::test]
    async fn batch_sink_expired_linger() {
        let sent_requests = Arc::new(Mutex::new(Vec::new()));

        let svc = tower::service_fn(|req| {
            let sent_requests = Arc::clone(&sent_requests);
            sent_requests.lock().unwrap().push(req);
            future::ok::<_, std::io::Error>(())
        });

        let mut batch_settings = BatchSettings::default();
        batch_settings.size.bytes = 9999;
        batch_settings.size.events = 10;
        let mut buffered = BatchSink::new(svc, VecBuffer::new(batch_settings.size), TIMEOUT);

        let mut cx = Context::from_waker(noop_waker_ref());
        assert!(matches!(
            buffered.poll_ready_unpin(&mut cx),
            Poll::Ready(Ok(()))
        ));
        assert!(matches!(
            buffered.start_send_unpin(EncodedEvent::new(0, 0, JsonSize::zero())),
            Ok(())
        ));
        assert!(matches!(
            buffered.poll_ready_unpin(&mut cx),
            Poll::Ready(Ok(()))
        ));
        assert!(matches!(
            buffered.start_send_unpin(EncodedEvent::new(1, 0, JsonSize::zero())),
            Ok(())
        ));

        // Move clock forward by linger timeout + 1 sec
        advance_time(TIMEOUT + Duration::from_secs(1)).await;

        // Flush buffer and make sure that this didn't take long time (because linger elapsed).
        let start = Instant::now();
        buffered.flush().await.unwrap();
        let elapsed = start.duration_since(start);
        assert!(elapsed < Duration::from_millis(200));

        let output = sent_requests.lock().unwrap();
        assert_eq!(&*output, &vec![vec![0, 1]]);
    }

    #[tokio::test]
    async fn partition_batch_sink_buffers_messages_until_limit() {
        let sent_requests = Arc::new(Mutex::new(Vec::new()));

        let svc = tower::service_fn(|req| {
            let sent_requests = Arc::clone(&sent_requests);
            sent_requests.lock().unwrap().push(req);
            future::ok::<_, std::io::Error>(())
        });

        let mut batch_settings = BatchSettings::default();
        batch_settings.size.bytes = 9999;
        batch_settings.size.events = 10;

        let sink = PartitionBatchSink::new(svc, VecBuffer::new(batch_settings.size), TIMEOUT);

        sink.sink_map_err(drop)
            .send_all(
                &mut stream::iter(0..22)
                    .map(|item| Ok(EncodedEvent::new(item, 0, JsonSize::zero()))),
            )
            .await
            .unwrap();

        let output = sent_requests.lock().unwrap();
        assert_eq!(
            &*output,
            &vec![
                vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
                vec![10, 11, 12, 13, 14, 15, 16, 17, 18, 19],
                vec![20, 21]
            ]
        );
    }

    #[tokio::test]
    async fn partition_batch_sink_buffers_by_partition_buffer_size_one() {
        let sent_requests = Arc::new(Mutex::new(Vec::new()));

        let svc = tower::service_fn(|req| {
            let sent_requests = Arc::clone(&sent_requests);
            sent_requests.lock().unwrap().push(req);
            future::ok::<_, std::io::Error>(())
        });

        let mut batch_settings = BatchSettings::default();
        batch_settings.size.bytes = 9999;
        batch_settings.size.events = 1;

        let sink = PartitionBatchSink::new(svc, VecBuffer::new(batch_settings.size), TIMEOUT);

        let input = vec![Partitions::A, Partitions::B];
        sink.sink_map_err(drop)
            .send_all(
                &mut stream::iter(input)
                    .map(|item| Ok(EncodedEvent::new(item, 0, JsonSize::zero()))),
            )
            .await
            .unwrap();

        let mut output = sent_requests.lock().unwrap();
        output[..].sort();
        assert_eq!(&*output, &vec![vec![Partitions::A], vec![Partitions::B]]);
    }

    #[tokio::test]
    async fn partition_batch_sink_buffers_by_partition_buffer_size_two() {
        let sent_requests = Arc::new(Mutex::new(Vec::new()));

        let svc = tower::service_fn(|req| {
            let sent_requests = Arc::clone(&sent_requests);
            sent_requests.lock().unwrap().push(req);
            future::ok::<_, std::io::Error>(())
        });

        let mut batch_settings = BatchSettings::default();
        batch_settings.size.bytes = 9999;
        batch_settings.size.events = 2;

        let sink = PartitionBatchSink::new(svc, VecBuffer::new(batch_settings.size), TIMEOUT);

        let input = vec![Partitions::A, Partitions::B, Partitions::A, Partitions::B];
        sink.sink_map_err(drop)
            .send_all(
                &mut stream::iter(input)
                    .map(|item| Ok(EncodedEvent::new(item, 0, JsonSize::zero()))),
            )
            .await
            .unwrap();

        let mut output = sent_requests.lock().unwrap();
        output[..].sort();
        assert_eq!(
            &*output,
            &vec![
                vec![Partitions::A, Partitions::A],
                vec![Partitions::B, Partitions::B]
            ]
        );
    }

    #[tokio::test]
    async fn partition_batch_sink_submits_after_linger() {
        let sent_requests = Arc::new(Mutex::new(Vec::new()));

        let svc = tower::service_fn(|req| {
            let sent_requests = Arc::clone(&sent_requests);
            sent_requests.lock().unwrap().push(req);
            future::ok::<_, std::io::Error>(())
        });

        let mut batch_settings = BatchSettings::default();
        batch_settings.size.bytes = 9999;
        batch_settings.size.events = 10;

        let mut sink = PartitionBatchSink::new(svc, VecBuffer::new(batch_settings.size), TIMEOUT);

        let mut cx = Context::from_waker(noop_waker_ref());
        assert!(matches!(
            sink.poll_ready_unpin(&mut cx),
            Poll::Ready(Ok(()))
        ));
        assert!(matches!(
            sink.start_send_unpin(EncodedEvent::new(1, 0, JsonSize::zero())),
            Ok(())
        ));
        assert!(matches!(sink.poll_flush_unpin(&mut cx), Poll::Pending));

        advance_time(TIMEOUT + Duration::from_secs(1)).await;

        let start = Instant::now();
        sink.flush().await.unwrap();
        let elapsed = start.duration_since(start);
        assert!(elapsed < Duration::from_millis(200));

        let output = sent_requests.lock().unwrap();
        assert_eq!(&*output, &vec![vec![1]]);
    }

    #[tokio::test]
    async fn service_sink_doesnt_propagate_error() {
        let ack_counter = Counter::default();

        // We need a mock executor here because we need to ensure
        // that we poll the service futures within the mock clock
        // context. This allows us to manually advance the time on the
        // "spawned" futures.
        let svc = tower::service_fn(|req: Request| {
            if req.0 == 3 {
                future::err("bad")
            } else {
                future::ok("good")
            }
        });
        let mut sink = ServiceSink::new(svc);
        let req = |items: usize| {
            let mut req = Request::new(items, &ack_counter);
            let finalizers = std::mem::take(&mut req.1);
            EncodedBatch {
                items: req,
                finalizers,
                count: items,
                byte_size: 1,
                json_byte_size: JsonSize::new(1),
            }
        };

        // send some initial requests
        let mut fut1 = sink.call(req(1));
        let mut fut2 = sink.call(req(2));

        assert_eq!(ack_counter.load(Relaxed), 0);

        let mut cx = Context::from_waker(noop_waker_ref());
        assert!(matches!(fut1.poll_unpin(&mut cx), Poll::Ready(())));
        assert!(matches!(fut2.poll_unpin(&mut cx), Poll::Ready(())));
        assert!(matches!(sink.poll_complete(&mut cx), Poll::Ready(())));

        yield_now().await;
        assert_eq!(ack_counter.load(Relaxed), 3);

        // send one request that will error and one normal
        let mut fut3 = sink.call(req(3)); // I will error
        let mut fut4 = sink.call(req(4));

        // make sure they all "worked"
        assert!(matches!(fut3.poll_unpin(&mut cx), Poll::Ready(())));
        assert!(matches!(fut4.poll_unpin(&mut cx), Poll::Ready(())));
        assert!(matches!(sink.poll_complete(&mut cx), Poll::Ready(())));

        yield_now().await;
        assert_eq!(ack_counter.load(Relaxed), 7);
    }

    #[tokio::test]
    async fn partition_batch_sink_ordering_per_partition() {
        let sent_requests = Arc::new(Mutex::new(Vec::new()));

        let mut delay = true;
        let svc = tower::service_fn(|req| {
            let sent_requests = Arc::clone(&sent_requests);
            if delay {
                // Delay and then error
                delay = false;
                sleep(Duration::from_secs(1))
                    .map(move |_| {
                        sent_requests.lock().unwrap().push(req);
                        Result::<_, std::io::Error>::Ok(())
                    })
                    .boxed()
            } else {
                sent_requests.lock().unwrap().push(req);
                future::ok::<_, std::io::Error>(()).boxed()
            }
        });

        let mut batch_settings = BatchSettings::default();
        batch_settings.size.bytes = 9999;
        batch_settings.size.events = 10;

        let mut sink = PartitionBatchSink::new(svc, VecBuffer::new(batch_settings.size), TIMEOUT);
        sink.ordered();

        let input = (0..20).map(|i| (0, i)).chain((0..20).map(|i| (1, i)));
        sink.sink_map_err(drop)
            .send_all(
                &mut stream::iter(input)
                    .map(|item| Ok(EncodedEvent::new(item, 0, JsonSize::zero()))),
            )
            .await
            .unwrap();

        let output = sent_requests.lock().unwrap();
        // We sended '0' partition first and delayed sending only first request, first 10 events,
        // which should delay sending the second batch of events in the same partition until
        // the first one succeeds.
        assert_eq!(
            &*output,
            &vec![
                (0..10).map(|i| (1, i)).collect::<Vec<_>>(),
                (10..20).map(|i| (1, i)).collect(),
                (0..10).map(|i| (0, i)).collect(),
                (10..20).map(|i| (0, i)).collect(),
            ]
        );
    }

    #[derive(Debug, PartialEq, Eq, Ord, PartialOrd)]
    enum Partitions {
        A,
        B,
    }

    impl EncodedLength for Partitions {
        fn encoded_length(&self) -> usize {
            10 // Dummy value
        }
    }

    impl Partition<Bytes> for Partitions {
        fn partition(&self) -> Bytes {
            format!("{:?}", self).into()
        }
    }

    impl Partition<Bytes> for usize {
        fn partition(&self) -> Bytes {
            "key".into()
        }
    }

    impl Partition<Bytes> for (usize, usize) {
        fn partition(&self) -> Bytes {
            self.0.to_string().into()
        }
    }

    impl EncodedLength for (usize, usize) {
        fn encoded_length(&self) -> usize {
            16
        }
    }
}
