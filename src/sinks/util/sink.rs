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
//! to _stream_ them to the downstream service. `BatchSink` and `PartitonBatchSink`
//! are similar in the sense that they both take some `tower::Service`, `Batch` and
//! `Acker` and will provide full batching, request dipstaching and acking based on
//! the settings passed.
//!
//! For more advanced use cases like http based sinks, one should use the
//! `BatchedHttpSink` type, which is a wrapper for `BatchSink` and `HttpSink`.
//!
//! # Driving to completetion
//!
//! Each sink utility provided here strictly follows the patterns described in
//! the `futures01::Sink` docs. Each sink utility must be polled from a valid
//! tokio context wether that may be an actual runtime or using any of the
//! `tokio01-test` utilities.
//!
//! For service based sinks like `BatchSink` and `PartitionBatchSink` they also
//! must be polled within a valid tokio executor context or passed a valid executor.
//! This is due to the fact that they will spawn service requests to allow them to be
//! driven independently from the sink. A oneshot channel is used to tie them back into
//! the sink to allow it to notify the consumer that the request has succeeded.

use super::batch::{Batch, BatchSettings};
use super::buffer::partition::Partition;
use crate::buffers::Acker;
use futures01::{
    future::Either,
    stream::FuturesUnordered,
    sync::oneshot::{self, Receiver},
    try_ready, Async, AsyncSink, Future, Poll, Sink, StartSend, Stream,
};
use std::{
    collections::{HashMap, VecDeque},
    fmt,
    hash::Hash,
    marker::PhantomData,
    time::Instant,
};
use tokio01::{
    executor::{DefaultExecutor, Executor},
    timer::Delay,
};
use tower::Service;
use tracing_futures::Instrument;

// === StreamSink ===

const STREAM_SINK_MAX: usize = 10_000;

/// Simple stream based sink adapter.
///
/// This will wrap any inner sink acking all events
/// as soon as poll_complete returns ready. `start_send`
/// will also attempt to fully flush if the amount of
/// in flight acks is larger than `STREAM_SINK_MAX`.
#[derive(Debug)]
pub struct StreamSink<T> {
    inner: T,
    acker: Acker,
    pending: usize,
}

impl<T> StreamSink<T> {
    pub fn new(inner: T, acker: Acker) -> Self {
        Self {
            inner,
            acker,
            pending: 0,
        }
    }
}

impl<T: Sink> Sink for StreamSink<T> {
    type SinkItem = T::SinkItem;
    type SinkError = T::SinkError;

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        trace!("sending item.");
        match self.inner.start_send(item)? {
            AsyncSink::Ready => {
                self.pending += 1;
                trace!(message = "submit successful.", pending_acks = self.pending);

                if self.pending >= STREAM_SINK_MAX {
                    self.poll_complete()?;
                }

                Ok(AsyncSink::Ready)
            }

            AsyncSink::NotReady(item) => {
                trace!("Inner sink applying back pressure.");
                Ok(AsyncSink::NotReady(item))
            }
        }
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        try_ready!(self.inner.poll_complete());

        trace!(message = "Acking events.", acking_num = self.pending);
        self.acker.ack(self.pending);
        self.pending = 0;

        Ok(().into())
    }
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
pub struct BatchSink<S, B, Request, E = DefaultExecutor> {
    service: ServiceSink<S, Request>,
    batch: B,
    settings: BatchSettings,
    linger: Option<Delay>,
    closing: bool,
    exec: E,
    _pd: PhantomData<Request>,
}

impl<S, B, Request> BatchSink<S, B, Request, DefaultExecutor>
where
    S: Service<Request>,
    S::Future: Send + 'static,
    S::Error: Into<crate::Error> + Send + 'static,
    S::Response: Response,
    B: Batch<Output = Request>,
{
    pub fn new(service: S, batch: B, settings: BatchSettings, acker: Acker) -> Self {
        BatchSink::with_executor(service, batch, settings, acker, DefaultExecutor::current())
    }
}

