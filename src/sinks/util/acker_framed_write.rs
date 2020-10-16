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

use crate::buffers::Acker;
use bytes::{Buf, BytesMut};
use futures::{ready, Sink};
use pin_project::pin_project;
use std::{
    collections::VecDeque,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::{self, AsyncWrite};
use tokio_util::codec::Encoder;

const INITIAL_CAPACITY: usize = 8 * 1024;
const BACKPRESSURE_BOUNDARY: usize = INITIAL_CAPACITY;

#[pin_project]
pub struct AckerFramedWrite<T, E> {
    #[pin]
    io: T,
    codec: E,
    acker: Acker,
    on_success: Box<dyn Fn(usize) + Send>,
    buffer: BytesMut,
    sizes: VecDeque<(usize, usize)>,
}

impl<T, E> AckerFramedWrite<T, E> {
    pub fn new(inner: T, encoder: E, acker: Acker, on_success: Box<dyn Fn(usize) + Send>) -> Self {
        Self {
            io: inner,
            codec: encoder,
            acker,
            on_success,
            buffer: BytesMut::with_capacity(INITIAL_CAPACITY),
            sizes: VecDeque::new(),
        }
    }

    // Everything should be considered sent to this call. For safity we consume object.
    pub fn ack_left(self) {
        self.acker.ack(self.sizes.len())
    }
}

impl<I, T, E> Sink<I> for AckerFramedWrite<T, E>
where
    T: AsyncWrite,
    E: Encoder<I>,
{
    type Error = <E as Encoder<I>>::Error;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // If the buffer is already over 8KiB, then attempt to flush it. If after flushing it's
        // *still* over 8KiB, then apply backpressure (reject the send).
        if self.buffer.len() >= BACKPRESSURE_BOUNDARY {
            match self.as_mut().poll_flush(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Ready(Ok(())) => (),
            };

            if self.buffer.len() >= BACKPRESSURE_BOUNDARY {
                return Poll::Pending;
            }
        }
        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, item: I) -> Result<(), Self::Error> {
        let mut pinned = self.project();
        // Additional to encoding save item encoded size
        let length = pinned.buffer.len();
        pinned.codec.encode(item, &mut pinned.buffer)?;
        let byte_size = pinned.buffer.len() - length;
        pinned.sizes.push_back((byte_size, byte_size));
        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        trace!("flushing framed transport");
        let mut pinned = self.project();

        while !pinned.buffer.is_empty() {
            trace!("writing; remaining={}", pinned.buffer.len());

            let buf = &pinned.buffer;
            let mut n = ready!(pinned.io.as_mut().poll_write(cx, &buf))?;

            if n == 0 {
                return Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "failed to \
                     write frame to transport",
                )
                .into()));
            }

            pinned.buffer.advance(n);

            // Ack items
            loop {
                match pinned.sizes.front_mut() {
                    Some(size) if (*size).0 > n => {
                        (*size).0 -= n;
                        break;
                    }
                    Some(size) if (*size).0 <= n => {
                        n -= (*size).0;
                        (pinned.on_success)((*size).1);
                        pinned.acker.ack(1);
                        pinned.sizes.pop_front();
                    }
                    Some(_) | None => break,
                }
            }
        }

        // Try flushing the underlying IO
        ready!(pinned.io.poll_flush(cx))?;

        trace!("framed transport flushed");
        Poll::Ready(Ok(()))
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        ready!(self.as_mut().poll_flush(cx))?;
        ready!(self.project().io.poll_shutdown(cx))?;

        Poll::Ready(Ok(()))
    }
}
