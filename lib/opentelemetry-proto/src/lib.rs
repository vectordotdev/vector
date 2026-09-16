#![warn(clippy::pedantic)]

pub mod common;
pub mod logs;
pub mod metrics;
#[allow(warnings)] // Ignore some clippy warnings
pub mod proto;
pub mod spans;
#[cfg(feature = "typed-trace")]
pub mod typed_trace;
