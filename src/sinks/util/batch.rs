use crate::record::Record;
use futures::{Async, AsyncSink, Future, Poll, Sink, StartSend};
use std::fmt;
use tower_service::Service;

pub struct BatchSink<S>
where
    S: Service<Vec<Vec<u8>>>,
{
    batcher: Vec<Vec<u8>>,
    service: S,
    state: State<S::Future>,
    size: usize,
}

enum State<T> {
    Poll(T),
    Batching,
}

impl<S> BatchSink<S>
where
    S: Service<Vec<Vec<u8>>>,
{
    pub fn new(service: S, size: usize) -> Self {
        BatchSink {
            batcher: Vec::new(),
            service,
            state: State::Batching,
            size,
        }
    }
}

impl<S> Sink for BatchSink<S>
where
    S: Service<Vec<Vec<u8>>>,
    S::Error: fmt::Debug,
    S::Response: fmt::Debug,
{
    type SinkItem = Record;
    type SinkError = ();

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        if self.batcher.len() > self.size {
            self.poll_complete()?;

            if self.batcher.len() > self.size {
                return Ok(AsyncSink::NotReady(item));
            }
        }

        self.batcher.push(item.into());

        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        loop {
            match self.state {
                State::Poll(ref mut fut) => match fut.poll() {
                    Ok(Async::Ready(_response)) => {
                        self.state = State::Batching;
                        continue;
                    }
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    Err(err) => panic!("Error sending request: {:?}", err),
                },

                State::Batching => {
                    if self.batcher.len() > self.size {
                        let items = self.batcher.drain(..).collect();
                        let fut = self.service.call(items);
                        self.state = State::Poll(fut);

                        continue;
                    } else {
                        // check timer here???
                        // Buffer isnt full and there isn't an inflight request
                        if !self.batcher.is_empty() {
                            // Buffer isnt empty, isnt full and there is no inflight
                            // so lets take the rest of the buffer and send it.
                            let items = self.batcher.drain(..).collect();
                            let fut = self.service.call(items);
                            self.state = State::Poll(fut);

                            continue;
                        } else {
                            return Ok(Async::Ready(()));
                        }
                    }
                }
            }
        }
    }
}
