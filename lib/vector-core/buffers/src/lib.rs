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
    ByteSizeOf + EncodeBytes<Self> + DecodeBytes<Self> + Send + Sync + Unpin + Sized + 'static
{
}

// Blanket implementation for anything that is already bufferable.
impl<T> Bufferable for T where
    T: ByteSizeOf + EncodeBytes<Self> + DecodeBytes<Self> + Send + Sync + Unpin + Sized + 'static
{
}
