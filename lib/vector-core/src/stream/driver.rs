use std::{collections::HashMap, fmt};

use buffers::{Ackable, Acker};
use futures::{stream::FuturesUnordered, FutureExt, Stream, StreamExt, TryFutureExt};
use tokio::{pin, select, sync::oneshot};
use tower::{Service, ServiceExt};
use tracing::Instrument;

use crate::event::{EventStatus, Finalizable};

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
    /// No errors are currently returned.  Te return type is purely to simplify caller code, but may
    /// return an error for a legitimate reason in the future.
    pub async fn run(self) -> Result<(), ()> {
        let in_flight = FuturesUnordered::new();
        let mut pending_acks = HashMap::new();
        let mut seq_head: u64 = 0;
        let mut seq_tail: u64 = 0;

        let Self {
            input,
            mut service,
            acker,
            ..
        } = self;

        pin!(input);
        pin!(in_flight);

        loop {
            select! {
                // We've received an item from the input stream.
                Some(req) = input.next() => {
                    // Rebind the variable to avoid a bug with the pattern matching
                    // in `select!`: https://github.com/tokio-rs/tokio/issues/4076
                    let mut req = req;
                    let seqno = seq_head;
                    seq_head += 1;

                    let (tx, rx) = oneshot::channel();

                    in_flight.push(rx);

                    trace!(
                        message = "Submitting service request.",
                        in_flight_requests = in_flight.len()
                    );
                    let ack_size = req.ack_size();
                    let finalizers = req.take_finalizers();

                    let svc = service.ready().await.expect("should not get error when waiting for svc readiness");
                    let fut = svc.call(req)
                        .err_into()
                        .map(move |result: Result<Svc::Response, Svc::Error>| {
                            let status = match result {
                                Err(error) => {
                                    error!(message = "Service call failed.", ?error, seqno);
                                    EventStatus::Failed
                                },
                                Ok(response) => {
                                    trace!(message = "Service call succeeded.", seqno);
                                    *response.as_ref()
                                }
                            };
                            finalizers.update_status(status);

                            // The receiver could drop before we reach this point if Driver`
                            // goes away as part of a sink closing.  We can't do anything
                            // about it, so just silently ignore the error.
                            let _ = tx.send((seqno, ack_size));
                        })
                        .instrument(info_span!("request", request_id = %seqno));
                    tokio::spawn(fut);
                },

                // One of our service calls has completed.
                Some(Ok((seqno, ack_size))) = in_flight.next() => {
                    trace!(message = "Sending request.", seqno, ack_size);
                    pending_acks.insert(seqno, ack_size);

                    let mut num_to_ack = 0;
                    while let Some(ack_size) = pending_acks.remove(&seq_tail) {
                        num_to_ack += ack_size;
                        seq_tail += 1;
                    }

                    if num_to_ack > 0 {
                        trace!(message = "Acking events.", ack_size = num_to_ack);
                        acker.ack(num_to_ack);
                    }
                },

                else => break
            }
        }

        Ok(())
    }
}