impl<S, B, Request, E> BatchSink<S, B, Request, E>
where
    S: Service<Request>,
    S::Future: Send + 'static,
    S::Error: Into<crate::Error> + Send + 'static,
    S::Response: Response,
    B: Batch<Output = Request>,
    E: Executor,
{
    pub fn with_executor(
        service: S,
        batch: B,
        settings: BatchSettings,
        acker: Acker,
        exec: E,
    ) -> Self {
        let service = ServiceSink::new(service, acker);

        Self {
            service,
            batch,
            settings,
            linger: None,
            closing: false,
            exec,
            _pd: PhantomData,
        }
    }

    fn should_send(&mut self) -> bool {
        self.closing || self.batch.len() >= self.settings.size || self.linger_elapsed()
    }

    fn linger_elapsed(&mut self) -> bool {
        if let Some(delay) = &mut self.linger {
            delay.poll().expect("timer error").is_ready()
        } else {
            false
        }
    }
}

impl<S, B, Request, E> Sink for BatchSink<S, B, Request, E>
where
    S: Service<Request>,
    S::Future: Send + 'static,
    S::Error: Into<crate::Error> + Send + 'static,
    S::Response: Response,
    B: Batch<Output = Request>,
    E: Executor,
{
    type SinkItem = B::Input;
    type SinkError = crate::Error;

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        if self.batch.len() >= self.settings.size {
            trace!("batch full.");
            self.poll_complete()?;

            if self.batch.len() > self.settings.size {
                debug!(message = "Batch full; applying back pressure.", size = %self.settings.size, rate_limit_secs = 10);
                return Ok(AsyncSink::NotReady(item));
            }
        }

        if self.batch.len() == 0 {
            trace!("Creating new batch.");
            // We just inserted the first item of a new batch, so set our delay to the longest time
            // we want to allow that item to linger in the batch before being flushed.
            let deadline = Instant::now() + self.settings.timeout;
            self.linger = Some(Delay::new(deadline));
        }

        self.batch.push(item);

        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        loop {
            if self.batch.is_empty() {
                trace!("no batches; driving service to completion.");
                return self.service.poll_complete();
            } else {
                // We have data to send, so check if we should send it and either attempt the send
                // or return that we're not ready to send. If we send and it works, loop to poll or
                // close inner instead of prematurely returning Ready
                if self.should_send() {
                    try_ready!(self.service.poll_ready());

                    trace!("Service ready; Sending batch.");
                    let batch = self.batch.fresh_replace();

                    let batch_size = batch.num_items();
                    let request = batch.finish();

                    let fut = self.service.call(request, batch_size);

                    self.exec.spawn(fut).expect("Spawn service future");

                    // Disable linger timeout
                    self.linger.take();
                } else {
                    // We have a batch but we can't send any items
                    // most likely because we have not hit either
                    // our batch size or the timeout. Here we want
                    // to poll the inner futures but still return
                    // NotReady if the linger timeout is not complete yet.
                    if let Some(linger) = &mut self.linger {
                        trace!("polling batch linger.");
                        self.service.poll_complete()?;
                        try_ready!(linger.poll());
                    } else {
                        try_ready!(self.service.poll_complete());
                    }
                }
            }
        }
    }

    fn close(&mut self) -> Poll<(), Self::SinkError> {
        trace!("closing batch sink.");
        self.closing = true;
        self.poll_complete()
    }
}

impl<S, B, Request> fmt::Debug for BatchSink<S, B, Request>
where
    S: fmt::Debug,
    B: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BatchSink")
            .field("service", &self.service)
            .field("batch", &self.batch)
            .field("settings", &self.settings)
            .finish()
    }
}

// === PartitionBatchSink ===

type LingerDelay<K> = Box<dyn Future<Item = LingerState<K>, Error = ()> + Send + 'static>;

