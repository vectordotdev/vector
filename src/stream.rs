// The MIT License (MIT)
//
// Copyright (c) 2016 Jon Gjengset
// Copyright (c) 2016 Alex Crichton
// Copyright (c) 2017 The Tokio Authors
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

use futures01::{Async, Future, IntoFuture, Poll, Stream};

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

// NOTE: This construct is a modified copy of `Select` from crate `futures`.
/// An adapter for merging the output of two streams where this stream ends if any
/// of the streams end.
///
/// The merged stream produces items from either of the underlying streams as
/// they become available, and the streams are polled in a round-robin fashion.
/// Errors, however, are not merged: you get at most one error at a time.
#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
pub struct WeakSelect<S1, S2> {
    stream1: S1,
    stream2: S2,
    flag: bool,
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
    /// use tokio01::prelude::*;
    ///
    /// let listener = tokio01::net::TcpListener::bind(&"0.0.0.0:0".parse().unwrap()).unwrap();
    /// let (tx, rx) = futures01::sync::oneshot::channel();
    ///
    /// let mut rt = tokio01::runtime::Runtime::new().unwrap();
    /// rt.spawn(
    ///     listener
    ///         .incoming()
    ///         .take_until(rx.map_err(|_| ()))
    ///         .map_err(|e| eprintln!("accept failed = {:?}", e))
    ///         .for_each(|sock| {
    ///             let (reader, writer) = sock.split();
    ///             tokio01::spawn(
    ///                 tokio01::io::copy(reader, writer)
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

    /// Creates a stream that selects the next element from either this stream
    /// or the provided one, whichever is ready first.
    ///
    /// This combinator will attempt to pull items from both streams. Each
    /// stream will be polled in a round-robin fashion, and whenever a stream is
    /// ready to yield an item that item is yielded.
    ///
    /// The `weak_select` function is similar to `select` except that
    /// the resulting stream will end once any of of the streams end.
    ///
    /// Error are passed through from either stream.
    fn weak_select<S>(self, stream: S) -> WeakSelect<Self, S>
    where
        Self: Sized,
        S: Stream<Item = Self::Item, Error = Self::Error>,
    {
        WeakSelect {
            stream1: self,
            stream2: stream,
            flag: false,
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

impl<S1, S2> Stream for WeakSelect<S1, S2>
where
    S1: Stream,
    S2: Stream<Item = S1::Item, Error = S1::Error>,
{
    type Item = S1::Item;
    type Error = S1::Error;

    fn poll(&mut self) -> Poll<Option<S1::Item>, S1::Error> {
        let (a, b) = if self.flag {
            (
                &mut self.stream2 as &mut dyn Stream<Item = _, Error = _>,
                &mut self.stream1 as &mut dyn Stream<Item = _, Error = _>,
            )
        } else {
            (
                &mut self.stream1 as &mut dyn Stream<Item = _, Error = _>,
                &mut self.stream2 as &mut dyn Stream<Item = _, Error = _>,
            )
        };
        self.flag = !self.flag;

        match a.poll()? {
            // Poll other stream
            Async::NotReady => {
                self.flag = !self.flag;
                b.poll()
            }
            // This stream is ready with an item or has ended.
            ok => Ok(ok),
        }
    }
}
