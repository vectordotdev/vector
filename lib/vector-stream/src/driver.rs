use std::{collections::VecDeque, fmt, future::poll_fn, task::Poll};

use futures::{poll, FutureExt, Stream, StreamExt, TryFutureExt};
use tokio::{pin, select};
use tower::Service;
use tracing::Instrument;
use vector_common::internal_event::emit;
use vector_common::internal_event::{
    register, ByteSize, BytesSent, CallError, InternalEventHandle as _, PollReadyError, Registered,
    RegisteredEventCache, SharedString, TaggedEventsSent,
};
use vector_common::request_metadata::{GroupedCountByteSize, MetaDescriptive};
use vector_core::event::{EventFinalizers, EventStatus, Finalizable};

use super::FuturesUnorderedCount;

pub trait DriverResponse {
    fn event_status(&self) -> EventStatus;
    fn events_sent(&self) -> &GroupedCountByteSize;

    /// Return the number of bytes that were sent in the request that returned this response.
    // TODO, remove the default implementation once all sinks have
    // implemented this function.
    fn bytes_sent(&self) -> Option<usize> {
        None
    }
}

/// Drives the interaction between a stream of items and a service which processes them
/// asynchronously.
///
/// `Driver`, as a high-level, facilitates taking items from an arbitrary `Stream` and pushing them
/// through a `Service`, spawning each call to the service so that work can be run concurrently,
/// managing waiting for the service to be ready before processing more items, and so on.
///
/// Additionally, `Driver` handles event finalization, which triggers acknowledgements
/// to the source or disk buffer.
///
/// This capability is parameterized so any implementation which can define how to interpret the
/// response for each request, as well as define how many events a request is compromised of, can be
/// used with `Driver`.
pub struct Driver<St, Svc> {
    input: St,
    service: Svc,
    protocol: Option<SharedString>,
}

impl<St, Svc> Driver<St, Svc> {
    pub fn new(input: St, service: Svc) -> Self {
        Self {
            input,
            service,
            protocol: None,
        }
    }

    /// Set the protocol name for this driver.
    ///
    /// If this is set, the driver will fetch and use the `bytes_sent` value from responses in a
    /// `BytesSent` event.
    #[must_use]
    pub fn protocol(mut self, protocol: impl Into<SharedString>) -> Self {
        self.protocol = Some(protocol.into());
        self
    }
}

