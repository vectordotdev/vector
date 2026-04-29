//! The Vector Core Library
//!
//! The Vector Core Library are the foundational pieces needed to make a vector
//! and is not vector with pieces missing. While this library is obviously
//! tailored to the needs of vector it is written in such a way to make
//! experimentation and testing _in the library_ cheap and demonstrative.
//!
//! This library was extracted from the top-level project package, discussed in
//! RFC 7027.

#![deny(warnings)]
#![deny(clippy::all)]
#![deny(clippy::pedantic)]
#![deny(unreachable_pub)]
#![deny(unused_allocation)]
#![deny(unused_extern_crates)]
#![deny(unused_assignments)]
#![deny(unused_comparisons)]
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

pub mod config;
pub mod event;
pub mod fanout;
pub mod ipallowlist;
pub mod latency;
pub mod metrics;
pub mod partition;
pub mod schema;
pub mod serde;
pub mod sink;
pub mod source;
pub mod source_sender;
pub mod tcp;
#[cfg(test)]
mod test_util;
pub mod time;
pub mod tls;
pub mod transform;
#[cfg(feature = "vrl")]
pub mod vrl;

use std::path::PathBuf;

pub use event::EstimatedJsonEncodedSizeOf;
use float_eq::FloatEq;

#[cfg(feature = "vrl")]
pub use crate::vrl::compile_vrl;

#[macro_use]
extern crate tracing;

pub fn default_data_dir() -> Option<PathBuf> {
    Some(PathBuf::from("/var/lib/vector/"))
}

pub(crate) use vector_common::{Error, Result};

pub(crate) fn float_eq(l_value: f64, r_value: f64) -> bool {
    (l_value.is_nan() && r_value.is_nan()) || l_value.eq_ulps(&r_value, &1)
}

// These macros aren't actually usable in lib crates without some `vector_lib` shenanigans.
#[macro_export]
macro_rules! emit {
    ($event:expr) => {
        vector_lib::internal_event::emit($event)
    };
}

#[macro_export]
macro_rules! register {
    ($event:expr) => {
        vector_lib::internal_event::register($event)
    };
}

// Re-export `inventory` so `register_extra_metric_label!` can resolve `submit!` through this
// crate without forcing downstream callers to declare `inventory` as a direct dependency.
#[doc(hidden)]
pub use inventory as __inventory;

/// Register an additional metric label that should be preserved on metric keys when it is present
/// as a tracing-span field.
///
/// `VectorLabelFilter` consults a single registry of label keys — Vector's own built-in global
/// labels (`component_id`, `component_type`, `component_kind`, `buffer_type`) plus any registered
/// through this macro. Downstream crates use it to extend the allowlist without modifying
/// Vector — for example, an embedder that owns a "deployment-version" concept of its own can
/// write `register_extra_metric_label!("deployment_version");` once at module scope and any
/// internal metric emitted from inside a span carrying that field will inherit it as a label.
///
/// Registrations are collected at link time via the `inventory` crate, so the read path is
/// lock-free. The expansion goes through this crate's re-exports of `inventory` and
/// [`MetricLabel`](crate::metrics::MetricLabel), so callers do not need a direct `inventory`
/// dependency.
#[macro_export]
macro_rules! register_extra_metric_label {
    ($key:expr) => {
        $crate::__inventory::submit! {
            $crate::metrics::MetricLabel($key)
        }
    };
}
