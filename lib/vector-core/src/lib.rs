//! The Vector Core Library
//!
//! The Vector Core Library are the foundational pieces needed to make a vector
//! and is not vector with pieces missing. While this library is obviously
//! tailored to the needs of vector it is written in such a way to make
//! experimentation and testing _in the library_ cheap and demonstrative.
//!
//! This library was extracted from the top-level project package, discussed in
//! RFC 7027.

#[cfg(feature = "api")]
pub mod api;
pub mod config;
pub mod event;
pub mod mapping;
pub mod metrics;
#[cfg(test)]
mod test_util;

#[macro_use]
extern crate tracing;
#[macro_use]
extern crate pest_derive;

/// Vector's basic error type, dynamically dispatched and safe to send across
/// threads.
pub type Error = Box<dyn std::error::Error + Send + Sync + 'static>;

/// Vector's basic result type, defined in terms of [`Error`] and generic over
/// `T`.
pub type Result<T> = std::result::Result<T, Error>;