impl<St, Svc> Driver<St, Svc>
where
    St: Stream,
    St::Item: Finalizable + MetaDescriptive,
    Svc: Service<St::Item>,
    Svc::Error: fmt::Debug + 'static,
    Svc::Future: Send + 'static,
    Svc::Response: DriverResponse,
{
    /// Runs the driver until the input stream is exhausted.
    ///
    /// All in-flight calls to the provided `service` will also be completed before `run` returns.
    ///
    /// # Errors
    ///
    /// The return type is mostly to simplify caller code.
    /// An error is currently only returned if a service returns an error from `poll_ready`
    pub async fn run(self) -> Result<(), ()> {
        let mut in_flight = FuturesUnorderedCount::new();
        let mut next_batch: Option<VecDeque<St::Item>> = None;
        let mut seq_num = 0usize;

        let Self {
            input,
            mut service,
            protocol,
        } = self;

        let batched_input = input.ready_chunks(1024);
        pin!(batched_input);

        let bytes_sent = protocol.map(|protocol| register(BytesSent { protocol }));
        let events_sent = RegisteredEventCache::new(());

        loop {
            // Core behavior of the loop:
            // - always check to see if we have any response futures that have completed
            //  -- if so, handling acking as many events as we can (ordering matters)
            // - if we have a "current" batch, try to send each request in it to the service
            //   -- if we can't drain all requests from the batch due to lack of service readiness,
            //   then put the batch back and try to send the rest of it when the service is ready
            //   again
            // - if we have no "current" batch, but there is an available batch from our input
            //   stream, grab that batch and store it as our current batch
            //
            // Essentially, we bounce back and forth between "grab the new batch from the input
            // stream" and "send all requests in the batch to our service" which _could be trivially
            // modeled with a normal imperative loop.  However, we want to be able to interleave the
            // acknowledgement of responses to allow buffers and sources to continue making forward
            // progress, which necessitates a more complex weaving of logic.  Using `select!` is
            // more code, and requires a more careful eye than blindly doing
            // "get_next_batch().await; process_batch().await", but it does make doing the complex
            // logic easier than if we tried to interleave it ourselves with an imperative-style loop.

            select! {
                // Using `biased` ensures we check the branches in the order they're written, since
                // the default behavior of the `select!` macro is to randomly order branches as a
                // means of ensuring scheduling fairness.
                biased;

                // One or more of our service calls have completed.
                Some(_count) = in_flight.next(), if !in_flight.is_empty() => {}

                // We've got an input batch to process and the service is ready to accept a request.
                maybe_ready = poll_fn(|cx| service.poll_ready(cx)), if next_batch.is_some() => {
                    let mut batch = next_batch.take()
                        .unwrap_or_else(|| unreachable!("batch should be populated"));

                    let mut maybe_ready = Some(maybe_ready);
                    while !batch.is_empty() {
                        // Make sure the service is ready to take another request.
                        let maybe_ready = match maybe_ready.take() {
                            Some(ready) => Poll::Ready(ready),
                            None => poll!(poll_fn(|cx| service.poll_ready(cx))),
                        };

                        let svc = match maybe_ready {
                            Poll::Ready(Ok(())) => &mut service,
                            Poll::Ready(Err(error)) => {
                                emit(PollReadyError{ error });
                                return Err(())
                            }
                            Poll::Pending => {
                                next_batch = Some(batch);
                                break
                            },
                        };

                        let mut req = batch.pop_front().unwrap_or_else(|| unreachable!("batch should not be empty"));
                        seq_num += 1;
                        let request_id = seq_num;

                        trace!(
                            message = "Submitting service request.",
                            in_flight_requests = in_flight.len(),
                            request_id,
                        );
                        let finalizers = req.take_finalizers();
                        let bytes_sent = bytes_sent.clone();
                        let events_sent = events_sent.clone();
                        let event_count = req.get_metadata().event_count();

                        let fut = svc.call(req)
                            .err_into()
                            .map(move |result| Self::handle_response(
                                result,
                                request_id,
                                finalizers,
                                event_count,
                                &bytes_sent,
                                &events_sent,
                            ))
                            .instrument(info_span!("request", request_id).or_current());

                        in_flight.push(fut);
                    }
                }

                // We've received some items from the input stream.
                Some(reqs) = batched_input.next(), if next_batch.is_none() => {
                    next_batch = Some(reqs.into());
                }

                else => break
            }
        }

        Ok(())
    }

    fn handle_response(
        result: Result<Svc::Response, Svc::Error>,
        request_id: usize,
        finalizers: EventFinalizers,
        event_count: usize,
        bytes_sent: &Option<Registered<BytesSent>>,
        events_sent: &RegisteredEventCache<(), TaggedEventsSent>,
    ) {
        match result {
            Err(error) => {
                Self::emit_call_error(Some(error), request_id, event_count);
                finalizers.update_status(EventStatus::Rejected);
            }
            Ok(response) => {
                trace!(message = "Service call succeeded.", request_id);
                finalizers.update_status(response.event_status());
                if response.event_status() == EventStatus::Delivered {
                    if let Some(bytes_sent) = bytes_sent {
                        if let Some(byte_size) = response.bytes_sent() {
                            bytes_sent.emit(ByteSize(byte_size));
                        }
                    }

                    response.events_sent().emit_event(events_sent);

                // This condition occurs specifically when the `HttpBatchService::call()` is called *within* the `Service::call()`
                } else if response.event_status() == EventStatus::Rejected {
                    Self::emit_call_error(None, request_id, event_count);
                    finalizers.update_status(EventStatus::Rejected);
                }
            }
        };
        drop(finalizers); // suppress "argument not consumed" warning
    }

    /// Emit the `Error` and `EventsDropped` internal events.
    /// This scenario occurs after retries have been attempted.
    fn emit_call_error(error: Option<Svc::Error>, request_id: usize, count: usize) {
        emit(CallError {
            error,
            request_id,
            count,
        });
    }
}

