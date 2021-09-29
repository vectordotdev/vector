//! The Vector Core buffer
//!
//! This library implements a channel like functionality, one variant which is
//! solely in-memory and the other that is on-disk. Both variants are bounded.

#![deny(clippy::all)]
#![deny(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::type_complexity)] // long-types happen, especially in async code

#[macro_use]
extern crate tracing;

mod acker;
pub mod bytes;
#[cfg(feature = "disk-buffer")]
pub mod disk;
mod internal_events;
#[cfg(test)]
mod test;
mod variant;

use crate::bytes::{DecodeBytes, EncodeBytes};
use crate::internal_events::EventsDropped;
pub use acker::Acker;
use core_common::byte_size_of::ByteSizeOf;
use core_common::internal_event::emit;
use futures::StreamExt;
use futures::channel::mpsc::Receiver;
use futures::{channel::mpsc, Sink, SinkExt, Stream};
use internal_events::{BufferEventsReceived, BufferEventsSent};
use pin_project::pin_project;
#[cfg(test)]
use quickcheck::{Arbitrary, Gen};
use serde::{Deserialize, Serialize};
use tracing::{Instrument, Span};
use std::fmt::{Debug, Display};
use std::pin::Pin;
use std::task::{Context, Poll};
pub use variant::*;

/// Build a new buffer based on the passed `Variant`
///
/// # Errors
///
/// This function will fail only when creating a new disk buffer. Because of
/// legacy reasons the error is not a type but a `String`.
pub fn build<'a, T>(
    variant: Variant,
) -> Result<
    (
        BufferInputCloner<T>,
        Box<dyn Stream<Item = T> + 'a + Unpin + Send>,
        Acker,
    ),
    String,
>
where
    T: 'a + ByteSizeOf + Send + Sync + Unpin + Clone + EncodeBytes<T> + DecodeBytes<T>,
    <T as EncodeBytes<T>>::Error: Debug,
    <T as DecodeBytes<T>>::Error: Debug + Display,
{
    match variant {
        #[cfg(feature = "disk-buffer")]
        Variant::Disk {
            max_size,
            when_full,
            data_dir,
            id,
            span,
        } => {
            let buffer_dir = format!("{}_buffer", id);

            let (tx, rx, acker) =
                disk::open(&data_dir, &buffer_dir, max_size).map_err(|error| error.to_string())?;
            let tx = BufferInputCloner::Disk(tx, when_full, span);
            Ok((tx, rx, acker))
        }
        Variant::Memory {
            max_events,
            when_full,
            span,
        } => {
            let (tx, rx) = mpsc::channel(max_events);
            let span_disabled = span.is_disabled();
            let tx = BufferInputCloner::Memory(tx, when_full, span);
            if span_disabled {
                Ok((tx, Box::new(rx), Acker::Null))
            } else {
                let rx = rx.inspect(|item: &T| {
                    emit(&BufferEventsSent {
                        count: 1,
                        byte_size: item.allocated_bytes(),
                    });
                });
                Ok((tx, Box::new(rx), Acker::Null))
            }
        }
    }
}

#[derive(Deserialize, Serialize, Debug, PartialEq, Copy, Clone)]
#[serde(rename_all = "snake_case")]
pub enum WhenFull {
    Block,
    DropNewest,
}

impl Default for WhenFull {
    fn default() -> Self {
        WhenFull::Block
    }
}

#[cfg(test)]
impl Arbitrary for WhenFull {
    fn arbitrary(g: &mut Gen) -> Self {
        if bool::arbitrary(g) {
            WhenFull::Block
        } else {
            WhenFull::DropNewest
        }
    }
}

// Clippy warns that the `Disk` variant below is much larger than the
// `Memory` variant (currently 233 vs 25 bytes) and recommends boxing
// the large fields to reduce the total size.
#[allow(clippy::large_enum_variant)]
#[derive(Clone)]
pub enum BufferInputCloner<T>
where
    T: ByteSizeOf + Send + Sync + Unpin + Clone + EncodeBytes<T> + DecodeBytes<T>,
    <T as EncodeBytes<T>>::Error: Debug,
    <T as DecodeBytes<T>>::Error: Debug,
{
    Memory(mpsc::Sender<T>, WhenFull, Span),
    #[cfg(feature = "disk-buffer")]
    Disk(disk::Writer<T>, WhenFull, Span),
}

