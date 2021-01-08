// Copyright (c) 2016 Alex Crichton
// Copyright (c) 2017 The Tokio Authors

// Permission is hereby granted, free of charge, to any
// person obtaining a copy of this software and associated
// documentation files (the "Software"), to deal in the
// Software without restriction, including without
// limitation the rights to use, copy, modify, merge,
// publish, distribute, sublicense, and/or sell copies of
// the Software, and to permit persons to whom the Software
// is furnished to do so, subject to the following
// conditions:

// The above copyright notice and this permission notice
// shall be included in all copies or substantial portions
// of the Software.

// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
// ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
// TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
// PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
// SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
// CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
// OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
// IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
// DEALINGS IN THE SOFTWARE.

use futures::{ready, stream::Peekable, Sink, SinkExt, Stream, StreamExt};
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::sync::mpsc;

impl<T: ?Sized, Item> VecSinkExt<Item> for T where T: Sink<Item> {}

pub trait VecSinkExt<Item>: Sink<Item> {
    /// A future that completes after the given stream has been fully processed
    /// into the sink, including flushing.
    /// Compare to `SinkExt::send_all` this future accept `Peekable` stream and
    /// do not have own buffer.
    fn send_all_peekable<'a, St>(
        &'a mut self,
        stream: &'a mut Peekable<St>,
    ) -> SendAll<'a, Self, St>
    where
        St: Stream<Item = Item> + Sized,
        Self: Sized,
    {
        SendAll { sink: self, stream }
    }
}

/// Future for the [`send_all_peekable`](VecSinkExt::send_all_peekable) method.
pub struct SendAll<'a, Si, St>
where
    St: Stream,
{
    sink: &'a mut Si,
    stream: &'a mut Peekable<St>,
}

impl<Si, St, Item, Error> Future for SendAll<'_, Si, St>
where
    Si: Sink<Item, Error = Error> + Unpin,
    St: Stream<Item = Item> + Unpin,
{
    type Output = Result<(), Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        loop {
            match Pin::new(&mut *self.stream).as_mut().poll_peek(cx) {
                Poll::Ready(Some(_)) => {
                    ready!(self.sink.poll_ready_unpin(cx))?;
                    let item = match self.stream.poll_next_unpin(cx) {
                        Poll::Ready(Some(item)) => item,
                        _ => panic!("Item should exist after poll_peek succeeds"),
                    };
                    self.sink.start_send_unpin(item)?;
                }
                Poll::Ready(None) => {
                    ready!(self.sink.poll_flush_unpin(cx))?;
                    return Poll::Ready(Ok(()));
                }
                Poll::Pending => {
                    ready!(self.sink.poll_flush_unpin(cx))?;
                    return Poll::Pending;
                }
            }
        }
    }
}

/// Wrapper for mpsc::Sender to turn it into a Sink.
pub struct BoundedSink<T> {
    sender: mpsc::Sender<T>,
}

impl<T> BoundedSink<T> {
    pub fn new(sender: mpsc::Sender<T>) -> Self {
        Self { sender }
    }
}

impl<T> Sink<T> for BoundedSink<T> {
    type Error = ();
    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.sender
            .poll_ready(cx)
            .map_err(|error| error!(message = "Sender error.", %error))
    }
    fn start_send(mut self: Pin<&mut Self>, item: T) -> Result<(), Self::Error> {
        self.sender
            .try_send(item)
            .map_err(|error| error!(message = "Sender error.", %error))
    }
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
    fn poll_close(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
}