#[cfg(test)]
mod tests {
    use std::{
        future::Future,
        pin::Pin,
        sync::{atomic::AtomicUsize, atomic::Ordering, Arc},
        task::{ready, Context, Poll},
        time::Duration,
    };

    use futures_util::stream;
    use rand::{prelude::StdRng, SeedableRng};
    use rand_distr::{Distribution, Pareto};
    use tokio::{
        sync::{OwnedSemaphorePermit, Semaphore},
        time::sleep,
    };
    use tokio_util::sync::PollSemaphore;
    use tower::Service;
    use vector_common::{
        finalization::{BatchNotifier, EventFinalizer, EventFinalizers, EventStatus, Finalizable},
        json_size::JsonSize,
        request_metadata::{GroupedCountByteSize, RequestMetadata},
    };
    use vector_common::{internal_event::CountByteSize, request_metadata::MetaDescriptive};

    use super::{Driver, DriverResponse};

    type Counter = Arc<AtomicUsize>;

    #[derive(Debug)]
    struct DelayRequest(EventFinalizers, RequestMetadata);

    impl DelayRequest {
        fn new(value: usize, counter: &Counter) -> Self {
            let (batch, receiver) = BatchNotifier::new_with_receiver();
            let counter = Arc::clone(counter);
            tokio::spawn(async move {
                receiver.await;
                counter.fetch_add(value, Ordering::Relaxed);
            });
            Self(
                EventFinalizers::new(EventFinalizer::new(batch)),
                RequestMetadata::default(),
            )
        }
    }

    impl Finalizable for DelayRequest {
        fn take_finalizers(&mut self) -> vector_core::event::EventFinalizers {
            std::mem::take(&mut self.0)
        }
    }

    impl MetaDescriptive for DelayRequest {
        fn get_metadata(&self) -> &RequestMetadata {
            &self.1
        }

        fn metadata_mut(&mut self) -> &mut RequestMetadata {
            &mut self.1
        }
    }

    struct DelayResponse {
        events_sent: GroupedCountByteSize,
    }

    impl DelayResponse {
        fn new() -> Self {
            Self {
                events_sent: CountByteSize(1, JsonSize::new(1)).into(),
            }
        }
    }

    impl DriverResponse for DelayResponse {
        fn event_status(&self) -> EventStatus {
            EventStatus::Delivered
        }

        fn events_sent(&self) -> &GroupedCountByteSize {
            &self.events_sent
        }
    }

    // Generic service that takes a usize and applies an arbitrary delay to returning it.
    struct DelayService {
        semaphore: PollSemaphore,
        permit: Option<OwnedSemaphorePermit>,
        jitter: Pareto<f64>,
        jitter_gen: StdRng,
        lower_bound_us: u64,
        upper_bound_us: u64,
    }

