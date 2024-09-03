//! The Vector Core common library
//!
//! This library includes common functionality relied upon by vector-core
//! and core-related crates (e.g. buffers).

#![deny(warnings)]
#![deny(clippy::all)]
#![deny(clippy::pedantic)]
#![deny(unreachable_pub)]
#![deny(unused_allocation)]
#![deny(unused_extern_crates)]
#![deny(unused_assignments)]
#![deny(unused_comparisons)]

#[cfg(feature = "btreemap")]
pub use vrl::btreemap;

#[cfg(feature = "byte_size_of")]
pub mod byte_size_of;

pub mod json_size;

pub mod config;

#[cfg(feature = "conversion")]
pub use vrl::compiler::TimeZone;

#[cfg(feature = "encoding")]
pub mod encode_logfmt {
    pub use vrl::core::encode_logfmt::*;
}

pub mod conversion {
    pub use vrl::compiler::conversion::*;
}

pub mod event_data_eq;
pub use event_data_eq::EventDataEq;

#[cfg(any(test, feature = "test"))]
pub mod event_test_util;

pub mod finalization;
pub mod finalizer;
pub use finalizer::EmptyStream;

pub mod id;

pub mod internal_event;

pub mod request_metadata;

pub mod shutdown;

#[cfg(feature = "sensitive_string")]
pub mod sensitive_string;

pub mod trigger;

#[macro_use]
extern crate tracing;

/// Vector's basic error type, dynamically dispatched and safe to send across
/// threads.
pub type Error = Box<dyn std::error::Error + Send + Sync + 'static>;

/// Vector's basic result type, defined in terms of [`Error`] and generic over
/// `T`.
pub type Result<T> = std::result::Result<T, Error>;
