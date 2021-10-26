use super::FuturesUnorderedChunked;
use crate::event::{EventStatus, Finalizable};
use buffers::{Ackable, Acker};
use futures::{poll, FutureExt, Stream, StreamExt, TryFutureExt};
use futures_util::future::poll_fn;
use std::{
    collections::{BinaryHeap, VecDeque},
    fmt,
    num::NonZeroUsize,
    task::Poll,
};
use tokio::{pin, select};
use tower::Service;
use tracing::Instrument;

#[derive(Eq)]
struct PendingAcknowledgement {
    seq_num: u64,
    ack_size: usize,
}

impl PartialEq for PendingAcknowledgement {
    fn eq(&self, other: &Self) -> bool {
        self.seq_num == other.seq_num
    }
}

impl PartialOrd for PendingAcknowledgement {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        // Reverse ordering so that in a `BinaryHeap`, the lowest sequence number is the highest priority.
        Some(other.seq_num.cmp(&self.seq_num))
    }
}

impl Ord for PendingAcknowledgement {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other
            .partial_cmp(self)
            .expect("PendingAcknowledgement should always return a valid comparison")
    }
}

#[derive(Default)]
struct AcknowledgementTracker {
    out_of_order: BinaryHeap<PendingAcknowledgement>,
    seq_head: u64,
    seq_tail: u64,
    ack_depth: usize,
}

impl AcknowledgementTracker {
    /// Acquires the next available sequence number.
    pub fn get_next_seq_num(&mut self) -> u64 {
        let seq_num = self.seq_head;
        self.seq_head += 1;
        seq_num
    }

    /// Marks the given sequence number as complete.
    pub fn mark_seq_num_complete(&mut self, seq_num: u64, ack_size: usize) {
        assert!(seq_num <= self.seq_head);
        assert!(seq_num >= self.seq_tail);
        if seq_num == self.seq_tail {
            self.ack_depth += ack_size;
            self.seq_tail += 1;
        } else {
            self.out_of_order
                .push(PendingAcknowledgement { seq_num, ack_size });
        }
    }

    /// Gets the acknowledgement "depth" based on previously marked sequence numbers.
    ///
    /// When a sequence number is marked as complete, we track its acknowledgement size.  The
    /// acknowledgement size is accumulated internally for all in-order completions.  If we've
    /// accumulated a non-zero acknowledgement depth, it is returned here and the depth is reset.
    /// Otherwise, `None` is returned.
    pub fn get_latest_ack_depth(&mut self) -> Option<NonZeroUsize> {
        // Drain any out-of-order acknowledgements that can now be ordered correctly.
        while let Some(ack) = self.out_of_order.peek() {
            if ack.seq_num == self.seq_tail {
                let PendingAcknowledgement { ack_size, .. } = self
                    .out_of_order
                    .pop()
                    .expect("should not be here unless self.out_of_order is non-empty");
                self.ack_depth += ack_size;
                self.seq_tail += 1;
            } else {
                break;
            }
        }

        match self.ack_depth {
            0 => None,
            n => {
                self.ack_depth = 0;
                NonZeroUsize::new(n)
            }
        }
    }
}

/// Drives the interaction between a stream of items and a service which processes them
/// asynchronously.
///
/// `Driver`, as a high-level, facilitates taking items from an arbitrary `Stream` and pushing them
/// through a `Service`, spawning each call to the service so that work can be run concurrently,
/// managing waiting for the service to be ready before processing more items, and so on.
///
/// Additionally, `Driver` handles two event-specific facilities: finalization and acknowledgement.
///
/// This capability is parameterized so any implementation which can define how to interpret the
/// response for each request, as well as define how many events a request is compromised of, can be
/// used with `Driver`.
pub struct Driver<St, Svc> {
    input: St,
    service: Svc,
    acker: Acker,
}

impl<St, Svc> Driver<St, Svc> {
    pub fn new(input: St, service: Svc, acker: Acker) -> Self {
        Self {
            input,
            service,
            acker,
        }
    }
}