/// A partition based batcher, given some `Service` and `Batch` where the
/// input is partitionable via the `Partition` trait, it will hold many
/// in flight batches.
///
/// This type is similar to `BatchSink` with the added benefit that it has
/// more fine grained partitioning ability. It will hold many different batches
/// of events and contain linger timeouts for each.
///
/// # Acking
///
/// Service based acking will only ack events when all prior request
/// batches have been acked. This means if sequential requests r1, r2,
/// and r3 are dispatched and r2 and r3 complete, all events contained
/// in all requests will not be acked until r1 has completed.
pub struct PartitionBatchSink<B, S, K, Request, E = DefaultExecutor> {
    batch: B,
    service: ServiceSink<S, Request>,
    exec: E,
    partitions: HashMap<K, B>,
    settings: BatchSettings,
    closing: bool,
    sending: VecDeque<B>,
    lingers: FuturesUnordered<LingerDelay<K>>,
    linger_handles: HashMap<K, oneshot::Sender<K>>,
}

enum LingerState<K> {
    Elapsed(K),
    Canceled,
}

impl<B, S, K, Request> PartitionBatchSink<B, S, K, Request, DefaultExecutor>
where
    B: Batch<Output = Request>,
    B::Input: Partition<K>,
    K: Hash + Eq + Clone + Send + 'static,
    S: Service<Request>,
    S::Future: Send + 'static,
    S::Error: Into<crate::Error> + Send + 'static,
    S::Response: Response,
{
    pub fn new(service: S, batch: B, settings: BatchSettings, acker: Acker) -> Self {
        PartitionBatchSink::with_executor(
            service,
            batch,
            settings,
            acker,
            DefaultExecutor::current(),
        )
    }
}

impl<B, S, K, Request, E> PartitionBatchSink<B, S, K, Request, E>
where
    B: Batch<Output = Request>,
    B::Input: Partition<K>,
    K: Hash + Eq + Clone + Send + 'static,
    S: Service<Request>,
    S::Future: Send + 'static,
    S::Error: Into<crate::Error> + Send + 'static,
    S::Response: Response,
    E: Executor,
{
    pub fn with_executor(
        service: S,
        batch: B,
        settings: BatchSettings,
        acker: Acker,
        exec: E,
    ) -> Self {
        let service = ServiceSink::new(service, acker);

        Self {
            batch,
            service,
            exec,
            partitions: HashMap::new(),
            settings,
            closing: false,
            sending: VecDeque::new(),
            lingers: FuturesUnordered::new(),
            linger_handles: HashMap::new(),
        }
    }

    fn set_linger(&mut self, partition: K) {
        let (tx, rx) = oneshot::channel();
        let partition_clone = partition.clone();

        let deadline = Instant::now() + self.settings.timeout;
        let delay = Delay::new(deadline)
            .map(move |_| LingerState::Elapsed(partition_clone))
            .map_err(|_| ());

        let cancel = rx.map(|_| LingerState::Canceled).map_err(|_| ());

        let fut = cancel
            .select2(delay)
            .map(|state| match state {
                Either::A((state, _)) => state,
                Either::B((state, _)) => state,
            })
            .map_err(|_| ());

        self.linger_handles.insert(partition, tx);
        self.lingers.push(Box::new(fut));
    }

    fn poll_send(&mut self, batch: B) -> Poll<(), crate::Error> {
        if let Async::NotReady = self.service.poll_ready()? {
            self.sending.push_front(batch);
            Ok(Async::NotReady)
        } else {
            let batch_size = batch.num_items();
            let batch = batch.finish();
            let fut = self.service.call(batch, batch_size);

            self.exec.spawn(fut).expect("Spawn service future");

            self.service.poll_complete()
        }
    }
}