    // Clippy is unhappy about all of our f64/u64 shuffling.  We don't actually care about losing
    // the fractional part of 20,459.13142 or whatever.  It just doesn't matter for this test.
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_precision_loss)]
    impl DelayService {
        pub(crate) fn new(permits: usize, lower_bound: Duration, upper_bound: Duration) -> Self {
            assert!(upper_bound > lower_bound);
            Self {
                semaphore: PollSemaphore::new(Arc::new(Semaphore::new(permits))),
                permit: None,
                jitter: Pareto::new(1.0, 1.0).expect("distribution should be valid"),
                jitter_gen: StdRng::from_seed([
                    3, 1, 4, 1, 5, 9, 2, 6, 5, 3, 5, 8, 9, 7, 9, 3, 2, 3, 8, 4, 6, 2, 6, 4, 3, 3,
                    8, 3, 2, 7, 9, 5,
                ]),
                lower_bound_us: lower_bound.as_micros().max(10_000) as u64,
                upper_bound_us: upper_bound.as_micros().max(10_000) as u64,
            }
        }

        pub(crate) fn get_sleep_dur(&mut self) -> Duration {
            let lower = self.lower_bound_us;
            let upper = self.upper_bound_us;

            // Generate a value between 10ms and 500ms, with a long tail shape to the distribution.
            #[allow(clippy::cast_sign_loss)] // Value will be positive anyways
            self.jitter
                .sample_iter(&mut self.jitter_gen)
                .map(|n| n * lower as f64)
                .map(|n| n as u64)
                .filter(|n| *n > lower && *n < upper)
                .map(Duration::from_micros)
                .next()
                .expect("jitter iter should be endless")
        }
    }

    impl Service<DelayRequest> for DelayService {
        type Response = DelayResponse;
        type Error = ();
        type Future =
            Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + Sync>>;

        fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            assert!(
                self.permit.is_none(),
                "should not call poll_ready again after a successful call"
            );

            match ready!(self.semaphore.poll_acquire(cx)) {
                None => panic!("semaphore should not be closed!"),
                Some(permit) => assert!(self.permit.replace(permit).is_none()),
            }

            Poll::Ready(Ok(()))
        }

        fn call(&mut self, req: DelayRequest) -> Self::Future {
            let permit = self
                .permit
                .take()
                .expect("calling `call` without successful `poll_ready` is invalid");
            let sleep_dur = self.get_sleep_dur();

            Box::pin(async move {
                sleep(sleep_dur).await;

                // Manually drop our permit here so that we take ownership and then actually
                // release the slot back to the semaphore.
                drop(permit);
                drop(req);

                Ok(DelayResponse::new())
            })
        }
    }

    #[tokio::test]
    async fn driver_simple() {
        // This test uses a service which creates response futures that sleep for a variable, but
        // bounded, amount of time, giving the impression of work being completed.  Completion of
        // all requests/responses is asserted by checking that the shared counter matches the
        // expected ack amount.  The delays themselves are deterministic based on a fixed-seed
        // RNG, so the test should always run in a fairly constant time between runs.
        //
        // TODO: Given the use of a deterministic RNG, we could likely transition this test to be
        // driven via `proptest`, to also allow driving the input requests.  The main thing that we
        // do not control is the arrival of requests in the input stream itself, which means that
        // the generated batches will almost always be the biggest possible size, since the stream
        // is always immediately available.
        //
        // It might be possible to spawn a background task to drive a true MPSC channel with
        // requests based on input provided from `proptest` to control not only the value (which
        // determines ack size) but the delay between messages, as well... simulating delays between
        // bursts of messages, similar to real sources.

        let counter = Counter::default();

        // Set up our driver input stream, service, etc.
        let input_requests = (1..=2048).collect::<Vec<_>>();
        let input_total: usize = input_requests.iter().sum();
        let input_stream = stream::iter(
            input_requests
                .into_iter()
                .map(|i| DelayRequest::new(i, &counter)),
        );
        let service = DelayService::new(10, Duration::from_millis(5), Duration::from_millis(150));
        let driver = Driver::new(input_stream, service);

        // Now actually run the driver, consuming all of the input.
        assert_eq!(driver.run().await, Ok(()));
        // Make sure the final finalizer task runs.
        tokio::task::yield_now().await;
        assert_eq!(input_total, counter.load(Ordering::SeqCst));
    }
}
