//! The Vector Core buffer
//!
//! This library implements a channel like functionality, one variant which is
//! solely in-memory and the other that is on-disk. Both variants are bounded.

#![deny(clippy::all)]
#![deny(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::type_complexity)] // long-types happen, especially in async code
#![allow(clippy::must_use_candidate)]

#[macro_use]
extern crate tracing;

mod acker;
mod buffer_usage_data;
pub mod bytes;
mod config;
pub use config::{BufferConfig, BufferType};
#[cfg(feature = "disk-buffer")]
pub mod disk;

pub mod disk_v2;

pub mod helpers;

mod internal_events;
#[cfg(test)]
mod test;
pub mod topology;
mod variant;

use crate::buffer_usage_data::BufferUsageData;
use crate::bytes::{DecodeBytes, EncodeBytes};
pub use acker::{Ackable, Acker};
use core_common::byte_size_of::ByteSizeOf;
use futures::{channel::mpsc, Sink, SinkExt};
use futures::{Stream, StreamExt};
use pin_project::pin_project;
#[cfg(test)]
use quickcheck::{Arbitrary, Gen};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tracing::Span;
pub use variant::*;

pub type BufferStream<T> = Box<dyn Stream<Item = T> + Unpin + Send>;

/// Build a new buffer based on the passed `Variant`
///
/// # Errors
///
/// This function will fail only when creating a new disk buffer. Because of
/// legacy reasons the error is not a type but a `String`.
#[allow(clippy::needless_pass_by_value)]
pub fn build<T>(
    variant: Variant,
    span: Span,
) -> Result<(BufferInputCloner<T>, BufferStream<T>, Acker), String>
where
    T: Bufferable + Clone,
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
            let buffer_usage_data = BufferUsageData::new(when_full, span, Some(max_size), None);
            let (tx, rx, acker) =
                disk::open(&data_dir, &buffer_dir, max_size, buffer_usage_data.clone())
                    .map_err(|error| error.to_string())?;
            let tx = BufferInputCloner::Disk(tx, when_full, buffer_usage_data);
            Ok((tx, rx, acker))
        }
        Variant::Memory {
            max_events,
            when_full,
            instrument,
            ..
        } => {
            let (tx, rx) = mpsc::channel(max_events);
            if instrument {
                let buffer_usage_data =
                    BufferUsageData::new(when_full, span, None, Some(max_events));
                let tx = BufferInputCloner::Memory(tx, when_full, Some(buffer_usage_data.clone()));
                let rx = rx.inspect(move |item: &T| {
                    buffer_usage_data.increment_sent_event_count_and_byte_size(1, item.size_of());
                });

                Ok((tx, Box::new(rx), Acker::Null))
            } else {
                let tx = BufferInputCloner::Memory(tx, when_full, None);

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
    Overflow,
}

impl Default for WhenFull {
    fn default() -> Self {
        WhenFull::Block
    }
}

#[cfg(test)]
impl Arbitrary for WhenFull {
    fn arbitrary(g: &mut Gen) -> Self {
        // TODO: We explicitly avoid generating "overflow" as a possible value because nothing yet
        // supports handling it, and will be defaulted to to using "block" if they encounter
        // "overflow".  Thus, there's no reason to emit it here... yet.
        if bool::arbitrary(g) {
            WhenFull::Block
        } else {
            WhenFull::DropNewest
        }
    }
}

/// An item that can be buffered.
///
/// This supertrait serves as the base trait for any item that can be pushed into a buffer.
pub trait Bufferable:
    ByteSizeOf + EncodeBytes<Self> + DecodeBytes<Self> + Debug + Send + Sync + Unpin + Sized + 'static
{
}

// Blanket implementation for anything that is already bufferable.
impl<T> Bufferable for T where
    T: ByteSizeOf
        + EncodeBytes<Self>
        + DecodeBytes<Self>
        + Debug
        + Send
        + Sync
        + Unpin
        + Sized
        + 'static
{
}

// Clippy warns that the `Disk` variant below is much larger than the
// `Memory` variant (currently 233 vs 25 bytes) and recommends boxing
// the large fields to reduce the total size.
#[allow(clippy::large_enum_variant)]
#[derive(Clone)]
pub enum BufferInputCloner<T>
where
    T: Bufferable + Clone,
{
    Memory(mpsc::Sender<T>, WhenFull, Option<Arc<BufferUsageData>>),
    #[cfg(feature = "disk-buffer")]
    Disk(disk::Writer<T>, WhenFull, Arc<BufferUsageData>),
}

impl<T> BufferInputCloner<T>
where
    T: Bufferable + Clone,
{
    #[must_use]
    pub fn get(&self) -> Box<dyn Sink<T, Error = ()> + Send + Unpin> {
        match self {
            BufferInputCloner::Memory(tx, when_full, buffer_usage_data) => {
                let inner = tx
                    .clone()
                    .sink_map_err(|error| error!(message = "Sender error.", %error));

                Box::new(MemoryBufferInput::new(
                    inner,
                    *when_full,
                    buffer_usage_data.clone(),
                ))
            }

            #[cfg(feature = "disk-buffer")]
            BufferInputCloner::Disk(writer, when_full, buffer_usage_data) => {
                let inner: disk::Writer<T> = (*writer).clone();
                if when_full == &WhenFull::DropNewest {
                    Box::new(DropWhenFull::new(inner, buffer_usage_data.clone()))
                } else {
                    Box::new(inner)
                }
            }
        }
    }
}

#[pin_project]
pub struct MemoryBufferInput<S> {
    #[pin]
    inner: S,
    drop: Option<bool>,
    buffer_usage_data: Option<Arc<BufferUsageData>>,
}

impl<S> MemoryBufferInput<S> {
    pub fn new(
        inner: S,
        when_full: WhenFull,
        buffer_usage_data: Option<Arc<BufferUsageData>>,
    ) -> Self {
        let drop = match when_full {
            WhenFull::Block | WhenFull::Overflow => None,
            WhenFull::DropNewest => Some(false),
        };

        Self {
            inner,
            drop,
            buffer_usage_data,
        }
    }
}

// Instrumenting events received by the memory buffer can be accomplished by
// hooking into the lifecycle of writing events to the buffer, hence the
// MemoryBufferInput wrapper. This is not necessary for disk buffers
// which are instrumented for this at a lower level in their implementation.
impl<T: ByteSizeOf, S: Sink<T> + Unpin> Sink<T> for MemoryBufferInput<S> {
    type Error = S::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let this = self.project();
        if this.drop.is_some() {
            match this.inner.poll_ready(cx) {
                Poll::Ready(Ok(())) => {
                    *this.drop = Some(false);
                    Poll::Ready(Ok(()))
                }
                Poll::Pending => {
                    *this.drop = Some(true);
                    Poll::Ready(Ok(()))
                }
                error @ std::task::Poll::Ready(..) => error,
            }
        } else {
            this.inner.poll_ready(cx)
        }
    }

    fn start_send(self: Pin<&mut Self>, item: T) -> Result<(), Self::Error> {
        if let Some(should_drop) = self.drop {
            if should_drop {
                debug!(
                    message = "Shedding load; dropping event.",
                    internal_log_rate_secs = 10
                );

                if let Some(buffer_usage_data) = &self.buffer_usage_data {
                    buffer_usage_data.try_increment_dropped_event_count(1);
                }
                return Ok(());
            }
        }
        if let Some(buffer_usage_data) = &self.buffer_usage_data {
            buffer_usage_data.increment_received_event_count_and_byte_size(1, item.size_of());
        }
        self.project().inner.start_send(item)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_close(cx)
    }
}

/// DropWhenFull is used by disk buffers to implement dropping behavior and as a
/// point of instrumentation. The MemoryBufferInput wrapper implements dropping
/// behavior so DropWhenFull is no longer needed for memory buffers.
#[pin_project]
pub struct DropWhenFull<S> {
    #[pin]
    inner: S,
    drop: bool,
    buffer_usage_data: Arc<BufferUsageData>,
}

impl<S> DropWhenFull<S> {
    pub fn new(inner: S, buffer_usage_data: Arc<BufferUsageData>) -> Self {
        Self {
            inner,
            drop: false,
            buffer_usage_data,
        }
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
            self.buffer_usage_data.try_increment_dropped_event_count(1);
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
