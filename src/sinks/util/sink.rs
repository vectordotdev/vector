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
    buffer::partition::Partition,
};
use crate::{buffers::Acker, Event};
use async_trait::async_trait;
use futures::{
    compat::{Compat, Future01CompatExt},
    future::BoxFuture,
    ready,
    stream::{BoxStream, FuturesUnordered},
    FutureExt, Sink, Stream, TryFutureExt,
};
use futures01::{
    future::Either, stream::FuturesUnordered as FuturesUnordered01, sync::oneshot as oneshot01,
    Async, AsyncSink, Future as Future01, Poll as Poll01, Sink as Sink01, StartSend,
    Stream as Stream01,
};
use pin_project::pin_project;
use std::{
    collections::{HashMap, VecDeque},
    fmt,
    future::Future,
    hash::Hash,
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::{
    sync::oneshot,
    time::{delay_for, Delay, Duration},
};
use tower::Service;
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
pub struct BatchSink<S, B, Request>
where
    B: Batch<Output = Request>,
{
    service: ServiceSink<S, Request>,
    buffer: Option<B::Input>,
    batch: StatefulBatch<B>,
    timeout: Duration,
    linger: Option<Delay>,
    closing: bool,
    _pd: PhantomData<Request>,
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
        let service = ServiceSink::new(service, acker);

        Self {
            service,
            buffer: None,
            batch: batch.into(),
            timeout,
            linger: None,
            closing: false,
            _pd: PhantomData,
        }
    }

    fn poll_should_send(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        if !self.closing && !self.batch.was_full() {
            let linger = self
                .linger
                .as_mut()
                .expect("linger should exists for should_send");
            ready!(Pin::new(linger).poll(cx));
        }

        Poll::Ready(())
    }
}

impl<S, B, Request> BatchSink<S, B, Request>
where
    B: Batch<Output = Request>,
{
    pub fn get_ref(&self) -> &S {
        &self.service.service
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

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        while self.buffer.is_some() || self.batch.was_full() {
            match self.as_mut().poll_flush(cx) {
                Poll::Ready(Ok(())) => {}
                Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                Poll::Pending => {}
            }
        }

        Poll::Ready(Ok(()))
    }

    fn start_send(mut self: Pin<&mut Self>, item: B::Input) -> Result<(), Self::Error> {
        if self.batch.is_empty() && self.linger.is_none() {
            trace!("Starting new batch timer.");
            // We just inserted the first item of a new batch, so set our delay to the longest time
            // we want to allow that item to linger in the batch before being flushed.
            self.linger = Some(delay_for(self.timeout));
        }

        if let PushResult::Overflow(item) = self.batch.push(item) {
            self.buffer = Some(item);
        }

        Ok(())
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let mut last_round = false;
        loop {
            if self.batch.is_empty() {
                // Send item from the buffer.
                if let Some(item) = self.buffer.take() {
                    self.as_mut().start_send(item)?;

                    if self.buffer.is_some() {
                        unreachable!("Empty buffer overflowed.");
                    }
                } else {
                    // Poll inner service while not ready.
                    ready!(self.service.poll_complete(cx));
                    return Poll::Ready(Ok(()));
                }
            } else {
                // We have data to send, so check if we should send it and either attempt the send
                // or return that we're not ready to send. If we send and it works, loop to poll
                // service instead of prematurely returning Ready.
                if matches!(self.poll_should_send(cx), Poll::Ready(())) {
                    last_round = false;

                    ready!(self.service.poll_ready(cx))?;

                    trace!("Service ready; Sending batch.");
                    let batch = self.batch.fresh_replace();

                    let batch_size = batch.num_items();
                    let request = batch.finish();

                    tokio::spawn(self.service.call(request, batch_size));

                    // Remove the now-sent batch's linger timeout
                    self.linger = None;
                } else {
                    // Result doesn't matter, our batch is not empty. Return Pending anyway, but
                    // loop once more for additional `poll_should_send` check.
                    ready!(self.service.poll_complete(cx));

                    if last_round {
                        return Poll::Pending;
                    }

                    last_round = true;
                }
            }
        }
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        trace!("Closing batch sink.");
        self.closing = true;
        self.poll_flush(cx)
    }
}