impl<St, Svc> Driver<St, Svc>
where
    St: Stream,
    St::Item: Ackable + Finalizable,
    Svc: Service<St::Item>,
    Svc::Error: fmt::Debug + 'static,
    Svc::Future: Send + 'static,
    Svc::Response: AsRef<EventStatus>,
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
        let mut in_flight = FuturesUnorderedChunked::new(1024);
        let mut ack_tracker = AcknowledgementTracker::default();
        let mut next_batch: Option<VecDeque<St::Item>> = None;

        let Self {
            input,
            mut service,
            acker,
        } = self;

        let batched_input = input.ready_chunks(1024);
        pin!(batched_input);

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
            // modeled with a normal imperative loop.  However, we wantr to be able to interleave the
            // acknowledgement of responses to allow buffers and sources to continue making forward
            // progress, which necessitates a more complex weaving of logic.  Using `select!` is
            // more code, and requires a more careful eye than blindly doing
            // "get_next_batch().await; process_batch().await", but it does make doing the complex
            // logic easier than if we tried to interleave it ourselves witgh an imperative-style loop.

            select! {
                // Using `biased` ensures we check the branches in the order they're written, since
                // the default behavior of the `select!` macro is to randomly order branches as a
                // means of ensuring scheduling fairness.
                biased;

                // One or more of our service calls have completed.
                Some(acks) = in_flight.next(), if !in_flight.is_empty() => {
                    for (seq_num, ack_size) in acks {
                        trace!(message = "Sending request.", seq_num, ack_size);
                        ack_tracker.mark_seq_num_complete(seq_num, ack_size);
                    }

                    if let Some(ack_depth) = ack_tracker.get_latest_ack_depth() {
                        trace!(message = "Acking events.", ack_size = ack_depth);
                        acker.ack(ack_depth.get());
                    }
                }

                // We've got an input batch to process and the service is ready to accept a request.
                maybe_ready = poll_fn(|cx| service.poll_ready(cx)), if next_batch.is_some() => {
                    let mut batch = next_batch.take()
                        .expect("batch should be populated");

                    let mut maybe_ready = Some(maybe_ready);
                    while !batch.is_empty() {
                        // Make sure the service is ready to take another request.
                        let maybe_ready = match maybe_ready.take() {
                            Some(ready) => Poll::Ready(ready),
                            None => poll!(poll_fn(|cx| service.poll_ready(cx))),
                        };

                        let svc = match maybe_ready {
                            Poll::Ready(Ok(())) => &mut service,
                            Poll::Ready(Err(err)) => {
                                error!(message = "Service return error from `poll_ready()`.", ?err);
                                return Err(())
                            }
                            Poll::Pending => {
                                next_batch = Some(batch);
                                break
                            },
                        };

                        let mut req = batch.pop_front().expect("batch should not be empty");
                        let seq_num = ack_tracker.get_next_seq_num();

                        trace!(
                            message = "Submitting service request.",
                            in_flight_requests = in_flight.len()
                        );
                        let ack_size = req.ack_size();
                        let finalizers = req.take_finalizers();

                        let fut = svc.call(req)
                            .err_into()
                            .map(move |result: Result<Svc::Response, Svc::Error>| {
                                let status = match result {
                                    Err(error) => {
                                        error!(message = "Service call failed.", ?error, seq_num);
                                        EventStatus::Failed
                                    },
                                    Ok(response) => {
                                        trace!(message = "Service call succeeded.", seq_num);
                                        *response.as_ref()
                                    }
                                };
                                finalizers.update_status(status);
                                (seq_num, ack_size)
                            })
                            .instrument(info_span!("request", request_id = %seq_num));

                        in_flight.push(fut);
                    }
                }

                // We've received some items from the input stream.
                Some(reqs) = batched_input.next(), if next_batch.is_none() => {
                    let reqs = reqs;
                    next_batch = Some(reqs.into());
                }

                else => break
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{
        future::Future,
        num::NonZeroUsize,
        pin::Pin,
        sync::atomic::Ordering,
        sync::Arc,
        task::{Context, Poll},
        time::Duration,
    };

    use buffers::{Ackable, Acker};
    use futures_util::{ready, stream};
    use proptest::{collection::vec_deque, prop_assert_eq, proptest, strategy::Strategy};
    use rand::{prelude::StdRng, SeedableRng};
    use rand_distr::{Distribution, Pareto};
    use tokio::{
        sync::{OwnedSemaphorePermit, Semaphore},
        time::sleep,
    };
    use tokio_util::sync::PollSemaphore;
    use tower::Service;

    use crate::{
        event::{EventFinalizers, EventStatus, Finalizable},
        stream::driver::AcknowledgementTracker,
    };

    use super::Driver;

    struct DelayRequest(usize);

    impl Ackable for DelayRequest {
        fn ack_size(&self) -> usize {
            self.0
        }
    }

    impl Finalizable for DelayRequest {
        fn take_finalizers(&mut self) -> crate::event::EventFinalizers {
            EventFinalizers::default()
        }
    }

    struct DelayResponse;

    impl AsRef<EventStatus> for DelayResponse {
        fn as_ref(&self) -> &EventStatus {
            &EventStatus::Delivered
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

    impl DelayService {
        pub fn new(permits: usize, lower_bound: Duration, upper_bound: Duration) -> Self {
            assert!(upper_bound > lower_bound);
            Self {
                semaphore: PollSemaphore::new(Arc::new(Semaphore::new(permits))),
                permit: None,
                jitter: Pareto::new(1.0, 1.0).expect("distribution should be valid"),
                jitter_gen: StdRng::from_seed([
                    3, 1, 4, 1, 5, 9, 2, 6, 5, 3, 5, 8, 9, 7, 9, 3, 2, 3, 8, 4, 6, 2, 6, 4, 3, 3,
                    8, 3, 2, 7, 9, 5,
                ]),
                lower_bound_us: lower_bound.as_micros().min(10_000) as u64,
                upper_bound_us: upper_bound.as_micros().max(10_000) as u64,
            }
        }

        pub fn get_sleep_dur(&mut self) -> Duration {
            let lower = self.lower_bound_us;
            let upper = self.upper_bound_us;

            // Generate a value between 10ms and 500ms, with a long tail shape to the distribution.
            self.jitter
                .sample_iter(&mut self.jitter_gen)
                .map(|n| n * lower as f64)
                .map(|n| n as u64)
                .filter(|n| *n > lower && *n < upper)
                .map(|n| Duration::from_micros(n))
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
            if self.permit.is_some() {
                panic!("should not call poll_ready again after a successful call");
            }

            match ready!(self.semaphore.poll_acquire(cx)) {
                None => panic!("semaphore should not be closed!"),
                Some(permit) => assert!(self.permit.replace(permit).is_none()),
            }

            Poll::Ready(Ok(()))
        }

        fn call(&mut self, _: DelayRequest) -> Self::Future {
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

                Ok(DelayResponse)
            })
        }
    }

    fn arb_shuffled_seq_nums<S>(selector: S) -> impl Strategy<Value = Vec<u64>>
    where
        S: Strategy<Value = usize>,
    {
        selector
            .prop_map(|len| (0..len).into_iter().map(|n| n as u64).collect())
            .no_shrink()
            .prop_shuffle()
    }

    #[test]
    fn acknowledgement_tracker_simple() {
        let mut ack_tracker = AcknowledgementTracker::default();

        assert_eq!(ack_tracker.get_latest_ack_depth(), None);

        let seq_num1 = ack_tracker.get_next_seq_num();
        ack_tracker.mark_seq_num_complete(seq_num1, 42);

        assert_eq!(ack_tracker.get_latest_ack_depth(), NonZeroUsize::new(42));
        assert_eq!(ack_tracker.get_latest_ack_depth(), None);

        let seq_num2 = ack_tracker.get_next_seq_num();
        let seq_num3 = ack_tracker.get_next_seq_num();
        ack_tracker.mark_seq_num_complete(seq_num3, 314);
        assert_eq!(ack_tracker.get_latest_ack_depth(), None);

        ack_tracker.mark_seq_num_complete(seq_num2, 86);
        assert_eq!(ack_tracker.get_latest_ack_depth(), NonZeroUsize::new(400));
    }

    proptest! {
        #[test]
        fn acknowledgement_tracker_gauntlet(
            mut seq_ack_order in arb_shuffled_seq_nums(0..1000usize),
            // TODO: We ensure we have the same number of batch size values as we do sequence
            // numbers to acknowledge, but since the batch size could be 0, we could theoretically
            // have all zeroes and never actually mark any sequence numbers complete.  I tried
            // finding a component for "deterministically seeded infinite integer iterator" but
            // `proptest` doesn't seem to have one.  Might be worth writing one, but it would also
            // be hard(er), maybe not possible at all, to shrink.... which feels important.  Maybe not?
            mut ack_batch_size in vec_deque(0..100, 1000..=1000),
        ) {
            // We get the sequence numbers that we should acknowledge in randomized order, so we
            // have to call `get_next_seq_num` as many times as the length of that vector to ensure
            // `seq_head` is configured correctly.  Additionally, we also get a "batch size" which
            // is how many items we should ack from `seq_ack_order` before getting the latest ack depth.
            //
            // We also use the sequence number as the acknowledge size, which we check at the very
            // end to ensure we got the expected total.
            let mut ack_tracker = AcknowledgementTracker::default();
            let mut total_ack_depth = 0;
            let expected_total_ack_depth: usize = seq_ack_order.iter().map(|n| *n as usize).sum();
            let mut order_drain = seq_ack_order.drain(..);

            // Prime our tracker by grabbing as many sequence numbers as we have items in
            // `seq_ack_order`.  This ensures the internal state is correct.
            for _ in 0..order_drain.len() {
                let _ = ack_tracker.get_next_seq_num();
            }

            // Now start acknowledging sequence numbers.  We do this in variable-sized chunks, based
            // on `ack_batch_size`, and get the ack depth at the end of the every batch,
            // accumulating it as part of the totle.
            while order_drain.len() > 0 {
                let batch_size = ack_batch_size.pop_front().expect("should always have enough batch size values");
                for _ in 0..batch_size {
                    match order_drain.next() {
                        None => break,
                        Some(seq_num) => ack_tracker.mark_seq_num_complete(seq_num, seq_num as usize),
                    }
                }

                if let Some(ack_depth) = ack_tracker.get_latest_ack_depth() {
                    total_ack_depth += ack_depth.get();
                }
            }

            prop_assert_eq!(expected_total_ack_depth, total_ack_depth);
        }
    }

    #[tokio::test]
    async fn driver_simple() {
        // This test uses a service which creates response futures that sleep for a variable, but
        // bounded, amount of time, giving the impression of work being completed.  Completion of
        // all requests/responses is asserted by checking that the counter used by the acker matches
        // the expected ack amount.  The delays themselves are deterministic based on a fixed-seed
        // RNG, so the test should always run in a fairly constant time between runs.
        //
        // TODO: Given the use of a deterministic RNG, we could likely transition this test to be
        // driven via `proptest`, to also allow driving the the input requests.  The main thing that
        // we do not control is the arrival of requests in the input stream itself, which means that
        // the generated batches will almost always be the biggest possible size, since the stream
        // is always immediately available.
        //
        // It might be possible to spawn a background task to drive a true MPSC channel with
        // requesats based on input provided from `proptest` to control not only the value (which
        // determines ack size) but the delay between messages, as well... simulating delays between
        // bursts of messages, similar to real sources.

        // Set up our driver input stream, service, etc.
        let input_requests = (0..2048usize).into_iter().collect::<Vec<_>>();
        let input_total: usize = input_requests.iter().sum();
        let input_stream = stream::iter(input_requests.into_iter().map(DelayRequest));
        let service = DelayService::new(10, Duration::from_millis(10), Duration::from_millis(175));
        let (acker, counter) = Acker::new_for_testing();
        let driver = Driver::new(input_stream, service, acker);

        // Now actually run the driver, consuming all of the input.
        if let Err(()) = driver.run().await {
            panic!("driver unexpectedly returned with error!");
        }

        assert_eq!(input_total, counter.load(Ordering::SeqCst));
    }
}
