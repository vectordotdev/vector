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
#![allow(clippy::float_cmp)]
#![allow(clippy::approx_constant)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::default_trait_access)]
#![allow(clippy::enum_glob_use)]
#![allow(clippy::unnested_or_patterns)]
#![allow(clippy::must_use_candidate)]

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
