pub mod batch;
pub mod buffer;
pub mod http;
pub mod retries;

use futures::{stream::FuturesUnordered, Async, AsyncSink, Poll, Sink, StartSend, Stream};
use log::{error, trace};
use std::time::Duration;
use tower_service::Service;

pub use buffer::Buffer;

pub trait SinkExt<B>
where
    B: batch::Batch,
    Self: Sink<SinkItem = B> + Sized,
{
    fn batched(self, batch: B, limit: usize) -> batch::BatchSink<B, Self> {
        batch::BatchSink::new(self, batch, limit)
    }

    fn batched_with_min(self, batch: B, min: usize, delay: Duration) -> batch::BatchSink<B, Self> {
        batch::BatchSink::new_min(self, batch, min, Some(delay))
    }
}

impl<B, S> SinkExt<B> for S
where
    B: batch::Batch,
    S: Sink<SinkItem = B> + Sized,
{
}

pub struct ServiceSink<T, S: Service<T>> {
    service: S,
    in_flight: FuturesUnordered<S::Future>,
    _phantom: std::marker::PhantomData<T>,
}

impl<T, S: Service<T>> ServiceSink<T, S> {
    pub fn new(service: S) -> Self {
        Self {
            service,
            in_flight: FuturesUnordered::new(),
            _phantom: std::marker::PhantomData,
        }
    }
}

type Error = Box<std::error::Error + 'static + Send + Sync>;

impl<T, S> Sink for ServiceSink<T, S>
where
    S: Service<T>,
    S::Error: Into<Error>,
    S::Response: std::fmt::Debug,
{
    type SinkItem = T;
    type SinkError = ();

    fn start_send(&mut self, item: T) -> StartSend<T, Self::SinkError> {
        let mut tried_once = false;
        loop {
            match self.service.poll_ready() {
                Ok(Async::Ready(())) => {
                    self.in_flight.push(self.service.call(item));
                    return Ok(AsyncSink::Ready);
                }

                Ok(Async::NotReady) => {
                    if tried_once {
                        return Ok(AsyncSink::NotReady(item));
                    } else {
                        self.poll_complete()?;
                        tried_once = true;
                    }
                }

                // TODO: figure out if/how to handle this
                Err(e) => panic!("service must be discarded: {}", e.into()),
            }
        }
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        loop {
            match self.in_flight.poll() {
                Ok(Async::NotReady) => return Ok(Async::NotReady),

                Ok(Async::Ready(None)) => return Ok(Async::Ready(())),

                Ok(Async::Ready(Some(response))) => trace!("request succeeded: {:?}", response),

                Err(e) => error!("request failed: {}", e.into()),
            }
        }
    }
}
