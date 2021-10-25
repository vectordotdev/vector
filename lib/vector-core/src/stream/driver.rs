use crate::event::{EventStatus, Finalizable};
use buffers::{Ackable, Acker};
use futures::{poll, stream::FuturesUnordered, FutureExt, Stream, StreamExt, TryFutureExt};
use std::{
    collections::{BinaryHeap, VecDeque},
    fmt,
    task::Poll,
};
use tokio::{pin, select};
use tower::{Service, ServiceExt};
use tracing::Instrument;
use crate::internal_event::EventsSent;

#[derive(Eq)]
struct PendingAcknowledgement {
    seq_no: u64,
    ack_size: usize,
}

impl PartialEq for PendingAcknowledgement {
    fn eq(&self, other: &Self) -> bool {
        self.seq_no == other.seq_no
    }
}

impl PartialOrd for PendingAcknowledgement {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        // Reverse ordering so that in a `BinaryHeap`, the lowest sequence number is the highest priority.
        Some(other.seq_no.cmp(&self.seq_no))
    }
}

impl Ord for PendingAcknowledgement {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other
            .partial_cmp(self)
            .expect("PendingAcknowledgement should always return a valid comparison")
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
        let mut in_flight = FuturesUnordered::new();
        let mut pending_acks = BinaryHeap::new();
        let mut next_batch: Option<VecDeque<St::Item>> = None;

        let mut seq_head: u64 = 0;
        let mut seq_tail: u64 = 0;

        let Self {
            input,
            mut service,
            acker,
        } = self;

        let batched_input = input.ready_chunks(1024);
        pin!(batched_input);

        loop {
            // We'll poll up to 1024 in-flight futures, which lets us queue up multiple completions
            // in a single turn of the loop, while only doing a single call to `ack`, which is far more
            // efficient in scenarios where the `Driver` has a very high rate of input.
            //
            // Crucially, we aren't awaiting on the stream here, but simply calling its `poll_next`
            // method, which means we won't go to sleep if `in_flight` has no more ready futures and
            // there are no more incoming events.  Thus, we _still_ fall through to our normal
            // select below to ensure we're awaiting in a way that lets us go to sleep.  This manual
            // drain is just an optimized path for cases where the driver is managing tens or
            // hundreds of thousands of in-flight requests at a time, and needs to be a little more
            // efficient with the work it does each loop.
            let mut limit = 1024;
            while let Some(Some((seq_no, ack_size))) = in_flight.next().now_or_never() {
                trace!(message = "Sending request.", seq_no, ack_size);
                pending_acks.push(PendingAcknowledgement { seq_no, ack_size });

                limit -= 1;
                if limit == 0 {
                    break;
                }
            }

            let mut num_to_ack = 0;
            while let Some(pending_ack) = pending_acks.peek() {
                if pending_ack.seq_no == seq_tail {
                    let PendingAcknowledgement { ack_size, .. } = pending_acks
                        .pop()
                        .expect("should not be here unless pending_acks is non-empty");
                    num_to_ack += ack_size;
                    seq_tail += 1;
                } else {
                    break;
                }
            }

            if num_to_ack > 0 {
                trace!(message = "Acking events.", ack_size = num_to_ack);
                acker.ack(num_to_ack);
            }

            select! {
                // Using `biased` ensures we check the branches in the order they're written, and
                // the way they're ordered is to ensure that we're reacting to completed requests as
                // soon as possible to acknowledge them and make room for more requests to be processed.
                biased;

                // One of our service calls has completed.
                Some((seq_no, ack_size)) = in_flight.next() => {
                    trace!(message = "Sending request.", seq_no, ack_size);
                    pending_acks.push(PendingAcknowledgement { seq_no, ack_size });
                }

                // We've got an input batch to process.
                _ = async {}, if next_batch.is_some() => {
                    let mut batch = next_batch.take()
                        .expect("batch should be populated");

                    while !batch.is_empty() {
                        let svc = match poll!(service.ready()) {
                            Poll::Ready(Ok(svc)) => svc,
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
                        let seqno = seq_head;
                        seq_head += 1;

                        trace!(
                            message = "Submitting service request.",
                            in_flight_requests = in_flight.len()
                        );
                        let ack_size = req.ack_size();
                        let finalizers = req.take_finalizers();

                        let fut = svc.call(req)
                            .err_into()
                            .map(move |result: Result<Svc::Response, Svc::Error>| {
                                match result {
                                    Err(error) => {
                                        error!(message = "Service call failed.", ?error, seqno);
                                        finalizers.update_status(EventStatus::Failed);
                                    },
                                    Ok(response) => {
                                        trace!(message = "Service call succeeded.", seqno);
                                        finalizers.update_status(response.event_status());
                                        // emit
                                        //TODO: emit EventsSent
                                    }
                                };
                                (seqno, ack_size)
                            })
                            .instrument(info_span!("request", request_id = %seqno));

                        in_flight.push(fut);
                    }
                }

                // We've received some items from the input stream.
                Some(reqs) = batched_input.next() => {
                    let reqs = reqs;
                    next_batch = Some(reqs.into());
                }

                else => {
                    break
                }
            }
        }

        Ok(())
    }
}

pub trait DriverResponse {
    fn event_status(&self) -> EventStatus;
    fn events_sent(&self) -> EventsSent;
}
