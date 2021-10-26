//! The Vector Core common library
//!
//! This library includes common functionality relied upon by vector-core
//! and core-related crates (e.g. buffers).

#![deny(clippy::all)]
#![deny(clippy::pedantic)]

pub mod byte_size_of;
pub mod internal_event;

#[macro_use]
extern crate tracing;

#[cfg(any(test, feature = "test"))]
pub mod event_test_util;
