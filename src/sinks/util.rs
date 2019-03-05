pub mod batch;
pub mod http;
pub mod retries;
pub mod size_buffered;

use futures::{stream::FuturesUnordered, Async, AsyncSink, Poll, Sink, StartSend, Stream};
use log::{error, trace};
use std::{error::Error, fmt};
use tower_service::Service;

pub trait SinkExt: Sink<SinkItem = Vec<u8>> + Sized {
    fn size_buffered(self, limit: usize, gzip: bool) -> size_buffered::SizeBuffered<Self> {
        size_buffered::SizeBuffered::new(self, limit, gzip)
    }
}

impl<S> SinkExt for S where S: Sink<SinkItem = Vec<u8>> + Sized {}

pub struct ServiceSink<T, S: Service<T>> {
    service: S,
    in_flight: FuturesUnordered<S::Future>,
    _phantom: std::marker::PhantomData<T>,
}

impl<T, S: Service<T>> ServiceSink<T, S> {
    fn new(service: S) -> Self {
        Self {
            service,
            in_flight: FuturesUnordered::new(),
            _phantom: std::marker::PhantomData,
        }
    }
}

type TowerError = Box<Error + 'static + Send + Sync>;

impl<T, S> Sink for ServiceSink<T, S>
where
    S: Service<T, Error = TowerError>,
    S::Response: fmt::Debug,
{
    type SinkItem = T;
    type SinkError = ();

    fn start_send(&mut self, item: T) -> StartSend<T, Self::SinkError> {
        match self.service.poll_ready() {
            Ok(Async::Ready(())) => {
                self.in_flight.push(self.service.call(item));
                Ok(AsyncSink::Ready)
            }

            Ok(Async::NotReady) => Ok(AsyncSink::NotReady(item)),

            // TODO: figure out if/how to handle this
            Err(e) => panic!("service must be discarded: {}", e),
        }
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        loop {
            match self.in_flight.poll() {
                Ok(Async::NotReady) => return Ok(Async::NotReady),

                Ok(Async::Ready(None)) => return Ok(Async::Ready(())),

                Ok(Async::Ready(Some(response))) => trace!("request succeeded: {:?}", response),

                Err(e) => error!("request failed: {}", e),
            }
        }
    }
}
