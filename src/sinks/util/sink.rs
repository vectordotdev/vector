use super::batch::{Batch, BatchSettings};
use crate::buffers::Acker;
use futures01::{
    stream::FuturesUnordered, try_ready, Async, AsyncSink, Future, Poll, Sink, StartSend, Stream,
};
use std::{collections::HashMap, fmt, marker::PhantomData, mem, time::Instant};
use tokio::timer::Delay;
use tower::Service;

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
        match self.inner.start_send(item)? {
            AsyncSink::Ready => {
                self.pending += 1;

                if self.pending >= STREAM_SINK_MAX {
                    self.poll_complete()?;
                }

                Ok(AsyncSink::Ready)
            }

            AsyncSink::NotReady(item) => Ok(AsyncSink::NotReady(item)),
        }
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        try_ready!(self.inner.poll_complete());

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
pub struct BatchSink<S, B, Request> {
    service: ServiceSink<S, Request>,
    batch: B,
    settings: BatchSettings,
    linger: Option<Delay>,
    closing: bool,
    _pd: PhantomData<Request>,
}

impl<S, B, Request> BatchSink<S, B, Request>
where
    S: Service<Request>,
    S::Future: Send + 'static,
    S::Error: Into<crate::Error> + Send + 'static,
    S::Response: fmt::Debug,
    B: Batch<Output = Request>,
{
    pub fn new(service: S, batch: B, settings: BatchSettings, acker: Acker) -> Self {
        let service = ServiceSink::new(service, acker);

        Self {
            service,
            batch,
            settings,
            linger: None,
            closing: false,
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

    fn poll_send(&mut self) -> Poll<(), crate::Error> {
        try_ready!(self.service.poll_ready());

        let fresh = self.batch.fresh();
        let batch = mem::replace(&mut self.batch, fresh);
        let batch_size = batch.num_items();
        let request = batch.finish();

        self.service.send(request, batch_size);

        self.linger.take();

        Ok(().into())
    }
}

impl<S, B, Request> Sink for BatchSink<S, B, Request>
where
    S: Service<Request>,
    S::Future: Send + 'static,
    S::Error: Into<crate::Error> + Send + 'static,
    S::Response: fmt::Debug,
    B: Batch<Output = Request>,
{
    type SinkItem = B::Input;
    type SinkError = crate::Error;

    // When used with Stream::forward, a successful call to start_send will always be followed
    // immediately by another call to start_send or a call to poll_complete. This means that
    // start_send only needs to concern itself with the case where we've hit our batch's capacity
    // and need to push it down to the inner sink. The other case, when our batch is not full but
    // we want to push it to the inner sink anyway, can be detected and handled by poll_complete.
    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        if self.batch.len() >= self.settings.size {
            self.poll_complete()?;

            if self.batch.len() > self.settings.size {
                debug!(message = "Buffer full; applying back pressure.", size = %self.settings.size, rate_limit_secs = 10);
                return Ok(AsyncSink::NotReady(item));
            }
        }

        if self.batch.len() == 0 {
            // We just inserted the first item of a new batch, so set our delay to the longest time
            // we want to allow that item to linger in the batch before being flushed.
            let deadline = Instant::now() + self.settings.timeout;
            self.linger = Some(Delay::new(deadline));
        }

        self.batch.push(item);

        Ok(AsyncSink::Ready)
    }

    // When used with Stream::forward, poll_complete will be called in a few different
    // circumstances:
    //
    //   1. internally via start_send when our batch is full
    //   2. externally from Forward when the stream returns NotReady
    //   3. internally via close from Forward when the stream returns Ready(None)
    //
    // In (1), we always want to attempt to push the current batch down into the inner sink.
    //
    // For (2), our behavior depends on configuration. If we have a minimum batch size that
    // hasn't yet been met, we'll want to wait for additional items before pushing the current
    // batch down. If there is no minimum or we've already met it, we will try to push the current
    // batch down. If the inner sink is not ready, we'll keep that batch and continue appending
    // to it.
    //
    // Finally, for (3), our behavior is essentially the same as for (2), except that we'll try to
    // send our existing batch whether or not it has met the minimum batch size.
    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        loop {
            if self.batch.is_empty() {
                return self.service.poll_complete();
            } else {
                // We have data to send, so check if we should send it and either attempt the send
                // or return that we're not ready to send. If we send and it works, loop to poll or
                // close inner instead of prematurely returning Ready
                if self.should_send() {
                    try_ready!(self.poll_send());
                } else {
                    return self.service.poll_complete();
                }
            }
        }
    }

    fn close(&mut self) -> Poll<(), Self::SinkError> {
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

// === ServiceSink ===

type ServiceFuture = Box<dyn Future<Item = (usize, usize), Error = crate::Error> + Send + 'static>;

struct ServiceSink<S, Request> {
    service: S,
    in_flight: FuturesUnordered<ServiceFuture>,
    acker: Acker,
    seq_head: usize,
    seq_tail: usize,
    pending_acks: HashMap<usize, usize>,
    _pd: PhantomData<Request>,
}

impl<S, Request> ServiceSink<S, Request>
where
    S: Service<Request>,
    S::Future: Send + 'static,
    S::Error: Into<crate::Error> + Send + 'static,
    S::Response: fmt::Debug,
{
    fn new(service: S, acker: Acker) -> Self {
        Self {
            service,
            in_flight: FuturesUnordered::new(),
            acker,
            seq_head: 0,
            seq_tail: 0,
            pending_acks: HashMap::new(),
            _pd: PhantomData,
        }
    }

    fn poll_ready(&mut self) -> Poll<(), crate::Error> {
        self.service.poll_ready().map_err(Into::into)
    }

    fn send(&mut self, req: Request, batch_size: usize) {
        let seqno = self.seq_head;
        self.seq_head += 1;

        // XXX: We could `tokio::spawn` here to allow the
        // service future to get fully driven to completion
        // on another worker thread.
        let response = self
            .service
            .call(req)
            .map(move |response| {
                trace!(message = "Response successful.", ?response);
                (seqno, batch_size)
            })
            .map_err(Into::into);

        self.in_flight.push(Box::new(response));
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
                    self.acker.ack(num_to_ack);
                }
                Err(error) => {
                    error!(
                        message = "Request failed.",
                        %error,
                    );
                    return Err(error);
                }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffers::Acker;
    use crate::sinks::util::{BatchSettings, Buffer};
    use futures01::{future, sync::oneshot, Sink};
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
        let mut clock = MockClock::new();

        let (acker, ack_counter) = Acker::new_for_testing();

        let svc = tower::service_fn(|_| future::ok::<_, std::io::Error>(()));
        let buffered = BatchSink::new(svc, Vec::new(), SETTINGS, acker);

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
        let mut clock = MockClock::new();

        let (acker, ack_counter) = Acker::new_for_testing();

        let oneshots = Arc::new(Mutex::new(vec![
            oneshot::channel(),
            oneshot::channel(),
            oneshot::channel(),
        ]));

        let svc = tower::service_fn(|_| future::ok::<_, std::io::Error>(()));

        let settings = BatchSettings {
            size: 1,
            ..SETTINGS
        };
        let sink = BatchSink::new(svc, Vec::new(), settings, acker);

        let _ = clock.enter(|_| {
            // buffered
            //     .sink_map_err(drop)
            //     .send_all(futures01::stream::iter_ok(0..22))
            //     .wait()
            //     .unwrap()
            let sink = sink.send(0).wait().unwrap();
        });

        assert_eq!(ack_counter.load(Relaxed), 22);
    }

    #[test]
    fn batch_sink_buffers_messages_until_limit() {
        let mut clock = MockClock::new();

        let (acker, _) = Acker::new_for_testing();
        let sent_requests = Arc::new(Mutex::new(Vec::new()));

        let svc = tower::service_fn(|req| {
            let sent_requests = sent_requests.clone();

            sent_requests.lock().unwrap().push(req);

            future::ok::<_, std::io::Error>(())
        });
        let buffered = BatchSink::new(svc, Vec::new(), SETTINGS, acker);

        let _ = clock.enter(|_| {
            buffered
                .sink_map_err(drop)
                .send_all(futures01::stream::iter_ok(0..22))
                .wait()
                .unwrap()
        });

        // TODO: test acking as well, this was not in the original
        // tests but since things are bundled together now we should
        // test that we correctly ack events.

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
        let mut clock = MockClock::new();

        let (acker, _) = Acker::new_for_testing();
        let sent_requests = Arc::new(Mutex::new(Vec::new()));

        let svc = tower::service_fn(|req| {
            let sent_requests = sent_requests.clone();

            sent_requests.lock().unwrap().push(req);

            future::ok::<_, std::io::Error>(())
        });
        let buffered = BatchSink::new(svc, Vec::new(), SETTINGS, acker);

        clock.enter(|_| {
            let buffered = buffered.send(0).wait().unwrap();
            let mut buffered = buffered.send(1).wait().unwrap();

            future::poll_fn(|| buffered.close()).wait().unwrap()
        });

        let output = sent_requests.lock().unwrap();
        assert_eq!(&*output, &vec![vec![0, 1]]);
    }

    #[test]
    fn batch_sink_expired_linger() {
        let mut clock = MockClock::new();

        let (acker, _) = Acker::new_for_testing();
        let sent_requests = Arc::new(Mutex::new(Vec::new()));

        let svc = tower::service_fn(|req| {
            let sent_requests = sent_requests.clone();

            sent_requests.lock().unwrap().push(req);

            future::ok::<_, std::io::Error>(())
        });
        let buffered = BatchSink::new(svc, Vec::new(), SETTINGS, acker);

        clock.enter(|handle| {
            let buffered = buffered.send(0).wait().unwrap();
            let mut buffered = buffered.send(1).wait().unwrap();

            // Move clock forward by linger timeout + 1 sec
            handle.advance(SETTINGS.timeout + Duration::from_secs(1));

            future::poll_fn(|| buffered.poll_complete()).wait().unwrap();
        });

        let output = sent_requests.lock().unwrap();
        assert_eq!(&*output, &vec![vec![0, 1]]);
    }

    #[test]
    fn batch_sink_allows_the_final_item_to_exceed_the_buffer_size() {
        let mut clock = MockClock::new();

        let (acker, _) = Acker::new_for_testing();
        let sent_requests = Arc::new(Mutex::new(Vec::new()));

        let svc = tower::service_fn(|req| {
            let sent_requests = sent_requests.clone();

            sent_requests.lock().unwrap().push(req);

            future::ok::<_, std::io::Error>(())
        });
        let buffered = BatchSink::new(svc, Buffer::new(false), SETTINGS, acker);

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
}
