//! The Vector Core common library
//!
//! This library includes common functionality relied upon by vector-core
//! and core-related crates (e.g. buffers).

#![deny(clippy::all)]
#![deny(clippy::pedantic)]
#![deny(unreachable_pub)]
#![deny(unused_allocation)]
#![deny(unused_extern_crates)]
#![deny(unused_assignments)]
#![deny(unused_comparisons)]

#[cfg(feature = "aws_cloudwatch_logs_subscription")]
pub mod aws_cloudwatch_logs_subscription;

#[cfg(feature = "btreemap")]
pub mod btreemap;

#[cfg(feature = "byte_size_of")]
pub mod byte_size_of;

pub mod config;

#[cfg(feature = "conversion")]
pub mod conversion;
#[cfg(feature = "conversion")]
pub mod datetime;
#[cfg(feature = "conversion")]
pub use datetime::TimeZone;

#[cfg(feature = "encoding")]
pub mod encode_key_value;
#[cfg(feature = "encoding")]
pub mod encode_logfmt;

pub mod event_data_eq;
pub use event_data_eq::EventDataEq;

#[cfg(any(test, feature = "test"))]
pub mod event_test_util;

pub mod finalization;
pub mod finalizer;

pub mod internal_event;

pub mod shutdown;

#[cfg(feature = "tokenize")]
pub mod tokenize;

pub mod trigger;

#[macro_use]
extern crate tracing;
