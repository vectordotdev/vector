//! The Vector Core Library
//!
//! The Vector Core Library are the foundational pieces needed to make a vector
//! and is not vector with pieces missing. While this library is obviously
//! tailored to the needs of vector it is written in such a way to make
//! experimentation and testing _in the library_ cheap and demonstrative.
//!
//! This library was extracted from the top-level project package, discussed in
//! RFC 7027.

#![deny(clippy::all)]
#![deny(clippy::pedantic)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::default_trait_access)] // triggers on generated prost code
#![allow(clippy::float_cmp)]
#![allow(clippy::match_wildcard_for_single_variants)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)] // many false positives in this package
#![allow(clippy::non_ascii_literal)] // using unicode literals is a-okay in vector
#![allow(clippy::unnested_or_patterns)] // nightly-only feature as of 1.51.0
#![allow(clippy::type_complexity)] // long-types happen, especially in async code

#[cfg(feature = "api")]
pub mod api;
pub mod config;
pub mod event;
pub mod fanout;
pub mod mapping;
pub mod metrics;
pub mod sink;
pub mod source;
#[cfg(test)]
mod test_util;
pub mod transform;
pub use vector_buffers as buffers;
pub mod partition;
pub mod serde;
pub mod stream;
pub mod time;
use std::path::PathBuf;

#[cfg(any(test, feature = "test"))]
pub use vector_common::event_test_util;
pub use vector_common::{byte_size_of::ByteSizeOf, internal_event};

#[macro_use]
extern crate derivative;
#[macro_use]
extern crate pest_derive;
#[macro_use]
extern crate tracing;

pub fn default_data_dir() -> Option<PathBuf> {
    Some(PathBuf::from("/var/lib/vector/"))
}

/// Vector's basic error type, dynamically dispatched and safe to send across
/// threads.
pub type Error = Box<dyn std::error::Error + Send + Sync + 'static>;

/// Vector's basic result type, defined in terms of [`Error`] and generic over
/// `T`.
pub type Result<T> = std::result::Result<T, Error>;
