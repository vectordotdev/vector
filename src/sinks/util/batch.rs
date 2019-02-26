use crate::record::Record;
use futures::{Async, AsyncSink, Future, Poll, Sink, StartSend};
use std::fmt;
use tower_service::Service;

pub struct BatchSink<S, B>
where
    B: Batch,
    S: Service<Vec<B::Item>>,
{
    batcher: B,
    service: S,
    state: State<S::Future>,
}

enum State<T> {
    Poll(T),
    Batching,
}

impl<S, B> BatchSink<S, B>
where
    B: Batch,
    S: Service<Vec<B::Item>>,
{
    pub fn new(batcher: B, service: S) -> Self {
        BatchSink {
            batcher,
            service,
            state: State::Batching,
        }
    }
}

impl<S, B> Sink for BatchSink<S, B>
where
    B: Batch,
    B::Item: From<Record>,
    S: Service<Vec<B::Item>>,
    S::Error: fmt::Debug,
{
    type SinkItem = Record;
    type SinkError = ();

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        if self.batcher.full() {
            self.poll_complete()?;

            if self.batcher.full() {
                return Ok(AsyncSink::NotReady(item));
            }
        }

        self.batcher.push(item.into());

        if self.batcher.full() {
            self.poll_complete()?;
        }

        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        loop {
            match self.state {
                State::Poll(ref mut fut) => match fut.poll() {
                    Ok(Async::Ready(_)) => {
                        self.state = State::Batching;
                        continue;
                    }
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    Err(err) => panic!("Error sending request: {:?}", err),
                },

                State::Batching => {
                    if self.batcher.full() {
                        let items = self.batcher.flush();
                        let fut = self.service.call(items);
                        self.state = State::Poll(fut);

                        continue;
                    } else {
                        // check timer here???
                        // Buffer isnt full and there isn't an inflight request
                        if !self.batcher.empty() {
                            // Buffer isnt empty, isnt full and there is no inflight
                            // so lets take the rest of the buffer and send it.
                            let items = self.batcher.flush();
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

pub trait Batch {
    type Item;

    fn push(&mut self, item: Self::Item);

    fn flush(&mut self) -> Vec<Self::Item>;

    fn full(&self) -> bool;

    fn empty(&self) -> bool;
}

pub struct VecBatcher<T> {
    inner: Vec<T>,
    size: usize,
}

impl<T> VecBatcher<T> {
    pub fn new(size: usize) -> Self {
        VecBatcher {
            inner: Vec::new(),
            size,
        }
    }
}

impl<T> Batch for VecBatcher<T> {
    type Item = T;

    fn full(&self) -> bool {
        self.inner.len() >= self.size
    }

    fn push(&mut self, item: T) {
        self.inner.push(item);
    }

    fn flush(&mut self) -> Vec<T> {
        // TODO(lucio): make this unsafe replace?
        self.inner.drain(..).collect()
    }

    fn empty(&self) -> bool {
        self.inner.is_empty()
    }
}
