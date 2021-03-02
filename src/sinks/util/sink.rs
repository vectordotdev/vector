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
//! are similar in the sense that they both take some `tower::Service`, `Batch` and
//! `Acker` and will provide full batching, request dispatching and acking based on
//! the settings passed.
//!
//! For more advanced use cases like HTTP based sinks, one should use the
//! `BatchedHttpSink` type, which is a wrapper for `BatchSink` and `HttpSink`.
//!
//! # Driving to completetion
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

use super::{
    batch::{Batch, PushResult, StatefulBatch},
    buffer::{Partition, PartitionBuffer, PartitionInnerBuffer},
    service::{Map, ServiceBuilderExt},
};
use crate::{buffers::Acker, Event};
use async_trait::async_trait;
use futures::{
    future::BoxFuture,
    ready,
    stream::{BoxStream, FuturesUnordered},
    FutureExt, Sink, Stream, TryFutureExt,
};
use pin_project::pin_project;
use std::{
    collections::HashMap,
    fmt,
    hash::Hash,
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::{
    sync::oneshot,
    time::{delay_for, Delay, Duration},
};
use tower::{Service, ServiceBuilder};
use tracing_futures::Instrument;

// === StreamSink ===

#[async_trait]
pub trait StreamSink {
    async fn run(&mut self, input: BoxStream<'_, Event>) -> Result<(), ()>;
}

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
#[pin_project]
#[derive(Debug)]
pub struct BatchSink<S, B, Request>
where
    B: Batch<Output = Request>,
{
    #[pin]
    inner: PartitionBatchSink<
        Map<S, PartitionInnerBuffer<Request, ()>, Request>,
        PartitionBuffer<B, ()>,
        (),
        PartitionInnerBuffer<Request, ()>,
    >,
}

impl<S, B, Request> BatchSink<S, B, Request>
where
    S: Service<Request>,
    S::Future: Send + 'static,
    S::Error: Into<crate::Error> + Send + 'static,
    S::Response: Response,
    B: Batch<Output = Request>,
{
    pub fn new(service: S, batch: B, timeout: Duration, acker: Acker) -> Self {
        let service = ServiceBuilder::new()
            .map(|req: PartitionInnerBuffer<Request, ()>| req.into_parts().0)
            .service(service);
        let batch = PartitionBuffer::new(batch);
        let inner = PartitionBatchSink::new(service, batch, timeout, acker);
        Self { inner }
    }
}

#[cfg(test)]
impl<S, B, Request> BatchSink<S, B, Request>
where
    B: Batch<Output = Request>,
{
    pub fn get_ref(&self) -> &S {
        &self.inner.service.service.inner
    }
}

impl<S, B, Request> Sink<B::Input> for BatchSink<S, B, Request>
where
    S: Service<Request>,
    S::Future: Send + 'static,
    S::Error: Into<crate::Error> + Send + 'static,
    S::Response: Response,
    B: Batch<Output = Request>,
{
    type Error = crate::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_ready(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: B::Input) -> Result<(), Self::Error> {
        let item = PartitionInnerBuffer::new(item, ());
        self.project().inner.start_send(item)
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
/// more fine grained partitioning ability. It will hold many different batches
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
#[pin_project]
pub struct PartitionBatchSink<S, B, K, Request>
where
    B: Batch<Output = Request>,
{
    service: ServiceSink<S, Request>,
    buffer: Option<(K, B::Input)>,
    batch: StatefulBatch<B>,
    partitions: HashMap<K, StatefulBatch<B>>,
    timeout: Duration,
    lingers: HashMap<K, Delay>,
    closing: bool,
}

impl<S, B, K, Request> PartitionBatchSink<S, B, K, Request>
where
    B: Batch<Output = Request>,
    B::Input: Partition<K>,
    K: Hash + Eq + Clone + Send + 'static,
    S: Service<Request>,
    S::Future: Send + 'static,
    S::Error: Into<crate::Error> + Send + 'static,
    S::Response: Response,
{
    pub fn new(service: S, batch: B, timeout: Duration, acker: Acker) -> Self {
        let service = ServiceSink::new(service, acker);

        Self {
            service,
            buffer: None,
            batch: batch.into(),
            partitions: HashMap::new(),
            timeout,
            lingers: HashMap::new(),
            closing: false,
        }
    }
}

impl<S, B, K, Request> Sink<B::Input> for PartitionBatchSink<S, B, K, Request>
where
    B: Batch<Output = Request>,
    B::Input: Partition<K>,
    K: Hash + Eq + Clone + Send + 'static,
    S: Service<Request>,
    S::Future: Send + 'static,
    S::Error: Into<crate::Error> + Send + 'static,
    S::Response: Response,
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

    fn start_send(mut self: Pin<&mut Self>, item: B::Input) -> Result<(), Self::Error> {
        let partition = item.partition();

        let batch = loop {
            if let Some(batch) = self.partitions.get_mut(&partition) {
                break batch;
            }

            let batch = self.batch.fresh();
            self.partitions.insert(partition.clone(), batch);

            let delay = delay_for(self.timeout);
            self.lingers.insert(partition.clone(), delay);
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
                if (*this.closing && !batch.is_empty())
                    || batch.was_full()
                    || matches!(
                        this.lingers
                            .get_mut(&partition)
                            .expect("linger should exists for poll_flush")
                            .poll_unpin(cx),
                        Poll::Ready(())
                    )
                {
                    partitions_ready.push(partition.clone());
                }
            }
            let mut batch_consumed = false;
            for partition in partitions_ready.iter() {
                let service_ready = match self.service.poll_ready(cx) {
                    Poll::Ready(Ok(())) => true,
                    Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                    Poll::Pending => false,
                };
                if service_ready {
                    trace!("Service ready; Sending batch.");

                    let batch = self.partitions.remove(&partition).unwrap();
                    self.lingers.remove(&partition);

                    let batch_size = batch.num_items();
                    let request = batch.finish();
                    tokio::spawn(self.service.call(request, batch_size));

                    batch_consumed = true;
                } else {
                    break;
                }
            }
            if batch_consumed {
                continue;
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

impl<S, B, K, Request> fmt::Debug for PartitionBatchSink<S, B, K, Request>
where
    S: fmt::Debug,
    B: Batch<Output = Request> + fmt::Debug,
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
    in_flight: FuturesUnordered<oneshot::Receiver<(usize, usize)>>,
    acker: Acker,
    seq_head: usize,
    seq_tail: usize,
    pending_acks: HashMap<usize, usize>,
    next_request_id: usize,
    _pd: PhantomData<Request>,
}

impl<S, Request> ServiceSink<S, Request>
where
    S: Service<Request>,
    S::Future: Send + 'static,
    S::Error: Into<crate::Error> + Send + 'static,
    S::Response: Response,
{
    fn new(service: S, acker: Acker) -> Self {
        Self {
            service,
            in_flight: FuturesUnordered::new(),
            acker,
            seq_head: 0,
            seq_tail: 0,
            pending_acks: HashMap::new(),
            next_request_id: 0,
            _pd: PhantomData,
        }
    }

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<crate::Result<()>> {
        self.service.poll_ready(cx).map_err(Into::into)
    }

    fn call(&mut self, req: Request, batch_size: usize) -> BoxFuture<'static, ()> {
        let seqno = self.seq_head;
        self.seq_head += 1;

        let (tx, rx) = oneshot::channel();

        self.in_flight.push(rx);

        let request_id = self.next_request_id;
        self.next_request_id = request_id.wrapping_add(1);

        trace!(
            message = "Submitting service request.",
            in_flight_requests = self.in_flight.len()
        );
        self.service
            .call(req)
            .err_into()
            .map(move |result| {
                match result {
                    Ok(response) if response.is_successful() => {
                        trace!(message = "Response successful.", ?response);
                    }
                    Ok(response) => {
                        error!(message = "Response wasn't successful.", ?response);
                    }
                    Err(error) => {
                        error!(message = "Request failed.", %error);
                    }
                }

                // If the rx end is dropped we still completed
                // the request so this is a weird case that we can
                // ignore for now.
                let _ = tx.send((seqno, batch_size));
            })
            .instrument(info_span!("request", %request_id))
            .boxed()
    }

    fn poll_complete(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        while !self.in_flight.is_empty() {
            match ready!(Pin::new(&mut self.in_flight).poll_next(cx)) {
                Some(Ok((seqno, batch_size))) => {
                    self.pending_acks.insert(seqno, batch_size);

                    let mut num_to_ack = 0;
                    while let Some(ack_size) = self.pending_acks.remove(&self.seq_tail) {
                        num_to_ack += ack_size;
                        self.seq_tail += 1
                    }
                    trace!(message = "Acking events.", acking_num = num_to_ack);
                    self.acker.ack(num_to_ack);
                }
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
            .field("acker", &self.acker)
            .field("seq_head", &self.seq_head)
            .field("seq_tail", &self.seq_tail)
            .field("pending_acks", &self.pending_acks)
            .finish()
    }
}

// === Response ===

pub trait Response: fmt::Debug {
    fn is_successful(&self) -> bool {
        true
    }
}

impl Response for () {}
impl<'a> Response for &'a str {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        buffers::Acker,
        sinks::util::{BatchSettings, EncodedLength, VecBuffer},
        test_util::trace_init,
    };
    use bytes::Bytes;
    use futures::{future, stream, task::noop_waker_ref, SinkExt, StreamExt};
    use std::{
        convert::Infallible,
        sync::{atomic::Ordering::Relaxed, Arc, Mutex},
    };
    use tokio::{task::yield_now, time::Instant};

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

    #[tokio::test]
    async fn batch_sink_acking_sequential() {
        let (acker, ack_counter) = Acker::new_for_testing();

        let svc = tower::service_fn(|_| future::ok::<_, std::io::Error>(()));
        let batch = BatchSettings::default().events(10).bytes(9999);
        let buffered = BatchSink::new(svc, VecBuffer::new(batch.size), TIMEOUT, acker);

        let _ = buffered
            .sink_map_err(drop)
            .send_all(&mut stream::iter(0..22).map(Ok))
            .await
            .unwrap();

        assert_eq!(ack_counter.load(Relaxed), 22);
    }

    #[tokio::test]
    async fn batch_sink_acking_unordered() {
        trace_init();

        // Services future will be spawned and work between `yield_now` calls.
        let (acker, ack_counter) = Acker::new_for_testing();

        let svc = tower::service_fn(|req: Vec<usize>| async move {
            let duration = match req[0] {
                0 => Duration::from_secs(1),
                1 => Duration::from_secs(1),
                2 => Duration::from_secs(1),

                // The 4th request will introduce some sort of
                // latency spike to ensure later events don't
                // get acked.
                3 => Duration::from_secs(5),
                4 => Duration::from_secs(1),
                5 => Duration::from_secs(1),
                _ => unreachable!(),
            };

            delay_for(duration).await;
            Ok::<(), Infallible>(())
        });

        let batch = BatchSettings::default().bytes(9999).events(1);

        let mut sink = BatchSink::new(svc, VecBuffer::new(batch.size), TIMEOUT, acker);

        let mut cx = Context::from_waker(noop_waker_ref());
        assert!(matches!(
            sink.poll_ready_unpin(&mut cx),
            Poll::Ready(Ok(()))
        ));
        assert!(matches!(sink.start_send_unpin(0), Ok(())));
        assert!(matches!(
            sink.poll_ready_unpin(&mut cx),
            Poll::Ready(Ok(()))
        ));
        assert!(matches!(sink.start_send_unpin(1), Ok(())));
        assert!(matches!(
            sink.poll_ready_unpin(&mut cx),
            Poll::Ready(Ok(()))
        ));
        assert!(matches!(sink.start_send_unpin(2), Ok(())));

        // Clear internal buffer
        assert!(matches!(sink.poll_flush_unpin(&mut cx), Poll::Pending));
        assert_eq!(ack_counter.load(Relaxed), 0);

        yield_now().await;
        advance_time(Duration::from_secs(3)).await;
        yield_now().await;

        assert!(matches!(
            sink.poll_flush_unpin(&mut cx),
            Poll::Ready(Ok(()))
        ));
        assert!(matches!(
            sink.poll_flush_unpin(&mut cx),
            Poll::Ready(Ok(()))
        ));
        assert!(matches!(
            sink.poll_flush_unpin(&mut cx),
            Poll::Ready(Ok(()))
        ));

        assert_eq!(ack_counter.load(Relaxed), 3);

        assert!(matches!(
            sink.poll_ready_unpin(&mut cx),
            Poll::Ready(Ok(()))
        ));
        assert!(matches!(sink.start_send_unpin(3), Ok(())));
        assert!(matches!(
            sink.poll_ready_unpin(&mut cx),
            Poll::Ready(Ok(()))
        ));
        assert!(matches!(sink.start_send_unpin(4), Ok(())));
        assert!(matches!(
            sink.poll_ready_unpin(&mut cx),
            Poll::Ready(Ok(()))
        ));
        assert!(matches!(sink.start_send_unpin(5), Ok(())));

        // Clear internal buffer
        assert!(matches!(sink.poll_flush_unpin(&mut cx), Poll::Pending));
        assert_eq!(ack_counter.load(Relaxed), 3);

        yield_now().await;
        advance_time(Duration::from_secs(2)).await;
        yield_now().await;

        assert!(matches!(sink.poll_flush_unpin(&mut cx), Poll::Pending));

        // Check that events 3,4,5 have not been acked yet
        // only the three previous should be acked.
        assert_eq!(ack_counter.load(Relaxed), 3);

        yield_now().await;
        advance_time(Duration::from_secs(5)).await;
        yield_now().await;

        assert!(matches!(
            sink.poll_flush_unpin(&mut cx),
            Poll::Ready(Ok(()))
        ));
        assert!(matches!(
            sink.poll_flush_unpin(&mut cx),
            Poll::Ready(Ok(()))
        ));
        assert!(matches!(
            sink.poll_flush_unpin(&mut cx),
            Poll::Ready(Ok(()))
        ));

        assert_eq!(ack_counter.load(Relaxed), 6);
    }

    #[tokio::test]
    async fn batch_sink_buffers_messages_until_limit() {
        let (acker, _) = Acker::new_for_testing();
        let sent_requests = Arc::new(Mutex::new(Vec::new()));

        let svc = tower::service_fn(|req| {
            let sent_requests = Arc::clone(&sent_requests);

            sent_requests.lock().unwrap().push(req);

            future::ok::<_, std::io::Error>(())
        });
        let batch = BatchSettings::default().bytes(9999).events(10);
        let buffered = BatchSink::new(svc, VecBuffer::new(batch.size), TIMEOUT, acker);

        let _ = buffered
            .sink_map_err(drop)
            .send_all(&mut stream::iter(0..22).map(Ok))
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
        let (acker, _) = Acker::new_for_testing();
        let sent_requests = Arc::new(Mutex::new(Vec::new()));

        let svc = tower::service_fn(|req| {
            let sent_requests = Arc::clone(&sent_requests);
            sent_requests.lock().unwrap().push(req);
            future::ok::<_, std::io::Error>(())
        });

        let batch = BatchSettings::default().bytes(9999).events(10);
        let mut buffered = BatchSink::new(svc, VecBuffer::new(batch.size), TIMEOUT, acker);

        let mut cx = Context::from_waker(noop_waker_ref());
        assert!(matches!(
            buffered.poll_ready_unpin(&mut cx),
            Poll::Ready(Ok(()))
        ));
        assert!(matches!(buffered.start_send_unpin(0), Ok(())));
        assert!(matches!(
            buffered.poll_ready_unpin(&mut cx),
            Poll::Ready(Ok(()))
        ));
        assert!(matches!(buffered.start_send_unpin(1), Ok(())));

        buffered.close().await.unwrap();

        let output = sent_requests.lock().unwrap();
        assert_eq!(&*output, &vec![vec![0, 1]]);
    }

    #[tokio::test]
    async fn batch_sink_expired_linger() {
        let (acker, _) = Acker::new_for_testing();
        let sent_requests = Arc::new(Mutex::new(Vec::new()));

        let svc = tower::service_fn(|req| {
            let sent_requests = Arc::clone(&sent_requests);
            sent_requests.lock().unwrap().push(req);
            future::ok::<_, std::io::Error>(())
        });

        let batch = BatchSettings::default().bytes(9999).events(10);
        let mut buffered = BatchSink::new(svc, VecBuffer::new(batch.size), TIMEOUT, acker);

        let mut cx = Context::from_waker(noop_waker_ref());
        assert!(matches!(
            buffered.poll_ready_unpin(&mut cx),
            Poll::Ready(Ok(()))
        ));
        assert!(matches!(buffered.start_send_unpin(0), Ok(())));
        assert!(matches!(
            buffered.poll_ready_unpin(&mut cx),
            Poll::Ready(Ok(()))
        ));
        assert!(matches!(buffered.start_send_unpin(1), Ok(())));

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
        let (acker, _) = Acker::new_for_testing();
        let sent_requests = Arc::new(Mutex::new(Vec::new()));

        let svc = tower::service_fn(|req| {
            let sent_requests = Arc::clone(&sent_requests);
            sent_requests.lock().unwrap().push(req);
            future::ok::<_, std::io::Error>(())
        });

        let batch = BatchSettings::default().bytes(9999).events(10);
        let sink = PartitionBatchSink::new(svc, VecBuffer::new(batch.size), TIMEOUT, acker);

        sink.sink_map_err(drop)
            .send_all(&mut stream::iter(0..22).map(Ok))
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
        let (acker, _) = Acker::new_for_testing();
        let sent_requests = Arc::new(Mutex::new(Vec::new()));

        let svc = tower::service_fn(|req| {
            let sent_requests = Arc::clone(&sent_requests);
            sent_requests.lock().unwrap().push(req);
            future::ok::<_, std::io::Error>(())
        });

        let batch = BatchSettings::default().bytes(9999).events(1);
        let sink = PartitionBatchSink::new(svc, VecBuffer::new(batch.size), TIMEOUT, acker);

        let input = vec![Partitions::A, Partitions::B];
        sink.sink_map_err(drop)
            .send_all(&mut stream::iter(input).map(Ok))
            .await
            .unwrap();

        let mut output = sent_requests.lock().unwrap();
        output[..].sort();
        assert_eq!(&*output, &vec![vec![Partitions::A], vec![Partitions::B]]);
    }

    #[tokio::test]
    async fn partition_batch_sink_buffers_by_partition_buffer_size_two() {
        let (acker, _) = Acker::new_for_testing();
        let sent_requests = Arc::new(Mutex::new(Vec::new()));

        let svc = tower::service_fn(|req| {
            let sent_requests = Arc::clone(&sent_requests);
            sent_requests.lock().unwrap().push(req);
            future::ok::<_, std::io::Error>(())
        });

        let batch = BatchSettings::default().bytes(9999).events(2);
        let sink = PartitionBatchSink::new(svc, VecBuffer::new(batch.size), TIMEOUT, acker);

        let input = vec![Partitions::A, Partitions::B, Partitions::A, Partitions::B];
        sink.sink_map_err(drop)
            .send_all(&mut stream::iter(input).map(Ok))
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
        let (acker, _) = Acker::new_for_testing();
        let sent_requests = Arc::new(Mutex::new(Vec::new()));

        let svc = tower::service_fn(|req| {
            let sent_requests = Arc::clone(&sent_requests);
            sent_requests.lock().unwrap().push(req);
            future::ok::<_, std::io::Error>(())
        });

        let batch = BatchSettings::default().bytes(9999).events(10);
        let mut sink = PartitionBatchSink::new(svc, VecBuffer::new(batch.size), TIMEOUT, acker);

        let mut cx = Context::from_waker(noop_waker_ref());
        assert!(matches!(
            sink.poll_ready_unpin(&mut cx),
            Poll::Ready(Ok(()))
        ));
        assert!(matches!(sink.start_send_unpin(1), Ok(())));
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
        // We need a mock executor here because we need to ensure
        // that we poll the service futures within the mock clock
        // context. This allows us to manually advance the time on the
        // "spawned" futures.
        let (acker, ack_counter) = Acker::new_for_testing();

        let svc = tower::service_fn(|req: u8| {
            if req == 3 {
                future::err("bad")
            } else {
                future::ok("good")
            }
        });
        let mut sink = ServiceSink::new(svc, acker);

        // send some initial requests
        let mut fut1 = sink.call(1, 1);
        let mut fut2 = sink.call(2, 2);

        assert_eq!(ack_counter.load(Relaxed), 0);

        let mut cx = Context::from_waker(noop_waker_ref());
        assert!(matches!(fut1.poll_unpin(&mut cx), Poll::Ready(())));
        assert!(matches!(fut2.poll_unpin(&mut cx), Poll::Ready(())));
        assert!(matches!(sink.poll_complete(&mut cx), Poll::Ready(())));

        assert_eq!(ack_counter.load(Relaxed), 3);

        // send one request that will error and one normal
        let mut fut3 = sink.call(3, 3); // i will error
        let mut fut4 = sink.call(4, 4);

        // make sure they all "worked"
        assert!(matches!(fut3.poll_unpin(&mut cx), Poll::Ready(())));
        assert!(matches!(fut4.poll_unpin(&mut cx), Poll::Ready(())));
        assert!(matches!(sink.poll_complete(&mut cx), Poll::Ready(())));

        assert_eq!(ack_counter.load(Relaxed), 10);
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

    impl Partition<Bytes> for u8 {
        fn partition(&self) -> Bytes {
            "key".into()
        }
    }

    impl Partition<Bytes> for i32 {
        fn partition(&self) -> Bytes {
            "key".into()
        }
    }

    impl Partition<Bytes> for Vec<i32> {
        fn partition(&self) -> Bytes {
            "key".into()
        }
    }
}