impl<B, S, K, Request, E> Sink for PartitionBatchSink<B, S, K, Request, E>
where
    B: Batch<Output = Request>,
    B::Input: Partition<K>,
    K: Hash + Eq + Clone + Send + 'static,
    S: Service<Request>,
    S::Future: Send + 'static,
    S::Error: Into<crate::Error> + Send + 'static,
    S::Response: Response,
    E: Executor,
{
    type SinkItem = B::Input;
    type SinkError = crate::Error;

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        // Apply back pressure if we are buffering more than
        // 5 batches, this should only happen if the inner sink
        // is apply back pressure.
        if self.sending.len() > 5 {
            trace!(
                message = "too many sending batches.",
                amount = self.sending.len()
            );
            self.poll_complete()?;

            if self.sending.len() > 5 {
                debug!(
                    message = "Too many open batches; applying back pressure.",
                    max_batch_size = 5,
                    rate_limit_secs = 10
                );
                return Ok(AsyncSink::NotReady(item));
            }
        }

        let partition = item.partition();

        if let Some(batch) = self.partitions.get_mut(&partition) {
            if batch.len() >= self.settings.size {
                trace!("Batch full; driving service to completion.");
                self.poll_complete()?;

                if let Some(batch) = self.partitions.get_mut(&partition) {
                    if batch.len() >= self.settings.size {
                        debug!(
                            message = "Buffer full; applying back pressure.",
                            max_size = %self.settings.size,
                            rate_limit_secs = 10
                        );
                        return Ok(AsyncSink::NotReady(item));
                    } else {
                        batch.push(item);
                        return Ok(AsyncSink::Ready);
                    }
                }
            } else {
                trace!("adding event to batch.");
                batch.push(item);
                return Ok(AsyncSink::Ready);
            }
        }

        trace!("replacing batch.");
        // We fall through to this case, when there is no batch already
        // or the batch got submitted by polling_complete above.
        let mut batch = self.batch.fresh();

        batch.push(item);
        self.set_linger(partition.clone());

        self.partitions.insert(partition, batch);

        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        self.service.poll_complete()?;

        while let Some(batch) = self.sending.pop_front() {
            self.poll_send(batch)?;
        }

        let closing = self.closing;
        let max_size = self.settings.size;

        let mut partitions = Vec::new();

        while let Ok(Async::Ready(Some(linger))) = self.lingers.poll() {
            // Only if the linger has elapsed trigger the removal
            if let LingerState::Elapsed(partition) = linger {
                trace!("batch linger expired.");
                self.linger_handles.remove(&partition);

                if let Some(batch) = self.partitions.remove(&partition) {
                    partitions.push(batch);
                }
            }
        }

        let ready = self
            .partitions
            .iter()
            .filter(|(_, b)| closing || b.len() >= max_size)
            .map(|(p, _)| p.clone())
            .collect::<Vec<_>>();

        let mut ready_batches = Vec::new();
        for partition in ready {
            if let Some(batch) = self.partitions.remove(&partition) {
                if let Some(linger_cancel) = self.linger_handles.remove(&partition) {
                    // XXX: had to remove the expect here, a cancaellation should
                    // always be a best effort.
                    let _ = linger_cancel.send(partition.clone());
                }

                ready_batches.push(batch);
            }
        }

        for batch in ready_batches.into_iter().chain(partitions) {
            self.poll_send(batch)?;
        }

        // If we still have an inflight partition then
        // we should have a linger associated with it that
        // will wake up this task when it is ready to be flushed.
        if !self.partitions.is_empty() {
            assert!(
                !self.lingers.is_empty(),
                "If partitions are not empty, then there must be a linger"
            );
            self.service.poll_complete()?;
            Ok(Async::NotReady)
        } else {
            self.service.poll_complete()
        }
    }

    fn close(&mut self) -> Poll<(), Self::SinkError> {
        trace!("closing partition batch sink.");
        self.closing = true;
        self.poll_complete()
    }
}

impl<B, S, K, Request> fmt::Debug for PartitionBatchSink<B, S, K, Request>
where
    B: fmt::Debug,
    S: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PartitionedBatchSink")
            .field("batch", &self.batch)
            .field("service", &self.service)
            .field("settings", &self.settings)
            .finish()
    }
}

// === ServiceSink ===

struct ServiceSink<S, Request> {
    service: S,
    in_flight: FuturesUnordered<Receiver<(usize, usize)>>,
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

