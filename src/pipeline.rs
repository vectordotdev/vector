use std::{collections::VecDeque, fmt, pin::Pin, task::Context};

use futures::{channel::mpsc, task::Poll, Sink};
#[cfg(test)]
use futures::{Stream, StreamExt};
#[cfg(test)]
use vector_core::event::EventStatus;
use vector_core::{event::Event, internal_event::EventsSent, ByteSizeOf};

#[derive(Debug)]
pub struct ClosedError;

impl fmt::Display for ClosedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Pipeline is closed.")
    }
}

impl std::error::Error for ClosedError {}

const MAX_ENQUEUED: usize = 1000;

#[derive(Derivative, Clone)]
#[derivative(Debug)]
pub struct Pipeline {
    inner: mpsc::Sender<Event>,
    enqueued: VecDeque<Event>,
}

impl Pipeline {
    fn try_flush(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), <Self as Sink<Event>>::Error>> {
        // We batch the updates to EventsSent for efficiency, and do it here because
        // it gives us a chance to allow the natural batching of `Pipeline` to kick in.
        let mut sent_count = 0;
        let mut sent_bytes = 0;

        while let Some(event) = self.enqueued.pop_front() {
            match self.inner.poll_ready(cx) {
                Poll::Pending => {
                    self.enqueued.push_front(event);
                    if sent_count > 0 {
                        emit!(&EventsSent {
                            count: sent_count,
                            byte_size: sent_bytes,
                        });
                    }
                    return Poll::Pending;
                }
                Poll::Ready(Ok(())) => {
                    // continue to send below
                }
                Poll::Ready(Err(_error)) => {
                    if sent_count > 0 {
                        emit!(&EventsSent {
                            count: sent_count,
                            byte_size: sent_bytes,
                        });
                    }
                    return Poll::Ready(Err(ClosedError));
                }
            }

            let event_bytes = event.size_of();
            match self.inner.start_send(event) {
                Ok(()) => {
                    // we good, keep looping
                    sent_count += 1;
                    sent_bytes += event_bytes;
                }
                Err(error) if error.is_full() => {
                    // We only try to send after a successful call to poll_ready, which reserves
                    // space for us in the channel. That makes this branch unreachable as long as
                    // the channel implementation fulfills its own contract.
                    panic!("Channel was both ready and full; this is a bug.")
                }
                Err(error) if error.is_disconnected() => {
                    if sent_count > 0 {
                        emit!(&EventsSent {
                            count: sent_count,
                            byte_size: sent_bytes,
                        });
                    }
                    return Poll::Ready(Err(ClosedError));
                }
                Err(_) => unreachable!(),
            }
        }
        if sent_count > 0 {
            emit!(&EventsSent {
                count: sent_count,
                byte_size: sent_bytes,
            });
        }
        Poll::Ready(Ok(()))
    }
}

impl Sink<Event> for Pipeline {
    type Error = ClosedError;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        if self.enqueued.len() < MAX_ENQUEUED {
            Poll::Ready(Ok(()))
        } else {
            self.try_flush(cx)
        }
    }

    fn start_send(mut self: Pin<&mut Self>, item: Event) -> Result<(), Self::Error> {
        self.enqueued.push_back(item);
        Ok(())
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.try_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.poll_flush(cx)
    }
}

impl Pipeline {
    #[cfg(test)]
    pub fn new_test() -> (Self, mpsc::Receiver<Event>) {
        Self::new_with_buffer(100)
    }

    #[cfg(test)]
    pub fn new_test_finalize(status: EventStatus) -> (Self, impl Stream<Item = Event> + Unpin) {
        let (pipe, recv) = Self::new_with_buffer(100);
        // In a source test pipeline, there is no sink to acknowledge
        // events, so we have to add a map to the receiver to handle the
        // finalization.
        let recv = recv.map(move |mut event| {
            let metadata = event.metadata_mut();
            metadata.update_status(status);
            metadata.update_sources();
            event
        });
        (pipe, recv)
    }

    pub fn new_with_buffer(n: usize) -> (Self, mpsc::Receiver<Event>) {
        let (tx, rx) = mpsc::channel(n);
        (Self::from_sender(tx), rx)
    }

    pub fn from_sender(inner: mpsc::Sender<Event>) -> Self {
        Self {
            inner,
            // We ensure the buffer is sufficient that it is unlikely to require reallocations.
            // There is a possibility a component might blow this queue size.
            enqueued: VecDeque::with_capacity(10),
        }
    }
}
