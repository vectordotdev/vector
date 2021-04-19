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

use futures::{future, stream::Fuse, Stream, StreamExt};
use pin_project::{pin_project, pinned_drop};
use std::{
    pin::Pin,
    task::{Context, Poll},
};

impl<T: ?Sized> VecStreamExt for T where T: Stream {}

pub trait VecStreamExt: Stream {
    /// Creates a stream that selects the next element from either this stream
    /// or the provided one, whichever is ready first.
    ///
    /// This combinator will attempt to pull items from both streams. Each
    /// stream will be polled in a round-robin fashion, and whenever a stream is
    /// ready to yield an item that item is yielded.
    ///
    /// The `select_weak` function is similar to `select` except that
    /// the resulting stream will end once any of of the streams end.
    ///
    /// Error are passed through from either stream.
    fn select_weak<S>(self, stream2: S) -> SelectWeak<Self, S>
    where
        Self: Sized,
        S: Stream<Item = Self::Item>,
    {
        SelectWeak {
            stream1: self.fuse(),
            stream2: stream2.fuse(),
            flag: false,
        }
    }

    /// Calls function F after an item from this stream has been processed
    /// by the consumer while the assumption holds.
    /// This assumes that the stream is being consumed in a loop, so when
    /// the consumer get's an item and then comes for another one
    /// we assume that it has processed the first item.
    fn on_processed<F: Fn()>(self, after: F) -> OnProcessed<Self, F>
    where
        Self: Sized,
    {
        OnProcessed {
            stream: self,
            after,
            processing: false,
        }
    }
}

/// An adapter for merging the output of two streams where this stream ends if any
/// of the streams end.
///
/// The merged stream produces items from either of the underlying streams as
/// they become available, and the streams are polled in a round-robin fashion.
/// Errors, however, are not merged: you get at most one error at a time.
#[pin_project]
pub struct SelectWeak<St1, St2> {
    #[pin]
    stream1: Fuse<St1>,
    #[pin]
    stream2: Fuse<St2>,
    flag: bool,
}

impl<St1, St2> Stream for SelectWeak<St1, St2>
where
    St1: Stream,
    St2: Stream<Item = St1::Item>,
{
    type Item = St1::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<St1::Item>> {
        let this = self.project();
        if *this.flag {
            poll_inner(this.flag, this.stream1, this.stream2, cx)
        } else {
            poll_inner(this.flag, this.stream2, this.stream1, cx)
        }
    }
}

fn poll_inner<St1, St2>(
    flag: &mut bool,
    a: Pin<&mut St1>,
    b: Pin<&mut St2>,
    cx: &mut Context<'_>,
) -> Poll<Option<St1::Item>>
where
    St1: Stream,
    St2: Stream<Item = St1::Item>,
{
    match a.poll_next(cx) {
        Poll::Ready(Some(item)) => {
            // give the other stream a chance to go first next time
            *flag = !*flag;
            Poll::Ready(Some(item))
        }
        Poll::Ready(None) => Poll::Ready(None),
        Poll::Pending => b.poll_next(cx),
    }
}

pub(crate) async fn tripwire_handler(closed: bool) {
    future::poll_fn(|_| {
        if closed {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    })
    .await
}

/// Calls function F after an item from stream has been processed
/// by the consumer.
///
/// This assumes that the stream is being consumed in a loop, so when
/// the consumer get's an item and then comes for another one
/// we assume that it has processed the first item.
/// This should hold in most cases. One example of when this doesn't
/// hold is when the consumer batches items, or if it goes away and
/// does something unrelated to the item which it has already processed.
/// In such cases this construct can still be used, but it won't be accurate.
#[pin_project(PinnedDrop)]
pub struct OnProcessed<S: Stream, F: Fn()> {
    #[pin]
    stream: S,
    after: F,
    processing: bool,
}

impl<S: Stream, F: Fn()> Stream for OnProcessed<S, F> {
    type Item = S::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<S::Item>> {
        let this = self.project();
        if *this.processing {
            *this.processing = false;
            (this.after)();
        }
        match this.stream.poll_next(cx) {
            Poll::Ready(Some(item)) => {
                *this.processing = true;
                Poll::Ready(Some(item))
            }
            poll => poll,
        }
    }
}

#[pinned_drop]
impl<S: Stream, F: Fn()> PinnedDrop for OnProcessed<S, F> {
    fn drop(self: Pin<&mut Self>) {
        let this = self.project();
        if *this.processing {
            *this.processing = false;
            (this.after)();
        }
    }
}