    fn poll_ready(&mut self) -> Poll<(), crate::Error> {
        self.service.poll_ready().map_err(Into::into)
    }

    fn call(
        &mut self,
        req: Request,
        batch_size: usize,
    ) -> Box<dyn Future<Item = (), Error = ()> + Send + 'static> {
        let seqno = self.seq_head;
        self.seq_head += 1;

        let (tx, rx) = oneshot::channel();

        self.in_flight.push(rx);

        let request_id = self.next_request_id;
        self.next_request_id = request_id.wrapping_add(1);

        trace!(
            message = "submitting service request.",
            in_flight_requests = self.in_flight.len()
        );
        let response = self
            .service
            .call(req)
            .map_err(Into::into)
            .then(move |result| {
                match result {
                    Ok(response) if response.is_successful() => {
                        trace!(message = "Response successful.", ?response);
                    }
                    Ok(response) => {
                        error!(message = "Response wasn't successful.", ?response);
                    }
                    Err(error) => {
                        error!(
                            message = "Request failed.",
                            %error,
                        );
                    }
                }

                // If the rx end is dropped we still completed
                // the request so this is a weird case that we can
                // ignore for now.
                let _ = tx.send((seqno, batch_size));

                Ok::<_, ()>(())
            })
            .instrument(info_span!("request", %request_id));

