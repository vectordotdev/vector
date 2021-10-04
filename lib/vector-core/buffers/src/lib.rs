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
use futures::{channel::mpsc, Sink, SinkExt, Stream};
use internal_events::{BufferEventsReceived, BufferEventsSent};
use pin_project::pin_project;
#[cfg(test)]
use quickcheck::{Arbitrary, Gen};
use serde::{Deserialize, Serialize};
use std::fmt::{Debug, Display};
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::time::interval;
use tracing::{Instrument, Span};
pub use variant::*;

pub struct BufferUsageData {
    received_event_count: AtomicUsize,
    received_event_byte_size: AtomicUsize,
    sent_event_count: AtomicUsize,
    sent_event_byte_size: AtomicUsize,
    dropped_event_count: Option<AtomicUsize>,
}

impl BufferUsageData {
    fn new(dropped_event_count: Option<AtomicUsize>) -> Self {
        Self {
            received_event_count: AtomicUsize::new(0),
            received_event_byte_size: AtomicUsize::new(0),
            sent_event_count: AtomicUsize::new(0),
            sent_event_byte_size: AtomicUsize::new(0),
            dropped_event_count,
        }
    }

    fn init_instrumentation(buffer_usage_data: &Arc<BufferUsageData>, span: Span) {
        let buffer_usage_data = buffer_usage_data.clone();
        tokio::spawn(
            async move {
                let mut interval = interval(Duration::from_secs(2));
                loop {
                    interval.tick().await;

                    emit(&BufferEventsReceived {
                        count: buffer_usage_data
                            .received_event_count
                            .load(Ordering::Relaxed),
                        byte_size: buffer_usage_data
                            .received_event_byte_size
                            .load(Ordering::Relaxed),
                    });

                    emit(&BufferEventsSent {
                        count: buffer_usage_data.sent_event_count.load(Ordering::Relaxed),
                        byte_size: buffer_usage_data
                            .sent_event_byte_size
                            .load(Ordering::Relaxed),
                    });

                    if let Some(dropped_event_count) = &buffer_usage_data.dropped_event_count {
                        emit(&EventsDropped {
                            count: dropped_event_count.load(Ordering::Relaxed),
                        });
                    }
                }
            }
            .instrument(span),
        );
    }
}

/// Build a new buffer based on the passed `Variant`
///
/// # Errors
///
/// This function will fail only when creating a new disk buffer. Because of
/// legacy reasons the error is not a type but a `String`.
pub fn build<'a, T>(
    variant: Variant,
    span: Span,
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
            ..
        } => {
            let buffer_dir = format!("{}_buffer", id);

            let (tx, rx, acker) = disk::open(&data_dir, &buffer_dir, max_size, span)
                .map_err(|error| error.to_string())?;
            let tx = BufferInputCloner::Disk(tx, when_full);
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
                let dropped_event_count = match when_full {
                    WhenFull::Block => None,
                    WhenFull::DropNewest => Some(AtomicUsize::new(0)),
                };

                let buffer_usage_data = Arc::new(BufferUsageData::new(dropped_event_count));
                BufferUsageData::init_instrumentation(&buffer_usage_data, span);
                let tx = BufferInputCloner::Memory(tx, when_full, Some(buffer_usage_data.clone()));
                let rx = rx.inspect(move |item: &T| {
                    buffer_usage_data
                        .sent_event_count
                        .fetch_add(1, Ordering::Relaxed);
                    buffer_usage_data
                        .sent_event_byte_size
                        .fetch_add(item.size_of(), Ordering::Relaxed);
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
    Memory(mpsc::Sender<T>, WhenFull, Option<Arc<BufferUsageData>>),
    #[cfg(feature = "disk-buffer")]
    Disk(disk::Writer<T>, WhenFull),
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
            BufferInputCloner::Disk(writer, when_full) => {
                let inner: disk::Writer<T> = (*writer).clone();
                if when_full == &WhenFull::DropNewest {
                    Box::new(DropWhenFull::new(inner, true))
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
            WhenFull::Block => None,
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
// InstrumentMemoryBuffer wrapper. This is not necessary for disk buffers
// which are instrumented for this at a lower level in their implementation.
impl<T: ByteSizeOf, S: Sink<T> + Unpin> Sink<T> for MemoryBufferInput<S> {
    type Error = S::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let this = self.project();
        if let Some(_) = this.drop {
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

                if let Some(buf_usage_data) = &self.buffer_usage_data {
                    if let Some(dropped_event_count) = &buf_usage_data.dropped_event_count {
                        dropped_event_count.fetch_add(1, Ordering::Relaxed);
                    }
                }
                return Ok(());
            }
        }
        if let Some(buf_usage_data) = &self.buffer_usage_data {
            buf_usage_data
                .received_event_count
                .fetch_add(1, Ordering::Relaxed);
            buf_usage_data
                .received_event_byte_size
                .fetch_add(item.size_of(), Ordering::Relaxed);
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

#[pin_project]
pub struct DropWhenFull<S> {
    #[pin]
    inner: S,
    drop: bool,
    instrument: bool,
}

impl<S> DropWhenFull<S> {
    pub fn new(inner: S, instrument: bool) -> Self {
        Self {
            inner,
            drop: false,
            instrument,
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
            if self.instrument {
                emit(&EventsDropped { count: 1 });
            }
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