impl<S, B, Request> fmt::Debug for BatchSink<S, B, Request>
where
    S: fmt::Debug,
    B: Batch<Output = Request> + fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BatchSink")
            .field("service", &self.service)
            .field("batch", &self.batch)
            .field("timeout", &self.timeout)
            .finish()
    }
}

// === PartitionBatchSinkOld ===

type LingerDelayOld<K> = Box<dyn Future01<Item = LingerStateOld<K>, Error = ()> + Send + 'static>;

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
pub struct PartitionBatchSinkOld<B, S, K, Request> {
    batch: StatefulBatch<B>,
    service: ServiceSinkOld<S, Request>,
    partitions: HashMap<K, StatefulBatch<B>>,
    timeout: Duration,
    closing: bool,
    sending: VecDeque<B>,
    lingers: FuturesUnordered01<LingerDelayOld<K>>,
    linger_handles: HashMap<K, oneshot01::Sender<K>>,
}

enum LingerStateOld<K> {
    Elapsed(K),
    Canceled,
}

impl<B, S, K, Request> PartitionBatchSinkOld<B, S, K, Request>
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
        let service = ServiceSinkOld::new(service, acker);

        Self {
            batch: batch.into(),
            service,
            partitions: HashMap::new(),
            timeout,
            closing: false,
            sending: VecDeque::new(),
            lingers: FuturesUnordered01::new(),
            linger_handles: HashMap::new(),
        }
    }

    fn set_linger(&mut self, partition: K) {
        let (tx, rx) = oneshot01::channel();
        let partition_clone = partition.clone();

        let delay = delay_for(self.timeout)
            .unit_error()
            .boxed()
            .compat()
            .map(move |_| LingerStateOld::Elapsed(partition_clone))
            .map_err(|_| ());

        let cancel = rx.map(|_| LingerStateOld::Canceled).map_err(|_| ());

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

    fn poll_send(&mut self, batch: B) -> Poll01<(), crate::Error> {
        if let Async::NotReady = self.service.poll_ready()? {
            self.sending.push_front(batch);
        } else {
            let batch_size = batch.num_items();
            let batch = batch.finish();

            let fut = self.service.call(batch, batch_size).compat();
            tokio::spawn(fut);
        }

        self.service.poll_complete()
    }

    fn handle_full_batch(&mut self, item: B::Input, partition: &K) -> FullBatchResultOld<B::Input> {
        trace!("Batch full; driving service to completion.");
        if let Err(error) = self.poll_complete() {
            return FullBatchResultOld::Result(Err(error));
        }

        match self.partitions.get_mut(partition) {
            Some(batch) => {
                if !batch.is_empty() {
                    debug!(
                        message = "Send buffer full; applying back pressure.",
                        rate_limit_secs = 10
                    );
                    FullBatchResultOld::Result(Ok(AsyncSink::NotReady(item)))
                } else {
                    match batch.push(item) {
                        PushResult::Ok(full) => {
                            if full {
                                if let Err(error) = self.poll_complete() {
                                    return FullBatchResultOld::Result(Err(error));
                                }
                            }
                            FullBatchResultOld::Result(Ok(AsyncSink::Ready))
                        }
                        PushResult::Overflow(_) => unreachable!("Empty buffer overflowed"),
                    }
                }
            }
            None => FullBatchResultOld::Continue(item),
        }
    }
}

enum FullBatchResultOld<T> {
    Continue(T),
    Result(StartSend<T, crate::Error>),
}

