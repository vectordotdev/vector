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
use futures::StreamExt;
use futures::{channel::mpsc, Sink, SinkExt, Stream};
use core_common::internal_event::emit;
use internal_events::{EventsReceived, EventsSent};
use pin_project::pin_project;
#[cfg(test)]
use quickcheck::{Arbitrary, Gen};
use serde::{Deserialize, Serialize};
use std::fmt::{Debug, Display};
use std::mem::size_of_val;
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
    T: 'a + Send + Sync + Unpin + Clone + EncodeBytes<T> + DecodeBytes<T>,
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
            ..
        } => {
            let buffer_dir = format!("{}_buffer", id);

            let (tx, rx, acker) =
                disk::open(&data_dir, &buffer_dir, max_size).map_err(|error| error.to_string())?;
            let tx = BufferInputCloner::Disk(tx, when_full);
            Ok((tx, rx, acker))
        }
        Variant::Memory {
            max_events,
            when_full,
        } => {
            let (tx, rx) = mpsc::channel(max_events);
            let tx = BufferInputCloner::Memory(tx, when_full);
            let rx = rx.inspect(|item| {
                emit(&EventsSent {
                    count: 1,
                    byte_size: size_of_val(item),
                });
            });
            let rx = Box::new(rx);
            Ok((tx, rx, Acker::Null))
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
    T: Send + Sync + Unpin + Clone + EncodeBytes<T> + DecodeBytes<T>,
    <T as EncodeBytes<T>>::Error: Debug,
    <T as DecodeBytes<T>>::Error: Debug,
{
    Memory(mpsc::Sender<T>, WhenFull),
    #[cfg(feature = "disk-buffer")]
    Disk(disk::Writer<T>, WhenFull),
}

impl<'a, T> BufferInputCloner<T>
where
    T: 'a + Send + Sync + Unpin + Clone + EncodeBytes<T> + DecodeBytes<T>,
    <T as EncodeBytes<T>>::Error: Debug,
    <T as DecodeBytes<T>>::Error: Debug + Display,
{
    #[must_use]
    pub fn get(&self) -> Box<dyn Sink<T, Error = ()> + 'a + Send + Unpin> {
        match self {
            BufferInputCloner::Memory(tx, when_full) => {
                let inner = tx
                    .clone()
                    .sink_map_err(|error| error!(message = "Sender error.", %error));
                if when_full == &WhenFull::DropNewest {
                    Box::new(DropWhenFull::new(inner))
                } else {
                    Box::new(BlockWhenFull::new(inner))
                }
            }

            #[cfg(feature = "disk-buffer")]
            BufferInputCloner::Disk(writer, when_full) => {
                let inner: disk::Writer<T> = (*writer).clone();
                if when_full == &WhenFull::DropNewest {
                    Box::new(DropWhenFull::new(inner))
                } else {
                    Box::new(BlockWhenFull::new(inner))
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

impl<T, S: Sink<T> + Unpin> Sink<T> for DropWhenFull<S> {
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
            let byte_size = size_of_val(&item);
            self.project().inner.start_send(item).map(|()| {
                emit(&EventsReceived {
                    count: 1,
                    byte_size,
                });
            })
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_close(cx)
    }
}

// The BlockWhenFull wrapper is used for instrumentation purposes
#[pin_project]
pub struct BlockWhenFull<S> {
    #[pin]
    inner: S,
}

impl<S> BlockWhenFull<S> {
    pub fn new(inner: S) -> Self {
        Self { inner }
    }
}

impl<T, S: Sink<T> + Unpin> Sink<T> for BlockWhenFull<S> {
    type Error = S::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_ready(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: T) -> Result<(), Self::Error> {
        let byte_size = size_of_val(&item);
        self.project().inner.start_send(item).map(|()| {
            emit(&EventsReceived {
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
