//! The Vector Core buffer
//!
//! This library implements a channel like functionality, one variant which is
//! solely in-memory and the other that is on-disk. Both variants are bounded.

#![deny(warnings)]
#![deny(clippy::all)]
#![deny(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::type_complexity)] // long-types happen, especially in async code
#![allow(clippy::must_use_candidate)]

#[macro_use]
extern crate tracing;

mod buffer_usage_data;

pub mod config;
pub use config::{BufferConfig, BufferType};
use encoding::Encodable;
use vector_config::configurable_component;

pub(crate) use vector_common::Result;

pub mod encoding;

mod internal_events;

#[cfg(test)]
pub mod test;
pub mod topology;

pub(crate) mod variants;

use std::fmt::Debug;

#[cfg(test)]
use quickcheck::{Arbitrary, Gen};
use vector_common::{byte_size_of::ByteSizeOf, finalization::AddBatchNotifier};

/// Event handling behavior when a buffer is full.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WhenFull {
    /// Wait for free space in the buffer.
    ///
    /// This applies backpressure up the topology, signalling that sources should slow down
    /// the acceptance/consumption of events. This means that while no data is lost, data will pile
    /// up at the edge.
    #[default]
    Block,

    /// Drops the event instead of waiting for free space in buffer.
    ///
    /// The event will be intentionally dropped. This mode is typically used when performance is the
    /// highest priority, and it is preferable to temporarily lose events rather than cause a
    /// slowdown in the acceptance/consumption of events.
    DropNewest,

    /// Overflows to the next stage in the buffer topology.
    ///
    /// If the current buffer stage is full, attempt to send this event to the next buffer stage.
    /// That stage may also be configured overflow, and so on, but ultimately the last stage in a
    /// buffer topology must use one of the other handling behaviors. This means that next stage may
    /// potentially be able to buffer the event, but it may also block or drop the event.
    ///
    /// This mode can only be used when two or more buffer stages are configured.
    #[configurable(metadata(docs::hidden))]
    Overflow,
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

/// An item that can be buffered in memory.
///
/// This supertrait serves as the base trait for any item that can be pushed into a memory buffer.
/// It is a relaxed version of `Bufferable` that allows for items that are not `Encodable` (e.g., `Instant`),
/// which is an unnecessary constraint for memory buffers.
pub trait InMemoryBufferable:
    AddBatchNotifier + ByteSizeOf + EventCount + Debug + Send + Sync + Unpin + Sized + 'static
{
}

// Blanket implementation for anything that is already in-memory bufferable.
impl<T> InMemoryBufferable for T where
    T: AddBatchNotifier + ByteSizeOf + EventCount + Debug + Send + Sync + Unpin + Sized + 'static
{
}

/// An item that can be buffered.
///
/// This supertrait serves as the base trait for any item that can be pushed into a buffer.
pub trait Bufferable: InMemoryBufferable + Encodable {}

// Blanket implementation for anything that is already bufferable.
impl<T> Bufferable for T where T: InMemoryBufferable + Encodable {}

pub trait EventCount {
    fn event_count(&self) -> usize;
}

impl<T> EventCount for Vec<T>
where
    T: EventCount,
{
    fn event_count(&self) -> usize {
        self.iter().map(EventCount::event_count).sum()
    }
}

impl<'a, T> EventCount for &'a T
where
    T: EventCount,
{
    fn event_count(&self) -> usize {
        (*self).event_count()
    }
}

#[track_caller]
pub(crate) fn spawn_named<T>(
    task: impl std::future::Future<Output = T> + Send + 'static,
    _name: &str,
) -> tokio::task::JoinHandle<T>
where
    T: Send + 'static,
{
    #[cfg(tokio_unstable)]
    return tokio::task::Builder::new()
        .name(_name)
        .spawn(task)
        .expect("tokio task should spawn");

    #[cfg(not(tokio_unstable))]
    tokio::spawn(task)
}
