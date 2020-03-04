use futures::{Async, Future, IntoFuture, Poll, Stream};

/// A stream combinator which takes elements from a stream until a future resolves.
///
/// This structure is produced by the [`StreamExt::take_until`] method.
#[derive(Clone, Debug)]
pub struct TakeUntil<S, F> {
    stream: S,
    until: F,
    free: bool,
}

/// This `Stream` extension trait provides a `take_until` method that terminates the stream once
/// the given future resolves.
pub trait StreamExt: Stream {
    /// Take elements from this stream until the given future resolves.
    ///
    /// This function will take elements from this stream until the given future resolves. Once it
    /// resolves, the stream will yield `None`, and produce no further elements.
    ///
    /// If the future produces an error, the stream will be allowed to continue indefinitely.
    ///
    /// ```
    /// # extern crate stream_cancel;
    /// extern crate tokio;
    /// extern crate futures;
    ///
    /// use stream_cancel::StreamExt;
    /// use tokio::prelude::*;
    ///
    /// let listener = tokio::net::TcpListener::bind(&"0.0.0.0:0".parse().unwrap()).unwrap();
    /// let (tx, rx) = futures::sync::oneshot::channel();
    ///
    /// let mut rt = tokio::runtime::Runtime::new().unwrap();
    /// rt.spawn(
    ///     listener
    ///         .incoming()
    ///         .take_until(rx.map_err(|_| ()))
    ///         .map_err(|e| eprintln!("accept failed = {:?}", e))
    ///         .for_each(|sock| {
    ///             let (reader, writer) = sock.split();
    ///             tokio::spawn(
    ///                 tokio::io::copy(reader, writer)
    ///                     .map(|amt| println!("wrote {:?} bytes", amt))
    ///                     .map_err(|err| eprintln!("IO error {:?}", err)),
    ///             )
    ///         }),
    /// );
    ///
    /// // tell the listener to stop accepting new connections
    /// tx.send(()).unwrap();
    /// rt.shutdown_on_idle().wait().unwrap();
    /// ```
    fn take_until<U>(self, until: U) -> TakeUntil<Self, U::Future>
    where
        U: IntoFuture<Item = (), Error = ()>,
        Self: Sized,
    {
        TakeUntil {
            stream: self,
            until: until.into_future(),
            free: false,
        }
    }
}

impl<S> StreamExt for S where S: Stream {}

impl<S, F> Stream for TakeUntil<S, F>
where
    S: Stream,
    F: Future<Item = (), Error = ()>,
{
    type Item = S::Item;
    type Error = S::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        if !self.free {
            match self.until.poll() {
                Ok(Async::Ready(_)) => {
                    // future resolved -- terminate stream
                    return Ok(Async::Ready(None));
                }
                Err(_) => {
                    // future failed -- unclear whether we should stop or continue?
                    // to provide a mechanism for the creator to let the stream run forever,
                    // we interpret this as "run forever".
                    self.free = true;
                }
                Ok(Async::NotReady) => {}
            }
        }

        self.stream.poll()
    }
}