impl<B, S, K, Request> Sink01 for PartitionBatchSinkOld<B, S, K, Request>
where
    B: Batch<Output = Request>,
    B::Input: Partition<K>,
    K: Hash + Eq + Clone + Send + 'static,
    S: Service<Request>,
    S::Future: Send + 'static,
    S::Error: Into<crate::Error> + Send + 'static,
    S::Response: Response,
{
    type SinkItem = B::Input;
    type SinkError = crate::Error;

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        // Apply back pressure if we are buffering more than
        // 5 batches, this should only happen if the inner sink
        // is apply back pressure.
        if self.sending.len() > 5 {
            trace!(
                message = "Too many sending batches.",
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

        let item = match self.partitions.get_mut(&partition) {
            Some(batch) => {
                if batch.was_full() {
                    match self.handle_full_batch(item, &partition) {
                        FullBatchResultOld::Result(result) => return result,
                        FullBatchResultOld::Continue(item) => item,
                    }
                } else {
                    trace!("Adding event to batch.");
                    match batch.push(item) {
                        PushResult::Ok(full) => {
                            if full {
                                self.poll_complete()?;
                            }
                            return Ok(AsyncSink::Ready);
                        }
                        PushResult::Overflow(item) => {
                            match self.handle_full_batch(item, &partition) {
                                FullBatchResultOld::Result(result) => return result,
                                FullBatchResultOld::Continue(item) => item,
                            }
                        }
                    }
                }
            }
            None => item,
        };

        trace!("Replacing batch.");
        // We fall through to this case, when there is no batch already
        // or the batch got submitted by polling_complete above.
        let mut batch = self.batch.fresh();

        match batch.push(item) {
            PushResult::Overflow(_) => unreachable!("Empty buffer overflowed"),
            PushResult::Ok(full) => {
                self.set_linger(partition.clone());

                self.partitions.insert(partition, batch);

                if full {
                    self.poll_complete()?;
                }

                Ok(AsyncSink::Ready)
            }
        }
    }

    fn poll_complete(&mut self) -> Poll01<(), Self::SinkError> {
        self.service.poll_complete()?;

        while let Some(batch) = self.sending.pop_front() {
            if self.poll_send(batch)? == Async::Ready(()) {
                break;
            }
        }

        let closing = self.closing;

        let mut partitions = Vec::new();

        while let Ok(Async::Ready(Some(linger))) = self.lingers.poll() {
            // Only if the linger has elapsed trigger the removal
            if let LingerStateOld::Elapsed(partition) = linger {
                trace!("Batch linger expired.");
                self.linger_handles.remove(&partition);

                if let Some(batch) = self.partitions.remove(&partition) {
                    partitions.push(batch);
                }
            }
        }

        let ready = self
            .partitions
            .iter()
            .filter(|(_, b)| closing || b.was_full())
            .map(|(p, _)| p.clone())
            .collect::<Vec<_>>();

        let mut ready_batches = Vec::new();
        for partition in ready {
            if let Some(batch) = self.partitions.remove(&partition) {
                if let Some(linger_cancel) = self.linger_handles.remove(&partition) {
                    // XXX: had to remove the expect here, a cancellation should
                    // always be a best effort.
                    let _ = linger_cancel.send(partition.clone());
                }

                ready_batches.push(batch);
            }
        }
        if !self.partitions.is_empty() {
            assert!(
                !self.lingers.is_empty(),
                "If partitions are not empty, then there must be a linger"
            );
        }

        for batch in ready_batches.into_iter().chain(partitions) {
            self.poll_send(batch.into_inner())?;
        }

        // If we still have an inflight partition then
        // we should have a linger associated with it that
        // will wake up this task when it is ready to be flushed.
        if !self.partitions.is_empty() || !self.sending.is_empty() {
            self.service.poll_complete()?;
            Ok(Async::NotReady)
        } else {
            self.service.poll_complete()
        }
    }

    fn close(&mut self) -> Poll01<(), Self::SinkError> {
        trace!("Closing partition batch sink.");
        self.closing = true;
        self.poll_complete()
    }
}

impl<B, S, K, Request> fmt::Debug for PartitionBatchSinkOld<B, S, K, Request>
where
    B: fmt::Debug,
    S: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PartitionedBatchSinkOld")
            .field("batch", &self.batch)
            .field("service", &self.service)
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

// === ServiceSinkOld ===

struct ServiceSinkOld<S, Request> {
    service: S,
    in_flight: FuturesUnordered01<oneshot01::Receiver<(usize, usize)>>,
    acker: Acker,
    seq_head: usize,
    seq_tail: usize,
    pending_acks: HashMap<usize, usize>,
    next_request_id: usize,
    _pd: PhantomData<Request>,
}

impl<S, Request> ServiceSinkOld<S, Request>
where
    S: Service<Request>,
    S::Future: Send + 'static,
    S::Error: Into<crate::Error> + Send + 'static,
    S::Response: Response,
{
    fn new(service: S, acker: Acker) -> Self {
        Self {
            service,
            in_flight: FuturesUnordered01::new(),
            acker,
            seq_head: 0,
            seq_tail: 0,
            pending_acks: HashMap::new(),
            next_request_id: 0,
            _pd: PhantomData,
        }
    }

    fn poll_ready(&mut self) -> Poll01<(), crate::Error> {
        let p = task_compat::with_context(|cx| self.service.poll_ready(cx));
        task_compat::poll_03_to_01(p).map_err(Into::into)
    }

    fn call(
        &mut self,
        req: Request,
        batch_size: usize,
    ) -> Box<dyn Future01<Item = (), Error = ()> + Send + 'static> {
        let seqno = self.seq_head;
        self.seq_head += 1;

        let (tx, rx) = oneshot01::channel();

        self.in_flight.push(rx);

        let request_id = self.next_request_id;
        self.next_request_id = request_id.wrapping_add(1);

        trace!(
            message = "Submitting service request.",
            in_flight_requests = self.in_flight.len()
        );
        let response = Compat::new(Box::pin(self.service.call(req)))
            .map_err(Into::into)
            .then(move |result| {
                match result {
                    Ok(response) if response.is_successful() => {
                        trace!(message = "Response successful.", response = ?response);
                    }
                    Ok(response) => {
                        error!(message = "Response wasn't successful.", response = ?response);
                    }
                    Err(error) => {
                        error!(message = "Request failed.", %error,);
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

    fn poll_complete(&mut self) -> Poll01<(), crate::Error> {
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
                    trace!(message = "Acking events.", acking_num = num_to_ack);
                    self.acker.ack(num_to_ack);
                }
                Err(_) => panic!("ServiceSinkOld service sender dropped."),
            }
        }
    }
}

impl<S, Request> fmt::Debug for ServiceSinkOld<S, Request>
where
    S: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ServiceSinkOld")
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
    use futures01::future as future01;
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

    // If we try poll future in tokio:0.2 Runtime directly we get `no Task is currently running`.
    async fn run_as_future01<F: Future + std::marker::Send>(f: F) -> <F as Future>::Output {
        future01::lazy(|| f.never_error().boxed().compat())
            .compat()
            .await
            .unwrap()
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
        let buffered = PartitionBatchSinkOld::new(svc, VecBuffer::new(batch.size), TIMEOUT, acker);

        let (_buffered, _) = buffered
            .sink_map_err(drop)
            .send_all(futures01::stream::iter_ok(0..22))
            .compat()
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
        let buffered = PartitionBatchSinkOld::new(svc, VecBuffer::new(batch.size), TIMEOUT, acker);

        let input = vec![Partitions::A, Partitions::B];
        let (_buffered, _) = buffered
            .sink_map_err(drop)
            .send_all(futures01::stream::iter_ok(input))
            .compat()
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
        let buffered = PartitionBatchSinkOld::new(svc, VecBuffer::new(batch.size), TIMEOUT, acker);

        let input = vec![Partitions::A, Partitions::B, Partitions::A, Partitions::B];
        let (_buffered, _) = buffered
            .sink_map_err(drop)
            .send_all(futures01::stream::iter_ok(input))
            .compat()
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
        run_as_future01(async {
            let (acker, _) = Acker::new_for_testing();
            let sent_requests = Arc::new(Mutex::new(Vec::new()));

            let svc = tower::service_fn(|req| {
                let sent_requests = Arc::clone(&sent_requests);
                sent_requests.lock().unwrap().push(req);
                future::ok::<_, std::io::Error>(())
            });

            let batch = BatchSettings::default().bytes(9999).events(10);
            let mut buffered =
                PartitionBatchSinkOld::new(svc, VecBuffer::new(batch.size), TIMEOUT, acker);

            buffered.start_send(1 as usize).unwrap();
            buffered.poll_complete().unwrap();

            advance_time(TIMEOUT + Duration::from_secs(1)).await;

            while buffered.poll_complete().unwrap() == Async::NotReady {
                yield_now().await;
            }

            let output = sent_requests.lock().unwrap();
            assert_eq!(&*output, &vec![vec![1]]);
        })
        .await;
    }

    #[tokio::test]
    async fn service_sink_doesnt_propagate_error() {
        run_as_future01(async {
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
            let mut sink = ServiceSinkOld::new(svc, acker);

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
        })
        .await;
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