        Box::new(response)
    }

    fn poll_complete(&mut self) -> Poll<(), crate::Error> {
        loop {
            match self.in_flight.poll() {
                Ok(Async::NotReady) => return Ok(Async::NotReady),
                Ok(Async::Ready(None)) => return Ok(Async::Ready(())),
                Ok(Async::Ready(Some((seqno, batch_size)))) => {
                    self.pending_acks.insert(seqno, batch_size);

                    let mut num_to_ack = 0;
                    while let Some(ack_size) = self.pending_acks.remove(&self.seq_tail) {
                        num_to_ack += ack_size;
                        self.seq_tail += 1
                    }
                    trace!(message = "acking events.", acking_num = num_to_ack);
                    self.acker.ack(num_to_ack);
                }
                Err(_) => panic!("ServiceSink service sender dropped"),
            }
        }
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
    use crate::buffers::Acker;
    use crate::sinks::util::{buffer::partition::Partition, BatchSettings, Buffer, Compression};
    use crate::test_util::runtime;
    use bytes::Bytes;
    use futures01::{future, Sink};
    use std::{
        sync::{atomic::Ordering::Relaxed, Arc, Mutex},
        time::Duration,
    };
    use tokio01_test::clock::MockClock;

    const SETTINGS: BatchSettings = BatchSettings {
        size: 10,
        timeout: Duration::from_secs(10),
    };

    #[test]
    fn batch_sink_acking_sequential() {
        let rt = runtime();

        let mut clock = MockClock::new();

        let (acker, ack_counter) = Acker::new_for_testing();

        let svc = tower::service_fn(|_| future::ok::<_, std::io::Error>(()));
        let buffered = BatchSink::with_executor(svc, Vec::new(), SETTINGS, acker, rt.executor());

        let _ = clock.enter(|_| {
            buffered
                .sink_map_err(drop)
                .send_all(futures01::stream::iter_ok(0..22))
                .wait()
                .unwrap()
        });

        assert_eq!(ack_counter.load(Relaxed), 22);
    }

    #[test]
    fn batch_sink_acking_unordered() {
        // We need a mock executor here because we need to ensure
        // that we poll the service futures within the mock clock
        // context. This allows us to manually advance the time on the
        // "spawned" futures.
        let mut exec = MockExec::default();
        let mut clock = MockClock::new();

        let (acker, ack_counter) = Acker::new_for_testing();

        let svc = tower::service_fn(|req: Vec<usize>| {
            let dur = match req[0] {
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

            let deadline = Instant::now() + dur;

            Delay::new(deadline).map(drop)
        });

        let settings = BatchSettings {
            size: 1,
            ..SETTINGS
        };

        let mut sink = BatchSink::with_executor(svc, Vec::new(), settings, acker, exec.clone());

        let _ = clock.enter(|handle| {
            assert!(sink.start_send(0).unwrap().is_ready());
            assert!(sink.start_send(1).unwrap().is_ready());

            assert_eq!(ack_counter.load(Relaxed), 0);

            handle.advance(Duration::from_secs(2));

            // We must first poll so that we send the messages
            // then we must pull the mock executor to set the timers
            // then poll again to ack.
            sink.poll_complete().unwrap();
            exec.poll().unwrap();
            sink.poll_complete().unwrap();

            assert_eq!(ack_counter.load(Relaxed), 2);

            assert!(sink.start_send(2).unwrap().is_ready());

            sink.poll_complete().unwrap();
            exec.poll().unwrap();
            sink.poll_complete().unwrap();

            assert_eq!(ack_counter.load(Relaxed), 3);

            assert!(sink.start_send(3).unwrap().is_ready());
            assert!(sink.start_send(4).unwrap().is_ready());
            assert!(sink.start_send(5).unwrap().is_ready());

            handle.advance(Duration::from_secs(2));

            sink.poll_complete().unwrap();
            exec.poll().unwrap();
            sink.poll_complete().unwrap();

            // Check that events 3,4,6 have not been acked yet
            // only the three previous should be acked.
            assert_eq!(ack_counter.load(Relaxed), 3);

            handle.advance(Duration::from_secs(5));

            exec.poll().unwrap();
            sink.flush().wait().unwrap();

            assert_eq!(ack_counter.load(Relaxed), 6);
        });
    }

    #[test]
    fn batch_sink_buffers_messages_until_limit() {
        let rt = runtime();
        let mut clock = MockClock::new();

        let (acker, _) = Acker::new_for_testing();
        let sent_requests = Arc::new(Mutex::new(Vec::new()));

        let svc = tower::service_fn(|req| {
            let sent_requests = sent_requests.clone();

            sent_requests.lock().unwrap().push(req);

            future::ok::<_, std::io::Error>(())
        });
        let buffered = BatchSink::with_executor(svc, Vec::new(), SETTINGS, acker, rt.executor());

        let _ = clock.enter(|_| {
            buffered
                .sink_map_err(drop)
                .send_all(futures01::stream::iter_ok(0..22))
                .wait()
                .unwrap()
        });

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

    #[test]
    fn batch_sink_flushes_below_min_on_close() {
        let rt = runtime();
        let mut clock = MockClock::new();

        let (acker, _) = Acker::new_for_testing();
        let sent_requests = Arc::new(Mutex::new(Vec::new()));

        let svc = tower::service_fn(|req| {
            let sent_requests = sent_requests.clone();

            sent_requests.lock().unwrap().push(req);

            future::ok::<_, std::io::Error>(())
        });
        let mut buffered =
            BatchSink::with_executor(svc, Vec::new(), SETTINGS, acker, rt.executor());

        clock.enter(|_| {
            assert!(buffered.start_send(0).unwrap().is_ready());
            assert!(buffered.start_send(1).unwrap().is_ready());

            future::poll_fn(|| buffered.close()).wait().unwrap()
        });

        let output = sent_requests.lock().unwrap();
        assert_eq!(&*output, &vec![vec![0, 1]]);
    }

    #[test]
    fn batch_sink_expired_linger() {
        let rt = runtime();
        let mut clock = MockClock::new();

        let (acker, _) = Acker::new_for_testing();
        let sent_requests = Arc::new(Mutex::new(Vec::new()));

        let svc = tower::service_fn(|req| {
            let sent_requests = sent_requests.clone();

            sent_requests.lock().unwrap().push(req);

            future::ok::<_, std::io::Error>(())
        });
        let mut buffered =
            BatchSink::with_executor(svc, Vec::new(), SETTINGS, acker, rt.executor());

        clock.enter(|handle| {
            assert!(buffered.start_send(0).unwrap().is_ready());
            assert!(buffered.start_send(1).unwrap().is_ready());

            // Move clock forward by linger timeout + 1 sec
            handle.advance(SETTINGS.timeout + Duration::from_secs(1));

            future::poll_fn(|| buffered.poll_complete()).wait().unwrap();
        });

        let output = sent_requests.lock().unwrap();
        assert_eq!(&*output, &vec![vec![0, 1]]);
    }

    #[test]
    fn batch_sink_allows_the_final_item_to_exceed_the_buffer_size() {
        let rt = runtime();
        let mut clock = MockClock::new();

        let (acker, _) = Acker::new_for_testing();
        let sent_requests = Arc::new(Mutex::new(Vec::new()));

        let svc = tower::service_fn(|req| {
            let sent_requests = sent_requests.clone();

            sent_requests.lock().unwrap().push(req);

            future::ok::<_, std::io::Error>(())
        });
        let buffered = BatchSink::with_executor(
            svc,
            Buffer::new(Compression::None),
            SETTINGS,
            acker,
            rt.executor(),
        );

        let input = vec![
            vec![0, 1, 2],
            vec![3, 4, 5],
            vec![6, 7, 8],
            vec![9, 10, 11],
            vec![12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23],
            vec![24],
        ];
        let _ = clock.enter(|_| {
            buffered
                .sink_map_err(drop)
                .send_all(futures01::stream::iter_ok(input))
                .wait()
                .unwrap()
        });

        let output = sent_requests.lock().unwrap();
        assert_eq!(
            &*output,
            &vec![
                vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
                vec![12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23],
                vec![24],
            ]
        );
    }

    #[test]
    fn partition_batch_sink_buffers_messages_until_limit() {
        let rt = runtime();
        let (acker, _) = Acker::new_for_testing();
        let sent_requests = Arc::new(Mutex::new(Vec::new()));

        let svc = tower::service_fn(|req| {
            let sent_requests = sent_requests.clone();

            sent_requests.lock().unwrap().push(req);

            future::ok::<_, std::io::Error>(())
        });
        let buffered =
            PartitionBatchSink::with_executor(svc, Vec::new(), SETTINGS, acker, rt.executor());

        let (_buffered, _) = buffered
            .sink_map_err(drop)
            .send_all(futures01::stream::iter_ok(0..22))
            .wait()
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

    #[test]
    fn partition_batch_sink_buffers_by_partition_buffer_size_one() {
        let rt = runtime();
        let (acker, _) = Acker::new_for_testing();
        let sent_requests = Arc::new(Mutex::new(Vec::new()));

        let svc = tower::service_fn(|req| {
            let sent_requests = sent_requests.clone();

            sent_requests.lock().unwrap().push(req);

            future::ok::<_, std::io::Error>(())
        });

        let settings = BatchSettings {
            size: 1,
            ..SETTINGS
        };

        let buffered =
            PartitionBatchSink::with_executor(svc, Vec::new(), settings, acker, rt.executor());

        let input = vec![Partitions::A, Partitions::B];

        let (_buffered, _) = buffered
            .sink_map_err(drop)
            .send_all(futures01::stream::iter_ok(input))
            .wait()
            .unwrap();

        let mut output = sent_requests.lock().unwrap();
        output[..].sort();
        assert_eq!(&*output, &vec![vec![Partitions::A], vec![Partitions::B]]);
    }

    #[test]
    fn partition_batch_sink_buffers_by_partition_buffer_size_two() {
        let rt = runtime();
        let (acker, _) = Acker::new_for_testing();
        let sent_requests = Arc::new(Mutex::new(Vec::new()));

        let svc = tower::service_fn(|req| {
            let sent_requests = sent_requests.clone();

            sent_requests.lock().unwrap().push(req);

            future::ok::<_, std::io::Error>(())
        });

        let settings = BatchSettings {
            size: 2,
            ..SETTINGS
        };

        let buffered =
            PartitionBatchSink::with_executor(svc, Vec::new(), settings, acker, rt.executor());

        let input = vec![Partitions::A, Partitions::B, Partitions::A, Partitions::B];

        let (_buffered, _) = buffered
            .sink_map_err(drop)
            .send_all(futures01::stream::iter_ok(input))
            .wait()
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

    #[test]
    fn partition_batch_sink_submits_after_linger() {
        let mut clock = MockClock::new();
        let rt = runtime();
        let (acker, _) = Acker::new_for_testing();
        let sent_requests = Arc::new(Mutex::new(Vec::new()));

        let svc = tower::service_fn(|req| {
            let sent_requests = sent_requests.clone();

            sent_requests.lock().unwrap().push(req);

            future::ok::<_, std::io::Error>(())
        });

        let mut buffered =
            PartitionBatchSink::with_executor(svc, Vec::new(), SETTINGS, acker, rt.executor());

        clock.enter(|handle| {
            buffered.start_send(1 as usize).unwrap();
            buffered.poll_complete().unwrap();

            handle.advance(Duration::from_secs(11));

            buffered.poll_complete().unwrap();
        });

        let output = sent_requests.lock().unwrap();
        assert_eq!(&*output, &vec![vec![1]]);
    }

    #[test]
    fn service_sink_doesnt_propagate_error() {
        // We need a mock executor here because we need to ensure
        // that we poll the service futures within the mock clock
        // context. This allows us to manually advance the time on the
        // "spawned" futures.
        let (acker, ack_counter) = Acker::new_for_testing();

        let svc = tower::service_fn(|req: u8| if req == 3 { Err("bad") } else { Ok("good") });

        let mut sink = ServiceSink::new(svc, acker);

        let mut clock = MockClock::new();
        clock.enter(|_handle| {
            // send some initial requests
            let mut fut1 = sink.call(1, 1);
            let mut fut2 = sink.call(2, 2);

            assert_eq!(ack_counter.load(Relaxed), 0);

            // make sure they all worked
            assert!(fut1.poll().unwrap().is_ready());
            assert!(fut2.poll().unwrap().is_ready());
            assert!(sink.poll_complete().unwrap().is_ready());
            assert_eq!(ack_counter.load(Relaxed), 3);

            // send one request that will error and one normal
            let mut fut3 = sink.call(3, 3); // i will error
            let mut fut4 = sink.call(4, 4);

            // make sure they all "worked"
            assert!(fut3.poll().unwrap().is_ready());
            assert!(fut4.poll().unwrap().is_ready());
            assert!(sink.poll_complete().unwrap().is_ready());
            assert_eq!(ack_counter.load(Relaxed), 10);
        });
    }

    #[derive(Default, Clone)]
    struct MockExec(
        Arc<
            Mutex<
                Vec<(
                    Box<dyn Future<Item = (), Error = ()> + Send + 'static>,
                    bool,
                )>,
            >,
        >,
    );

    impl tokio01::executor::Executor for MockExec {
        fn spawn(
            &mut self,
            fut: Box<dyn Future<Item = (), Error = ()> + Send + 'static>,
        ) -> Result<(), tokio01::executor::SpawnError> {
            let mut futs = self.0.lock().unwrap();
            Ok(futs.push((fut, false)))
        }
    }

    impl MockExec {
        pub fn poll(&mut self) -> futures01::Poll<(), ()> {
            let mut futs = self.0.lock().unwrap();

            for (fut, is_done) in &mut *futs {
                if !*is_done {
                    if let Async::Ready(_) = fut.poll()? {
                        *is_done = true;
                    }
                }
            }

            // XXX: We don't really use this beyond polling a group
            // of futures, it is expected that the user manually keeps
            // polling this. Its not very useful but it works for now.
            Ok(Async::Ready(()))
        }
    }

    #[derive(Debug, PartialEq, Eq, Ord, PartialOrd)]
    enum Partitions {
        A,
        B,
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