impl<'a, T> BufferInputCloner<T>
where
    T: 'a + ByteSizeOf + Send + Sync + Unpin + Clone + EncodeBytes<T> + DecodeBytes<T>,
    <T as EncodeBytes<T>>::Error: Debug,
    <T as DecodeBytes<T>>::Error: Debug + Display,
{
    #[must_use]
    pub fn get(&self) -> Box<dyn Sink<T, Error = ()> + 'a + Send + Unpin> {
        match self {
            BufferInputCloner::Memory(tx, when_full, span) => {
                let inner = tx
                    .clone()
                    .sink_map_err(|error| error!(message = "Sender error.", %error));
                
                if when_full == &WhenFull::DropNewest {
                    if span.is_disabled() {
                        Box::new(DropWhenFull::new(inner))
                    } else {
                        Box::new(DropWhenFull::new(InstrumentMemoryBuffer::new(inner, span.clone())))
                    }
                } else {
                    if span.is_disabled() {
                        Box::new(inner)
                    } else {
                        Box::new(InstrumentMemoryBuffer::new(inner, span.clone()))
                    }
                }
            }

            #[cfg(feature = "disk-buffer")]
            BufferInputCloner::Disk(writer, when_full, ..) => {
                let inner: disk::Writer<T> = (*writer).clone();
                if when_full == &WhenFull::DropNewest {
                    Box::new(DropWhenFull::new(inner))
                } else {
                    Box::new(inner)
                }
            }
        }
    }
}

#[pin_project]
pub struct DropWhenFull<S> {
    #[pin]
    inner: S,
    drop: bool,
}

impl<S> DropWhenFull<S> {
    pub fn new(inner: S) -> Self {
        Self { inner, drop: false }
    }
}

impl<T: ByteSizeOf, S: Sink<T> + Unpin> Sink<T> for DropWhenFull<S> {
    type Error = S::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let this = self.project();
        match this.inner.poll_ready(cx) {
            Poll::Ready(Ok(())) => {
                *this.drop = false;
                Poll::Ready(Ok(()))
            }
            Poll::Pending => {
                *this.drop = true;
                Poll::Ready(Ok(()))
            }
            error @ std::task::Poll::Ready(..) => error,
        }
    }

    fn start_send(self: Pin<&mut Self>, item: T) -> Result<(), Self::Error> {
        if self.drop {
            debug!(
                message = "Shedding load; dropping event.",
                internal_log_rate_secs = 10
            );
            emit(&EventsDropped { count: 1 });
            Ok(())
        } else {
            self.project().inner.start_send(item)
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_close(cx)
    }
}

// Instrumenting events received by the memory buffer can be accomplished by
// hooking into the lifecycle of writing events to the buffer, hence the
// InstrumentMemoryBuffer wrapper. This is not necessary for disk buffers
// which are instrumented for this at a lower level in their implementation.
#[pin_project]
pub struct InstrumentMemoryBuffer<S> {
    #[pin]
    inner: S,
    span: Span,
}

impl<S> InstrumentMemoryBuffer<S> {
    pub fn new(inner: S, span: Span) -> Self {
        Self { inner, span }
    }
}

impl<T: ByteSizeOf, S: Sink<T> + Unpin> Sink<T> for InstrumentMemoryBuffer<S> {
    type Error = S::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_ready(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: T) -> Result<(), Self::Error> {
        let span = self.span.clone();
        let _guard = span.enter();
        let byte_size = item.allocated_bytes();
        self.project().inner.start_send(item).map(|()| {
            emit(&BufferEventsReceived {
                count: 1,
                byte_size,
            });
        })
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_close(cx)
    }
}
