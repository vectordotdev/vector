use futures::{Async, Future, IntoFuture, Poll, Stream};

/// A stream combinator which takes elements from a stream until a future resolves.
///
/// This structure is produced by the [`StreamExt::take_until`] method.
#[derive(Clone, Debug)]
pub struct TakeUntil<S, F, O> {
    stream: S,
    until: F,
    until_res: Option<O>,
    free: bool,
}

/// A stream combinator which takes a handle to hold onto for the lifetime of the stream.
///
/// This structure is produced by the [`StreamExt::with_handle`] method.
pub struct WithHandle<S, H> {
    stream: S,
    _handle: H,
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
    fn take_until<U, O>(self, until: U) -> TakeUntil<Self, U::Future, O>
    where
        U: IntoFuture<Item = O, Error = ()>,
        Self: Sized,
    {
        TakeUntil {
            stream: self,
            until: until.into_future(),
            until_res: None,
            free: false,
        }
    }

    fn with_handle<H>(self, handle: H) -> WithHandle<Self, H>
    where
        Self: Sized,
    {
        WithHandle {
            stream: self,
            _handle: handle,
        }
    }
}

impl<S> StreamExt for S where S: Stream {}

impl<S, F, O> Stream for TakeUntil<S, F, O>
where
    S: Stream,
    F: Future<Item = O, Error = ()>,
{
    type Item = S::Item;
    type Error = S::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        if !self.free {
            match self.until.poll() {
                Ok(Async::Ready(res)) => {
                    // future resolved -- terminate stream
                    self.until_res = Some(res);
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

impl<S, H> Stream for WithHandle<S, H>
where
    S: Stream,
{
    type Item = S::Item;
    type Error = S::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        self.stream.poll()
    }
}
